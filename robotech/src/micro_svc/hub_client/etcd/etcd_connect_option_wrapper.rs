// etcd_config.rs
use etcd_client::ConnectOptions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EtcdConnectOptionsWrapper(pub ConnectOptions);

impl Serialize for EtcdConnectOptionsWrapper {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // ConnectOptions 字段都是私有的，这里只能序列化为一个空对象或用默认值
        // 如果需要真正序列化每个字段，见下方的远程定义方案
        use serde::ser::SerializeStruct;
        let s = serializer.serialize_struct("ConnectOptions", 0)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for EtcdConnectOptionsWrapper {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            user: Option<(String, String)>,
            #[serde(default)]
            keep_alive: Option<(u64, u64)>, // 用秒表示
            #[serde(default)]
            keep_alive_while_idle: Option<bool>,
            #[serde(default)]
            timeout: Option<u64>,
            #[serde(default)]
            connect_timeout: Option<u64>,
            #[serde(default)]
            tcp_keepalive: Option<u64>,
            #[serde(default)]
            require_leader: Option<bool>,
        }
        let helper = Helper::deserialize(deserializer)?;
        let mut opts = ConnectOptions::default();
        if let Some((user, password)) = helper.user {
            opts = opts.with_user(user, password);
        }
        if let Some((interval, timeout)) = helper.keep_alive {
            opts =
                opts.with_keep_alive(Duration::from_secs(interval), Duration::from_secs(timeout));
        }
        if let Some(true) = helper.keep_alive_while_idle {
            opts = opts.with_keep_alive_while_idle(true);
        }
        if let Some(secs) = helper.timeout {
            opts = opts.with_timeout(Duration::from_secs(secs));
        }
        if let Some(secs) = helper.connect_timeout {
            opts = opts.with_connect_timeout(Duration::from_secs(secs));
        }
        if let Some(secs) = helper.tcp_keepalive {
            opts = opts.with_tcp_keepalive(Duration::from_secs(secs));
        }
        if let Some(true) = helper.require_leader {
            opts = opts.with_require_leader(true);
        }
        Ok(EtcdConnectOptionsWrapper(opts))
    }
}

impl Default for EtcdConnectOptionsWrapper {
    fn default() -> Self {
        EtcdConnectOptionsWrapper(ConnectOptions::default())
    }
}

impl From<ConnectOptions> for EtcdConnectOptionsWrapper {
    fn from(opts: ConnectOptions) -> Self {
        EtcdConnectOptionsWrapper(opts)
    }
}

impl From<EtcdConnectOptionsWrapper> for ConnectOptions {
    fn from(wrapper: EtcdConnectOptionsWrapper) -> Self {
        wrapper.0
    }
}
