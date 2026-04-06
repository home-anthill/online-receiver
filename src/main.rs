use std::time::Duration;

use paho_mqtt::Message;
use redis::aio::ConnectionManager;
use rocket::{self, catchers, routes};
use tracing::{error, info, warn};

use online::catchers as app_catchers;
use online::config::{AppEnv, Env, init};
use online::models::notification::Notification;
use online::models::payload_trait::OnlineMqttPayload;
use online::mqtt::get_string_payload;
use online::mqtt::mqtt_client::MqttClient;
use online::mqtt::mqtt_config::MqttConfig;
use online::mqtt::mqtt_options::MqttOptions;
use online::routes as app_routes;

const TOPICS: &[&str] = &["online/+/features/+"];

#[rocket::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), rocket::Error> {
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
    let mqtt_options = MqttOptions::new(&mqtt_config).expect("failed to build MQTT options");
    let mut mqtt_client = MqttClient::new(mqtt_options).expect("failed to create MQTT client");
    mqtt_client.connect().await;
    mqtt_client.subscribe(TOPICS).await.expect("failed to subscribe to MQTT topics");

    // 4. Spawn the MQTT event loop as a background task
    let mqtt_handle = tokio::task::spawn(async move {
        info!(target: "app", "Waiting for incoming MQTT messages");
        while let Some(msg_opt) = mqtt_client.get_next_message().await {
            let _ = process_mqtt_message(&msg_opt, &mut mqtt_client, &con).await.inspect_err(|err| {
                error!(target: "app", "process_mqtt_message - failed to process MQTT message: {:?}", err);
            });
        }
    });

    // 5. Launch Rocket for the health endpoint
    info!(target: "app", "Starting Rocket...");
    let _rocket = rocket::build()
        .mount("/", routes![app_routes::api::keep_alive])
        .register(
            "/",
            catchers![
                app_catchers::bad_request,
                app_catchers::not_found,
                app_catchers::internal_server_error,
                app_catchers::service_unavailable,
            ],
        )
        .launch()
        .await?;

    mqtt_handle.abort();
    Ok(())
}

async fn process_mqtt_message(
    msg_opt: &Option<Message>,
    mqtt_client: &mut MqttClient,
    con: &ConnectionManager,
) -> Result<(), anyhow::Error> {
    if let Some(msg) = msg_opt {
        use online::db::online::insert_or_update_online as db_update;
        let payload_str = get_string_payload(msg)?;
        let notification: Notification<OnlineMqttPayload> = serde_json::from_str(&payload_str)?;
        db_update(con, &notification.api_token, &notification.device_uuid, &notification.feature_uuid)
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
