use std::time::Duration;

use paho_mqtt::Message;
use redis::aio::ConnectionManager;
use tracing::{debug, error, info, warn};

use online::config::{AppEnv, Env, init};
use online::db::online::insert_or_update_online;
use online::models::notification::Notification;
use online::models::payload_trait::OnlineMqttPayload;
use online::mqtt::get_string_payload;
use online::mqtt::mqtt_client::MqttClient;
use online::mqtt::mqtt_config::MqttConfig;
use online::mqtt::mqtt_options::MqttOptions;

const TOPICS: &[&str] = &["online/+/features/+"];

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Init logger and env
    let (env, _app_env): (Env, AppEnv) = init();

    // 2. Init and connect to Redis
    // If credentials are configured, inject them into the URI:
    //   redis://host:port -> redis://username:password@host:port
    if !env.redis_username.is_empty() && env.redis_password.is_empty() {
        warn!(target: "app", "REDIS_USERNAME is set but REDIS_PASSWORD is empty — no authentication will be attempted");
    }
    let redis_url = if env.redis_password.is_empty() {
        env.redis_uri.clone()
    } else {
        match env.redis_uri.find("://") {
            Some(scheme_end) => format!(
                "{scheme}{username}:{password}@{rest}",
                scheme = &env.redis_uri[..scheme_end + 3],
                username = urlencoding::encode(&env.redis_username),
                password = urlencoding::encode(&env.redis_password),
                rest = &env.redis_uri[scheme_end + 3..],
            ),
            None => {
                warn!(target: "app", "REDIS_URI has no recognizable scheme (missing '://'), skipping credential injection");
                env.redis_uri.clone()
            }
        }
    };
    let redis_client = redis::Client::open(redis_url).expect("invalid Redis URI");
    let con: ConnectionManager = redis_client.get_connection_manager().await.expect("failed to connect to Redis");

    // 3. Init and connect to MQTT
    info!(target: "app", "Initializing MQTT...");
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    let mqtt_options = MqttOptions::new(&mqtt_config)?;
    let mut mqtt_client = MqttClient::new(mqtt_options)?;
    mqtt_client.connect().await;
    mqtt_client.subscribe(TOPICS).await?;

    // 4. Wait for incoming MQTT messages
    info!(target: "app", "Waiting for incoming MQTT messages");
    while let Some(msg_opt) = mqtt_client.get_next_message().await {
        let _ = process_mqtt_message(&msg_opt, &mut mqtt_client, &con).await.inspect_err(|err| {
            error!(target: "app", "process_mqtt_message - failed to process MQTT message: {:?}", err);
        });
    }
    Ok(())
}

async fn process_mqtt_message(
    msg_opt: &Option<Message>,
    mqtt_client: &mut MqttClient,
    con: &ConnectionManager,
) -> Result<(), anyhow::Error> {
    if let Some(msg) = msg_opt {
        debug!(target: "app", "process_mqtt_message - MQTT message received");
        let payload_str = get_string_payload(msg)?;
        let notification: Notification<OnlineMqttPayload> = serde_json::from_str(&payload_str)?;
        insert_or_update_online(con, &notification.api_token, &notification.device_uuid, &notification.feature_uuid)
            .await
            .inspect_err(|err| {
                error!(target: "app", "process_mqtt_message - cannot insert/update online in db, err = {:?}", err);
            })?;
        Ok(())
    } else {
        // msg_opt="None" means we were disconnected. Try to reconnect...
        warn!(target: "app", "process_mqtt_message - Lost connection. Attempting reconnect in 5 seconds...");
        while let Err(err) = mqtt_client.reconnect().await {
            error!(target: "app", "process_mqtt_message - Error reconnecting: {:?}, retrying in 5 seconds...", err);
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        Ok(())
    }
}
