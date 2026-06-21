use crate::mq::mqtt::mqtt_config::MqttConfig;
use crate::mq::mqtt::mqtt_error::MqttError;
use log::{debug, error};
use rumqttc::{AsyncClient, MqttOptions, Publish, SubscribeFilter};
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::sleep;

/// # 启动MQTT订阅者
pub async fn start_mqtt_subscriber<F, Fut>(
    mqtt_config: MqttConfig,
    do_received: F,
) -> Result<(Arc<AsyncClient>, Arc<JoinHandle<()>>), MqttError>
where
    F: Fn(Publish) -> Fut + 'static + Send,
    Fut: Future<Output = Result<(), MqttError>> + 'static + Send,
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
        handle_error_sleep,
        ack_error_sleep,
    } = mqtt_config;
    let mut mqtt_options = MqttOptions::new(client_id, host, port);
    mqtt_options.set_keep_alive(keep_alive);
    mqtt_options.set_clean_session(clean_session);
    mqtt_options.set_credentials(username, password);
    mqtt_options.set_manual_acks(true);

    let (mqtt_client, mut mqtt_event_loop) = AsyncClient::new(mqtt_options, cap);
    let mqtt_client = Arc::new(mqtt_client);
    mqtt_client
        .subscribe_many(
            topic
                .iter()
                .map(|topic| SubscribeFilter::new(topic.clone(), qos.clone())),
        )
        .await?;

    let mqtt_client_clone = mqtt_client.clone();
    let mqtt_event_loop_handle = tokio::spawn(async move {
        loop {
            match mqtt_event_loop.poll().await {
                Ok(notification) => {
                    match notification {
                        rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish)) => {
                            debug!("收到MQTT消息: {:?}", publish);
                            match do_received(publish.clone()).await {
                                Ok(_) => {
                                    if let Err(e) = mqtt_client_clone.ack(&publish).await {
                                        error!("应答MQTT消息失败: {:?}", e);
                                        sleep(ack_error_sleep).await;
                                    }
                                }
                                Err(e) => {
                                    error!("处理MQTT消息失败: {:?}", e);
                                    sleep(handle_error_sleep).await;
                                }
                            }
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
    Ok((mqtt_client, Arc::new(mqtt_event_loop_handle)))
}
