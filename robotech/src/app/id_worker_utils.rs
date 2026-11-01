//! # ID 生成器（雪花算法）的框架级初始化
//!
//! 雪花算法的前提是每个生成者拥有互不相同的 `node`。手工在配置文件里写死
//! `node` 在多副本部署时必然撞号，因此这里提供统一的初始化入口：
//!
//! - `node = Some(n)`：视为显式指定，直接使用；
//! - `node = None`（配置里不写 `node`）：视为自动分配，启动时从注册中心发现
//!   同服务的健康实例，读取其实例 metadata 里的 `id-node`，从本实例的哈希起点
//!   环形扫描第一个空闲编号（范围 `0..=2^node_bits-1`，0 号也是合法编号）；
//! - 分配结果写入实例 metadata，后续启动的实例据此避让；
//! - 首次注册成功后再做一次冲突检查：若发现同号，按 `instance_id` 字典序决定
//!   胜者，败者重新选号并重建生成器。
//!
//! node 在进程内只确定一次：配置热更新沿用首次分配的结果，因为重建生成器会把
//! sequence 归零，同一个时间窗口内可能产生重号。
//!
//! ## 并发冷启动
//!
//! 两个副本同时冷启动时，双方都可能先看到"没有已占用的 node"。因此选号起点由
//! `instance_id` 哈希打散（见 [`node_start`]），只有哈希落到同一位置才会撞，概率
//! 为 1/号池（默认 `node_bits = 6`，即 1/64）；而固定从 0 起扫时是**必然**撞。
//!
//! 撞上之后由 [`spawn_id_node_conflict_check`] 纠正：首档 300ms 就开始检查，
//! 之后按 1s / 3s / 12s / 30s 重试。纠正前这段时间里双方生成的 ID 仍可能重复，
//! 要绝对避免重号，只能显式指定 `node`，或让副本错峰启动。

use crate::app::AppError;
use config::Value;
use idworker::IdWorkerConfig;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI16, Ordering};
use tracing::{info, warn};

/// 实例 metadata 中记录 worker node 的键名。
///
/// 注册中心（Consul / etcd / Nacos）会把 `ServiceInstance.metadata` 原样透传，
/// 因此该键对所有后端都可见。
pub const ID_NODE_METADATA_KEY: &str = "id-node";

/// `node` 的未分配哨兵值。用 i16 是为了让 255（`node_bits = 8` 时的合法编号）
/// 不被误判为"未分配"。
const NODE_UNASSIGNED: i16 = -1;

/// 进程内已确定的 worker node。
static ASSIGNED_NODE: AtomicI16 = AtomicI16::new(NODE_UNASSIGNED);

/// 是否处于自动分配模式（配置里没有写 `node`）。
static AUTO_ALLOCATE: AtomicBool = AtomicBool::new(false);

/// 首次初始化时的配置，冲突重分配时用来重建生成器。
static ID_WORKER_CONFIG: OnceLock<IdWorkerConfig> = OnceLock::new();

/// 当前进程已确定的 worker node；尚未初始化时为 `None`。
///
/// 注册中心注册实例时会把该值写入 metadata，供其它实例避让。
pub fn assigned_id_node() -> Option<u8> {
    u8::try_from(ASSIGNED_NODE.load(Ordering::Acquire)).ok()
}

/// # 初始化 / 更新 ID 生成器
///
/// 替代直接调用 `idworker::setup_id_worker`：先确定本实例的 `node`，
/// 再交给 idworker 创建生成器。
///
/// ## 参数
/// - `id_worker_config`：`[id-worker]` 段的配置
/// - `changed`：配置热更新的差异集合，`None` 表示首次初始化
///
/// ## 错误
/// 生成器创建失败（配置非法等）时返回 [`AppError`]。
pub async fn setup_id_worker(
    id_worker_config: IdWorkerConfig,
    changed: &Option<HashMap<String, Value>>,
) -> Result<(), AppError> {
    let mut config = id_worker_config;
    match assigned_id_node() {
        // 热更新：node 只确定一次。重建生成器会让 sequence 归零，
        // 同一个时间窗口内可能产生重复 ID，因此这里强制沿用首次结果。
        Some(node) => {
            if config.node != Some(node) {
                info!(
                    "id worker node 已确定为 {node}，忽略配置中的 {:?}",
                    config.node
                );
                config.node = Some(node);
            }
        }
        None => {
            let node = resolve_node(&config).await?;
            ASSIGNED_NODE.store(node as i16, Ordering::Release);
            config.node = Some(node);
            let _ = ID_WORKER_CONFIG.set(config.clone());
        }
    }
    idworker::setup_id_worker(config, changed)?;
    Ok(())
}

