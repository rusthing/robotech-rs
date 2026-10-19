//! # ID 生成器（雪花算法）的框架级初始化
//!
//! 雪花算法的前提是每个生成者拥有互不相同的 `node`。手工在配置文件里写死
//! `node` 在多副本部署时必然撞号，因此这里提供统一的初始化入口：
//!
//! - `node = Some(n)`：视为显式指定，直接使用；
//! - `node = None`（配置里不写 `node`）：视为自动分配，启动时从注册中心发现
//!   同服务的健康实例，读取其实例 metadata 里的 `id-node`，取最小的空闲编号
//!   （范围 `0..=2^node_bits-1`，0 号也是合法编号）；
//! - 分配结果写入实例 metadata，后续启动的实例据此避让；
//! - 首次注册成功后再做一次冲突检查：若发现同号，按 `instance_id` 字典序决定
//!   胜者，败者重新选号并重建生成器。
//!
//! node 在进程内只确定一次：配置热更新沿用首次分配的结果，因为重建生成器会把
//! sequence 归零，同一个时间窗口内可能产生重号。
//!
//! ## 已知窗口
//!
//! 两个副本同时冷启动时，双方都可能先看到"没有已占用的 node"而选中同一个号。
//! 冲突检查会在首次注册后数秒内发现并纠正，但纠正前的这一小段时间里，
//! 双方生成的 ID 可能重复。要求强一致的场景请在配置里显式指定 `node`，
//! 或让副本错峰启动。

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

    warn!("id worker node 回退为 0：多副本部署时会生成重复 ID，请检查注册中心配置");
    Ok(0)
}

/// 从未占用的编号里挑最小的一个。
///
/// 0 是合法编号，同样参与分配；`exclude` 用于重新选号时排除当前值。
fn pick_node(occupied: &[u8], max: u8, exclude: Option<u8>) -> Option<u8> {
    (0..=max).find(|node| Some(*node) != exclude && !occupied.contains(node))
}

/// 从注册中心发现同服务实例，返回最小的空闲 node。
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
/// 两个副本同时冷启动时可能选中同一个 node，这里在首次注册成功后（延迟数秒，
/// 等对端的注册可见）再查一次：
/// - 自动分配模式：同号的实例按 `instance_id` 字典序排序，排序靠后的一方重新选号；
/// - 显式配置模式：只告警，不改配置（显式指定意味着运维自己负责）。
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
            // 注册中心可能比本进程晚就绪（或启动瞬间网络抖动），
            // 因此按递增间隔重试；检查成功后立即退出。
            for delay in [3u64, 12, 30] {
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                match resolve_conflict().await {
                    Ok(()) => return,
                    Err(e) => warn!("id worker node 冲突检查失败（稍后重试）: {e:?}"),
                }
            }
        }
    });
}

/// 检查并纠正同号冲突。
#[cfg(feature = "registry-center")]
async fn resolve_conflict() -> Result<(), AppError> {
    let Some(node) = assigned_id_node() else {
        return Ok(());
    };
    let hub_client = crate::micro_svc::get_hub_client()?;
    let Some(svc_name) = hub_client.registry_svc_name() else {
        return Ok(());
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
        return Ok(());
    }

    if !AUTO_ALLOCATE.load(Ordering::Acquire) {
        warn!(
            "id worker node {node} 与实例 {same_node:?} 重复：配置中显式指定的 node 必须在\
             同一服务的所有副本间唯一，否则会生成重复 ID"
        );
        return Ok(());
    }

    // 所有同号实例按 instance_id 排序，最小者保留该 node
    let mut candidates = same_node.clone();
    candidates.push(me.as_str());
    candidates.sort_unstable();
    if candidates.first() == Some(&me.as_str()) {
        info!("id worker node {node} 与实例 {same_node:?} 同号，按字典序本实例保留该编号");
        return Ok(());
    }

    warn!("id worker node {node} 与实例 {same_node:?} 冲突，重新分配");
    reallocate(node).await
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
    fn pick_smallest_free_node() {
        assert_eq!(pick_node(&[], 15, None), Some(0));
        assert_eq!(pick_node(&[0], 15, None), Some(1));
        assert_eq!(pick_node(&[0, 1, 2], 15, None), Some(3));
        assert_eq!(pick_node(&[1], 15, None), Some(0));
    }

    #[test]
    fn zero_is_a_valid_node() {
        // 0 号是合法编号，可以被分配，也可以被显式使用
        assert_eq!(pick_node(&[], 1, None), Some(0));
        assert_eq!(pick_node(&[0], 1, None), Some(1));
        assert_eq!(pick_node(&[0], 1, None).map(|n| n as u16), Some(1));
        assert_eq!(max_node(0), 0);
        assert_eq!(pick_node(&[0], 0, None), None);
    }

    #[test]
    fn pick_node_excludes_current() {
        assert_eq!(pick_node(&[0], 15, Some(0)), Some(1));
        assert_eq!(pick_node(&[0, 1], 15, Some(2)), Some(3));
    }

    #[test]
    fn full_range_returns_none() {
        let all: Vec<u8> = (0..=15).collect();
        assert_eq!(pick_node(&all, 15, None), None);
    }

    #[test]
    fn sentinel_is_not_a_valid_node() {
        // 255 是 node_bits = 8 时的合法编号，不能被当作"未分配"
        assert_eq!(u8::try_from(NODE_UNASSIGNED).ok(), None);
        assert_eq!(u8::try_from(max_node(8) as i16).ok(), Some(255));
    }
}
