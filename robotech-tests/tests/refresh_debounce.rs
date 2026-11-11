//! # 刷新信号的合并窗口
//!
//! `refresh::subscribe` 收到信号后不立即执行动作，而是按 `[refresh]` 的
//! `debounce-window` / `max-wait` 合并：窗口内多条信号只执行一次，持续写入
//! 也不超过 `max-wait` 才更新一次。本用例连真实 NATS JetStream 验四件事：
//!
//! | 场景 | 断言点 |
//! |---|---|
//! | 默认窗口 3s / 15s | 一批 5 条信号全部静默后才执行**一次**，执行前计数为 0 |
//! | 持续写入 | 首条信号起算不超过 `max-wait` 就执行，整批被执行多次（不是只刷一次） |
//! | `debounce-window = "0s"` | 关闭合并，退回「每条信号立即执行」 |
//! | 执行后不再补执行 | 窗口清空后没有残留信号触发第二次执行 |
//!
//! ## 怎么跑
//!
//! 需要一个可达且开启 JetStream 的 NATS（用例会建 `refresh.probe.*` 三个
//! 独立 Stream，跑完删除；认证可选）：
//!
//! ```text
//! ROBOTECH_TEST_NATS_URL='nats://127.0.0.1:4222' \
//! ROBOTECH_TEST_NATS_USER=ruser \
//! ROBOTECH_TEST_NATS_PASSWORD='********' \
//!   cargo test -p robotech-tests --test refresh_debounce -- --nocapture
//! ```
//!
//! 没设 `ROBOTECH_TEST_NATS_URL` 时测试**跳过**并打印提示。
//!
//! 用例共用一个 NATS 连接与全局刷新配置（后者是进程级单例），所以三个场景
//! 串行放在同一个测试函数里，用不同主题隔离，避免并行互相改配置。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use robotech::mq::nats::{NatsConfig, get_jetstream_context, setup_nats_client};
use robotech::refresh::{self, RefreshConfig, RefreshSubject, set_config};
use tokio::time::{Instant, sleep};

/// 一次执行记录：次数 + 首次执行距开始的时间
struct Probe {
    base: Instant,
    count: AtomicUsize,
    first_ms: AtomicU64,
}

impl Probe {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            base: Instant::now(),
            count: AtomicUsize::new(0),
            first_ms: AtomicU64::new(u64::MAX),
        })
    }

    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// 尚未执行过时返回 `None`
    fn first_ms(&self) -> Option<u64> {
        match self.first_ms.load(Ordering::SeqCst) {
            u64::MAX => None,
            ms => Some(ms),
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.base.elapsed().as_millis() as u64
    }
}