/// 计算 `node_bits` 能容纳的最大 node 编号。
fn max_node(node_bits: u8) -> u8 {
    // node_bits 上限由 idworker 校验（<= 8），(1 << 8) - 1 = 255
    if node_bits >= 8 {
        u8::MAX
    } else {
        ((1u16 << node_bits) - 1) as u8
    }
}

/// 确定本实例的 worker node。
///
/// - `node = Some(n)`：显式指定，直接用（越界时由这里报错——serde 反序列化路径
///   不会走 idworker 的校验）；
/// - `node = None`：自动分配，可用编号为 `0..=max`。
async fn resolve_node(config: &IdWorkerConfig) -> Result<u8, AppError> {
    let max = max_node(config.node_bits);

    if let Some(node) = config.node {
        if node > max {
            return Err(AppError::IdWorkerNode(format!(
                "配置的 node {node} 超出 node-bits={} 的范围（0-{max}）",
                config.node_bits
            )));
        }
        info!("id worker node 使用配置值 {node}（可用范围 0-{max}）");
        return Ok(node);
    }

    AUTO_ALLOCATE.store(true, Ordering::Release);
    match allocate_node(max).await {
        Ok(Some(node)) => {
            info!("id worker node 自动分配为 {node}（同服务可用范围 0-{max}）");
            return Ok(node);
        }
        Ok(None) => warn!("同服务实例已占满 0-{max} 号 node，无法自动分配"),
        Err(e) => warn!("id worker node 自动分配失败: {e:?}"),
    }

    warn!(
        "id worker node 回退为 0：单进程部署时正确，多副本部署会生成重复 ID；\
         请检查注册中心配置，或显式指定 node"
    );
    Ok(0)
}

/// 从本实例的起点开始环形扫描，挑第一个未被占用的编号。
///
/// 0 是合法编号，同样参与分配；`exclude` 用于重新选号时排除当前值。
fn pick_node(occupied: &[u8], max: u8, exclude: Option<u8>) -> Option<u8> {
    pick_node_from(occupied, max, exclude, node_start(max))
}

/// 与 [`pick_node`] 相同，但起点由调用方给出（便于测试不同起点下的回绕行为）。
fn pick_node_from(occupied: &[u8], max: u8, exclude: Option<u8>, start: usize) -> Option<u8> {
    // max = 255 时 span = 256，不能用 u8 承载
    let span = max as usize + 1;
    let start = start % span;
    (0..span)
        .map(|offset| ((start + offset) % span) as u8)
        .find(|node| Some(*node) != exclude && !occupied.contains(node))
}

/// 本实例的选号起点。
///
/// 用 `instance_id` 的哈希取模，把各副本的扫描起点打散。固定从 0 起扫时，两个副本
/// 同时冷启动会**必然**选中同一个号（双方都看到"没人占号"→ 都拿 0）；打散之后
/// 只有哈希落到同一个起点才会撞，概率降到 1/号池。
///
/// `instance_id` 在 `setup_id_worker` 阶段是 ULID（Web 服务器尚未启动，还没换成
/// `{app}-{ip}-{port}`），每次启动都不同，因此起点等价于随机。
fn node_start(max: u8) -> usize {
    let instance_id = crate::env::get_app_instance_id();
    (fnv1a_64(instance_id.as_bytes()) % ((max as usize + 1) as u64)) as usize
}

/// FNV-1a 64 位哈希。只用于把实例 ID 摊到号池上，不做安全用途。
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 从注册中心发现同服务实例，返回从本实例起点起算的第一个空闲 node。
#[cfg(feature = "registry-center")]
async fn allocate_node(max: u8) -> Result<Option<u8>, AppError> {
    let hub_client = crate::micro_svc::get_hub_client()?;
    let svc_name = hub_client.registry_svc_name().ok_or_else(|| {
        AppError::IdWorkerNode("注册中心未配置服务名（micro-svc.svc-name）".to_string())
    })?;
    let occupied = occupied_nodes(&hub_client, &svc_name).await?;
    info!("同服务已占用的 worker node: {occupied:?}");
    Ok(pick_node(&occupied, max, None))
}

