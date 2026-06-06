use crate::mq::mqtt::mqtt_config::MqttConfig;
use crate::mq::mqtt::mqtt_error::MqttError;
use log::{debug, error};
use rumqttc::{AsyncClient, MqttOptions, Publish, SubscribeFilter};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::sleep;

/// # 启动MQTT订阅者
pub async fn start_mqtt_subscriber<F>(
    mqtt_config: MqttConfig,
    do_received: F,
) -> Result<(Arc<AsyncClient>, Arc<JoinHandle<()>>), MqttError>
where
    F: Fn(Publish) + 'static + Send,
{
    let MqttConfig {
        client_id,
        host,
        port,
        keep_alive,
        clean_session,
        username,
        password,
        cap,
        topic,
        qos,
        reconnect_interval,
    } = mqtt_config;
    let mut mqtt_options = MqttOptions::new(client_id, host, port);
    mqtt_options.set_keep_alive(keep_alive);
    mqtt_options.set_clean_session(clean_session);
    mqtt_options.set_credentials(username, password);

    let (mqtt_client, mut mqtt_event_loop) = AsyncClient::new(mqtt_options, cap);
    mqtt_client
        .subscribe_many(
            topic
                .iter()
                .map(|topic| SubscribeFilter::new(topic.clone(), qos.clone())),
        )
        .await?;

    let mqtt_event_loop_handle = tokio::spawn(async move {
        loop {
            match mqtt_event_loop.poll().await {
                Ok(notification) => {
                    match notification {
                        rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish)) => {
                            debug!("收到MQTT消息: {:?}", publish);
                            do_received(publish);
                        }
                        rumqttc::Event::Outgoing(_) => {} // 忽略出站消息
                        rumqttc::Event::Incoming(_) => {} // 忽略其他入站事件
                    }
                }
                Err(e) => {
                    error!("MQTT连接失败: {:?}", e);
                    sleep(reconnect_interval).await;
                }
            }
        }
    });
    Ok((Arc::new(mqtt_client), Arc::new(mqtt_event_loop_handle)))
}