/// 订阅一个主题，把每次执行记到 `Probe` 上
async fn 订阅(subject: &RefreshSubject, probe: Arc<Probe>) -> anyhow::Result<()> {
    refresh::subscribe(subject, move || {
        let probe = Arc::clone(&probe);
        async move {
            let ms = probe.elapsed_ms();
            let _ = probe.first_ms.compare_exchange(
                u64::MAX,
                ms,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            probe.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    })
    .await?;
    Ok(())
}

/// 按固定间隔连发 `times` 条信号，返回真正发出去的条数
async fn 连发(subject: &RefreshSubject, times: usize, gap: Duration) -> anyhow::Result<usize> {
    for _ in 0..times {
        refresh::publish(subject).await?;
        sleep(gap).await;
    }
    Ok(times)
}

/// 等到计数达到 `target`，超时则 panic 并带上实际值
async fn 等到(probe: &Probe, target: usize, limit: Duration) -> u64 {
    let started = Instant::now();
    loop {
        if probe.count() >= target {
            return started.elapsed().as_millis() as u64;
        }
        assert!(
            started.elapsed() < limit,
            "{limit:?} 内只执行了 {} 次（期望 {target} 次）",
            probe.count()
        );
        sleep(Duration::from_millis(20)).await;
    }
}

/// 关掉兜底重建，只留信号驱动，避免 60s 兜底干扰计数
fn 配置(debounce: Duration, max_wait: Duration) {
    set_config(RefreshConfig {
        probe_interval: Duration::from_secs(15),
        rebuild_interval: Duration::ZERO,
        debounce_window: debounce,
        max_wait,
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 刷新信号按窗口合并() -> anyhow::Result<()> {
    let Some(url) = std::env::var("ROBOTECH_TEST_NATS_URL").ok() else {
        println!("未设置 ROBOTECH_TEST_NATS_URL，跳过刷新合并联调");
        return Ok(());
    };

    let config = NatsConfig {
        servers: url.split(',').map(str::to_string).collect(),
        user: std::env::var("ROBOTECH_TEST_NATS_USER").ok(),
        password: std::env::var("ROBOTECH_TEST_NATS_PASSWORD").ok(),
        retry_on_initial_connect: Some(false),
        connection_timeout_ms: Some(5_000),
        ..Default::default()
    };
    setup_nats_client(config, &None).await?;

    let merge = RefreshSubject::from_static("refresh.probe.merge");
    let cap = RefreshSubject::from_static("refresh.probe.cap");
    let immediate = RefreshSubject::from_static("refresh.probe.immediate");

    let result = 跑三个场景(&merge, &cap, &immediate).await;

    // 用自己的三个 Stream 收尾，不动业务主题
    清理(&[&merge, &cap, &immediate]).await;
    result
}

async fn 跑三个场景(
    merge: &RefreshSubject,
    cap: &RefreshSubject,
    immediate: &RefreshSubject,
) -> anyhow::Result<()> {
    // ── 场景 1：默认窗口 3s / 15s，一批 5 条只执行一次 ──
    配置(Duration::from_secs(3), Duration::from_secs(15));
    let probe = Probe::new();
    订阅(merge, Arc::clone(&probe)).await?;

    连发(merge, 5, Duration::from_millis(100)).await?;
    assert_eq!(probe.count(), 0, "窗口内不应该执行，收到信号只登记");

    等到(&probe, 1, Duration::from_secs(10)).await;
    let first = probe.first_ms().expect("执行过就应有首次时间");
    // 5 条发完约 0.5s，窗口从最后一条起算 3s
    assert!(
        first >= 3_000,
        "首次执行应在最后一条信号静默 3s 之后，实际 {first}ms"
    );
    // 多等一会儿，确认没有残留信号触发第二次执行
    sleep(Duration::from_secs(2)).await;
    assert_eq!(probe.count(), 1, "一批信号只应执行一次，实际 {}", probe.count());
    println!("场景 1（3s 窗口合并 5 条）：首次执行 {}ms，共 1 次", first);

    // ── 场景 2：持续写入不能突破 max-wait ──
    // 窗口 1s、上限 2s，每 300ms 一条发 4.5s：若不设上限，末条之后才会刷，
    // 实际应在首条信号起算 2s 处强制刷一次，之后窗口重新开始
    配置(Duration::from_secs(1), Duration::from_secs(2));
    let probe = Probe::new();
    订阅(cap, Arc::clone(&probe)).await?;

    连发(cap, 15, Duration::from_millis(300)).await?;
    let first = probe.first_ms().expect("持续写入期间应至少执行一次");
    assert!(
        first <= 2_500,
        "max-wait=2s，首次执行不应晚于 2.5s（含调度误差），实际 {first}ms"
    );
    assert!(
        probe.count() >= 2,
        "持续写入 4.5s、上限 2s，至少应执行 2 次，实际 {}",
        probe.count()
    );
    println!(
        "场景 2（1s 窗口 / 2s 上限，持续写入 4.5s）：首次执行 {}ms，共 {} 次",
        first,
        probe.count()
    );

    // ── 场景 3：0s 关闭合并，退回立即执行 ──
    配置(Duration::ZERO, Duration::from_secs(15));
    let probe = Probe::new();
    订阅(immediate, Arc::clone(&probe)).await?;

    sleep(Duration::from_millis(300)).await;
    refresh::publish(immediate).await?;
    等到(&probe, 1, Duration::from_secs(3)).await;
    let first = probe.first_ms().expect("立即模式下应当执行");
    assert!(
        first < 1_000,
        "0s 关闭合并不该等 3s 窗口，实际 {first}ms"
    );
    println!("场景 3（0s 关闭合并）：首次执行 {}ms", first);

    Ok(())
}

/// 删除本用例建的 Stream（只碰 `refresh.probe.*`）
async fn 清理(subjects: &[&RefreshSubject]) {
    let js = match get_jetstream_context().await {
        Ok(js) => js,
        Err(e) => {
            println!("取得 JetStream 上下文失败，跳过清理: {e}");
            return;
        }
    };
    for subject in subjects {
        let name = subject.stream_name();
        match js.delete_stream(&name).await {
            Ok(_) => println!("已删除测试 Stream: {name}"),
            Err(e) => println!("删除测试 Stream {name} 失败（可忽略）: {e}"),
        }
    }
}