/// 未启用注册中心时无法自动分配。
#[cfg(not(feature = "registry-center"))]
async fn allocate_node(_max: u8) -> Result<Option<u8>, AppError> {
    Err(AppError::IdWorkerNode(
        "未启用 registry-center feature，无法自动分配 node".to_string(),
    ))
}

/// 收集同服务其它实例已占用的 node 编号。
#[cfg(feature = "registry-center")]
async fn occupied_nodes(
    hub_client: &crate::micro_svc::HubClient,
    svc_name: &str,
) -> Result<Vec<u8>, AppError> {
    let me = crate::env::get_app_instance_id();
    let instances = hub_client.discover(svc_name, None, None).await?;
    let mut nodes: Vec<u8> = instances
        .iter()
        .filter(|instance| instance.instance_id != me)
        .filter_map(|instance| instance.metadata.get(ID_NODE_METADATA_KEY))
        .filter_map(|value| value.parse::<u8>().ok())
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    Ok(nodes)
}

/// # 启动首次注册后的 node 冲突检查
///
/// 两个副本同时冷启动时可能选中同一个 node，这里在注册前后按递增档位反复检查
/// （300ms / 1s / 3s / 12s / 30s，等对端的注册可见）：
/// - 自动分配模式：同号的实例按 `instance_id` 字典序排序，排序靠后的一方重新选号；
/// - 显式配置模式：只告警，不改配置（显式指定意味着运维自己负责）。
///
/// 首档不能太晚：原来从 3 秒才开始查，对高频写服务已经是几十万条重号。
/// 也不能"查一次没看到同号就收工"——300ms 时对端很可能还没注册完。
///
/// 该函数自身立即返回，检查在后台任务中完成。
pub fn spawn_id_node_conflict_check() {
    if assigned_id_node().is_none() {
        return;
    }
    // 没有注册中心就没有"同号实例"这个概念，也不该刷告警
    #[cfg(feature = "registry-center")]
    match crate::micro_svc::get_hub_client() {
        Ok(hub_client) if hub_client.has_registry() => {}
        _ => return,
    }
    tokio::spawn(async move {
        #[cfg(feature = "registry-center")]
        {
            // 注册中心可能比本进程晚就绪（或启动瞬间网络抖动），因此按递增档位重试。
            let marks = [300u64, 1_000, 3_000, 12_000, 30_000];
            let started = tokio::time::Instant::now();
            for (idx, at_ms) in marks.iter().enumerate() {
                let is_last = idx + 1 == marks.len();
                tokio::time::sleep_until(started + std::time::Duration::from_millis(*at_ms))
                    .await;
                match resolve_conflict().await {
                    // 已经有结论（处理过同号，或显式配置只告警）
                    Ok(true) => return,
                    // 本档没看到同号：可能只是对端还没注册，继续等下一档
                    Ok(false) if is_last => return,
                    Ok(false) => {}
                    Err(e) => warn!("id worker node 冲突检查失败（稍后重试）: {e:?}"),
                }
            }
        }
    });
}

/// 检查并纠正同号冲突。
///
/// 返回 `true` 表示这一轮已有结论（处理完同号，或没有可查的对象），调用方可以停止
/// 后续档位；返回 `false` 表示**本轮没看到同号**——这通常只是对端还没注册完成，
/// 需要留到下一档再查，不能当成"无冲突"而提前收工。
#[cfg(feature = "registry-center")]
async fn resolve_conflict() -> Result<bool, AppError> {
    let Some(node) = assigned_id_node() else {
        return Ok(true);
    };
    let hub_client = crate::micro_svc::get_hub_client()?;
    let Some(svc_name) = hub_client.registry_svc_name() else {
        return Ok(true);
    };
    let me = crate::env::get_app_instance_id();
    let instances = hub_client.discover(&svc_name, None, None).await?;

    let same_node: Vec<&str> = instances
        .iter()
        .filter(|instance| {
            instance
                .metadata
                .get(ID_NODE_METADATA_KEY)
                .and_then(|value| value.parse::<u8>().ok())
                == Some(node)
        })
        .map(|instance| instance.instance_id.as_str())
        .filter(|instance_id| *instance_id != me)
        .collect();

    if same_node.is_empty() {
        return Ok(false);
    }

    if !AUTO_ALLOCATE.load(Ordering::Acquire) {
        warn!(
            "id worker node {node} 与实例 {same_node:?} 重复：配置中显式指定的 node 必须在\
             同一服务的所有副本间唯一，否则会生成重复 ID"
        );
        return Ok(true);
    }

    // 所有同号实例按 instance_id 排序，最小者保留该 node
    let mut candidates = same_node.clone();
    candidates.push(me.as_str());
    candidates.sort_unstable();
    if candidates.first() == Some(&me.as_str()) {
        info!("id worker node {node} 与实例 {same_node:?} 同号，按字典序本实例保留该编号");
        return Ok(true);
    }

    warn!("id worker node {node} 与实例 {same_node:?} 冲突，重新分配");
    reallocate(node).await?;
    Ok(true)
}

