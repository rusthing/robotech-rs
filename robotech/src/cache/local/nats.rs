//! # NATS 本地缓存
//!
//! 基于 `ArcSwap` 的无锁本地内存缓存 + NATS 消息同步。
//!
//! ## 工作原理
//!
//! 1. **存储**：每个 `NatsCacheHandler` 对应一个独立缓存实例，存储在全局 `RwLock<HashMap>` 中
//! 2. **同步**：通过 NATS Core Subscription 订阅指定主题，接收其他节点的变更事件
//! 3. **发布**：`publish_cache_event` 将 CacheEvent 广播到指定主题，其他节点收到后调用 `apply_event` 精确更新
//!
//! ## 使用示例
//!
//! ```ignore
//! use robotech::cache::local::nats::{NatsCacheHandler, publish_cache_event};
//! use robotech::cache::CacheEvent;
//!
//! #[derive(Default, Clone)]
//! struct MyCache { data: HashMap<u64, Item> }
//!
//! impl NatsCacheHandler for MyCache {
//!     fn topics() -> &'static [&'static str] { &["my.cache.topic"] }
//!     fn apply_event(&mut self, topic: &str, event: &CacheEvent) { ... }
//! }
//!
//! // 启动
//! NatsCache::<MyCache>::setup().await?;
//! NatsCache::<MyCache>::refresh(full_cache);
//!
//! // 读取
//! let snapshot = NatsCache::<MyCache>::get();
//!
//! // 发布变更
//! publish_cache_event("my.cache.topic", &CacheEvent::Add(data)).await?;
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};

use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::cache::CacheEvent;
use crate::mq::nats::{publish as nats_publish, subscribe as nats_subscribe};

type CacheMap = RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>;

fn cache_map() -> &'static CacheMap {
    use std::sync::OnceLock;
    static MAP: OnceLock<CacheMap> = OnceLock::new();
    MAP.get_or_init(|| RwLock::new(HashMap::new()))
}

type HandleMap = std::sync::Mutex<HashMap<TypeId, Vec<Arc<JoinHandle<()>>>>>;

fn handle_map() -> &'static HandleMap {
    use std::sync::OnceLock;
    static MAP: OnceLock<HandleMap> = OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// # 缓存精确更新处理器
///
/// 业务方实现此 trait 以定义收到 NATS 消息后如何精确更新缓存。
///
/// ## 必要约束
///
/// - `Default`：未初始化时的默认空缓存
/// - `Clone`：`update()` 时复制现有缓存
/// - `Send + Sync + 'static`：跨线程共享
pub trait NatsCacheHandler: Default + Clone + Send + Sync + 'static {
    /// 在 rcu 闭包中精确更新缓存。
    ///
    /// 根据 `topic` 和 `event` 对 `self` 做原地修改，调用者负责原子替换。
    fn apply_event(&mut self, topic: &str, event: &CacheEvent);

    /// 需要订阅的 NATS 主题列表。
    fn topics() -> &'static [&'static str];
}

/// # NATS 本地缓存管理器
///
/// 泛型 `H` 必须实现 [`NatsCacheHandler`]。
///
/// 通过静态方法操作缓存。同一类型 `H` 只有一个全局缓存实例。
pub struct NatsCache<H: NatsCacheHandler>(PhantomData<H>);

impl<H: NatsCacheHandler> NatsCache<H> {
    /// # 启动 NATS 订阅
    ///
    /// 遍历 [`NatsCacheHandler::topics`] 中的每个主题启动 Core NATS 订阅。
    /// 收到消息后自动反序列化为 [`CacheEvent`] 并调用 [`Self::update`]。
    ///
    /// 应在应用启动时调用一次，通常由 `bootstrap!` 宏自动接管。
    ///
    /// ## 参数
    ///
    /// 无。
    ///
    /// ## 返回值
    ///
    /// 成功时缓存即处于就绪状态（为空），需随后调用 [`Self::refresh`] 做全量加载。
    pub async fn setup() -> anyhow::Result<()> {
        let topics = H::topics();
        if topics.is_empty() {
            return Ok(());
        }

        let mut handles = Vec::new();

        for topic in topics {
            let handle = nats_subscribe(
                topic,
                None,
                None,
                None,
                |msg| async move {
                    let topic = msg.subject.to_string();
                    let payload = serde_json::from_slice::<CacheEvent>(&msg.payload);
                    match payload {
                        Ok(event) => {
                            NatsCache::<H>::update(|cache| cache.apply_event(&topic, &event));
                            Ok(())
                        }
                        Err(e) => {
                            error!("反序列化缓存事件失败 (topic={topic}): {e}");
                            Ok(())
                        }
                    }
                },
            )
            .await?;

            info!("NATS cache subscriber started for topic: {topic}");
            handles.push(handle);
        }

        handle_map()
            .lock()
            .unwrap()
            .insert(TypeId::of::<H>(), handles);

        Ok(())
    }

    /// # 获取缓存快照
    ///
    /// 返回当前缓存的共享只读副本。若未初始化则返回默认空值。
    ///
    /// ## 性能
    ///
    /// 使用 `RwLock::read` 获取快照，并发读不互斥。退出锁后持有独立的 `Arc`。
    pub fn get() -> Arc<H> {
        let map = cache_map().read().unwrap();
        map.get(&TypeId::of::<H>())
            .and_then(|arc| Arc::downcast::<H>(arc.clone()).ok())
            .unwrap_or_else(|| Arc::new(H::default()))
    }

    /// # 全量替换缓存
    ///
    /// 启动时或收到全量刷新信号时调用，直接交换整个缓存实例。
    pub fn refresh(cache: H) {
        cache_map()
            .write()
            .unwrap()
            .insert(TypeId::of::<H>(), Arc::new(cache));
    }

    /// # 原子 rcu 更新
    ///
    /// 复制当前缓存，原地执行修改闭包，再原子替换。
    ///
    /// 在 NATS 消息处理中调用，保证单线程顺序执行（NATS 消息天然有序）。
    pub fn update(f: impl FnOnce(&mut H)) {
        let mut map = cache_map().write().unwrap();
        let current = map
            .get(&TypeId::of::<H>())
            .and_then(|arc| Arc::downcast::<H>(arc.clone()).ok())
            .unwrap_or_else(|| Arc::new(H::default()));

        let mut new_cache = (*current).clone();
        f(&mut new_cache);
        map.insert(TypeId::of::<H>(), Arc::new(new_cache));
    }
}

/// # 发布缓存变更事件
///
/// 将 [`CacheEvent`] 通过 Core NATS 发布到指定主题。
/// 通常用于 SVC 的 `after_*` 回调中，通知其他节点更新缓存。
///
/// ## 参数
///
/// - `topic`：NATS 主题
/// - `event`：缓存变更事件
///
/// ## 返回值
///
/// 成功返回 `Ok(())`，失败返回错误。
pub async fn publish_cache_event(topic: &str, event: &CacheEvent) -> anyhow::Result<()> {
    nats_publish(topic, event, false).await?;
    Ok(())
}