/// 重新选号并重建生成器。
#[cfg(feature = "registry-center")]
async fn reallocate(current: u8) -> Result<(), AppError> {
    let Some(config) = ID_WORKER_CONFIG.get().cloned() else {
        return Ok(());
    };
    let max = max_node(config.node_bits);
    let hub_client = crate::micro_svc::get_hub_client()?;
    let Some(svc_name) = hub_client.registry_svc_name() else {
        return Ok(());
    };
    let occupied = occupied_nodes(&hub_client, &svc_name).await?;

    let Some(node) = pick_node(&occupied, max, Some(current)) else {
        warn!("没有可用的 node 空位，保持 {current}；请扩大 node-bits 或减少副本数");
        return Ok(());
    };

    let mut new_config = config;
    new_config.node = Some(node);
    idworker::setup_id_worker(new_config, &None)?;
    ASSIGNED_NODE.store(node as i16, Ordering::Release);
    warn!(
        "id worker node 已从 {current} 改为 {node}；重建生成器会重置 sequence，\
         启动初期的少量 ID 存在重号风险"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_node_matches_bits() {
        assert_eq!(max_node(0), 0);
        assert_eq!(max_node(4), 15);
        assert_eq!(max_node(6), 63);
        assert_eq!(max_node(8), u8::MAX);
    }

    #[test]
    fn pick_from_start() {
        // 起点未被占用时直接从起点开始，而不是从 0
        assert_eq!(pick_node_from(&[], 15, None, 3), Some(3));
        // 起点被占用则顺延
        assert_eq!(pick_node_from(&[3, 4], 15, None, 3), Some(5));
        // 起点超出号池按池宽取模
        assert_eq!(pick_node_from(&[], 15, None, 20), Some(4));
        // 从末尾起扫要能回绕到 0
        assert_eq!(pick_node_from(&[15], 15, None, 15), Some(0));
    }

    #[test]
    fn zero_is_a_valid_node() {
        // 0 号是合法编号，可以被分配，也可以被显式使用
        assert_eq!(pick_node_from(&[], 1, None, 0), Some(0));
        assert_eq!(pick_node_from(&[0], 1, None, 0), Some(1));
        assert_eq!(max_node(0), 0);
        assert_eq!(pick_node_from(&[0], 0, None, 0), None);
    }

    #[test]
    fn pick_node_excludes_current() {
        assert_eq!(pick_node_from(&[0], 15, Some(0), 0), Some(1));
        assert_eq!(pick_node_from(&[0, 1], 15, Some(2), 3), Some(3));
    }

    #[test]
    fn full_range_returns_none() {
        let all: Vec<u8> = (0..=15).collect();
        assert_eq!(pick_node_from(&all, 15, None, 7), None);
    }

    /// 两个副本同时冷启动、都看到空表时，不同起点必须拿到不同的号——
    /// 这是把「必然撞 0」变成「1/号池 才撞」的核心。
    #[test]
    fn different_starts_do_not_collide() {
        assert_eq!(pick_node_from(&[], 63, None, 0), Some(0));
        assert_eq!(pick_node_from(&[], 63, None, 17), Some(17));
    }

    #[test]
    fn fnv1a_is_stable() {
        // 标准 FNV-1a 64 位测试向量
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn node_start_fits_range() {
        for max in [0u8, 1, 15, 63, 255] {
            assert!(node_start(max) <= max as usize);
        }
    }

    #[test]
    fn sentinel_is_not_a_valid_node() {
        // 255 是 node_bits = 8 时的合法编号，不能被当作"未分配"
        assert_eq!(u8::try_from(NODE_UNASSIGNED).ok(), None);
        assert_eq!(u8::try_from(max_node(8) as i16).ok(), Some(255));
    }
}
