use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, KeyInit, Mac};
use mongodb::Database;
use paho_mqtt::Message;
use redis::aio::ConnectionManager;
use rocket::{self, catchers, routes};
use serde_json::Value;
use sha2::Sha256;
use tracing::{error, info, warn};

use online::catchers as app_catchers;
use online::config::{AppEnv, Env, init};
use online::models::notification::Notification;
use online::mqtt::get_string_payload;
use online::mqtt::mqtt_client::MqttClient;
use online::mqtt::mqtt_config::MqttConfig;
use online::mqtt::mqtt_options::MqttOptions;
use online::routes as app_routes;

const TOPICS: &[&str] = &["online/+/features/+"];
const SIGNED_MESSAGE_MAX_SKEW_SECS: i64 = 300;
const SIGNED_REPLAY_CACHE_TTL_SECS: usize = 720;
const SIGNED_NONCE_HEX_LEN: usize = 32;
const SIGNED_SIGNATURE_HEX_LEN: usize = 64;

type HmacSha256 = Hmac<Sha256>;

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_signed_envelope(notification: &Notification<Value>) -> Result<(), anyhow::Error> {
    if notification.timestamp <= 0 {
        anyhow::bail!("timestamp must be positive");
    }
    if !is_lower_hex(&notification.nonce, SIGNED_NONCE_HEX_LEN) {
        anyhow::bail!("nonce must be 32 lowercase hex characters");
    }
    if !is_lower_hex(&notification.signature, SIGNED_SIGNATURE_HEX_LEN) {
        anyhow::bail!("signature must be 64 lowercase hex characters");
    }
    Ok(())
}

fn verify_hmac(secret: &str, message: &[u8], expected_hex: &str) -> bool {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(message);
    match hex::decode(expected_hex) {
        Ok(expected_bytes) => mac.verify_slice(&expected_bytes).is_ok(),
        Err(_) => {
            let _ = mac.finalize();
            false
        }
    }
}

fn build_signed_mqtt_payload(notification: &Notification<Value>) -> Result<String, anyhow::Error> {
    let payload_json = serde_json::to_string(&notification.payload)?;
    Ok(format!(
        "{}\n{}\nonline\n{}\n{}\n{}",
        notification.device_uuid, notification.feature_uuid, notification.timestamp, notification.nonce, payload_json
    ))
}

fn verify_mqtt_signature(api_token: &str, notification: &Notification<Value>) -> Result<(), anyhow::Error> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    if (now - notification.timestamp).abs() > SIGNED_MESSAGE_MAX_SKEW_SECS {
        anyhow::bail!("message timestamp is outside the allowed freshness window");
    }
    let signed_payload = build_signed_mqtt_payload(notification)?;
    if !verify_hmac(api_token, signed_payload.as_bytes(), &notification.signature) {
        anyhow::bail!("invalid signed MQTT payload");
    }
    Ok(())
}

fn signed_replay_key(device_uuid: &str, feature_uuid: &str, nonce: &str) -> String {
    format!("signed-replay:v1:{device_uuid}:{feature_uuid}:{nonce}")
}

async fn claim_signed_nonce(con: &ConnectionManager, notification: &Notification<Value>) -> Result<(), anyhow::Error> {
    let mut con = con.clone();
    let key = signed_replay_key(&notification.device_uuid, &notification.feature_uuid, &notification.nonce);
    let result: Option<String> = redis::cmd("SET")
        .arg(key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(SIGNED_REPLAY_CACHE_TTL_SECS)
        .query_async(&mut con)
        .await?;
    ensure_signed_nonce_claimed(result)
}

fn ensure_signed_nonce_claimed(result: Option<String>) -> Result<(), anyhow::Error> {
    if result.is_none() {
        anyhow::bail!("replayed signed nonce detected");
    }
    Ok(())
}

fn notification_summary_log(notification: &Notification<Value>) -> String {
    format!(
        "device_uuid={}, feature_uuid={}, payload={}",
        notification.device_uuid, notification.feature_uuid, notification.payload
    )
}

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

    let mongo_db = online::db::sensor::connect_mongodb(&env.mongodb_url).await.expect("failed to connect to MongoDB");

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
            let _ = process_mqtt_message(&msg_opt, &mut mqtt_client, &con, &mongo_db).await.inspect_err(|err| {
                let topic = msg_opt.as_ref().map(|msg| msg.topic()).unwrap_or("[disconnected]");
                error!(target: "app", "process_mqtt_message - failed to process MQTT message topic={}, err={:?}", topic, err);
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
    mongo_db: &Database,
) -> Result<(), anyhow::Error> {
    if let Some(msg) = msg_opt {
        use online::db::online::insert_or_update_online as db_update;
        info!(
            target: "app",
            "process_mqtt_message - received MQTT message topic={}, payload_bytes={}",
            msg.topic(),
            msg.payload().len()
        );
        let payload_str = get_string_payload(msg)?;
        let notification: Notification<Value> = serde_json::from_str(&payload_str)?;
        validate_signed_envelope(&notification)?;
        let topic = online::models::topic::Topic::new(msg.topic())?;
        info!(
            target: "app",
            "process_mqtt_message - parsed MQTT message topic={}, notification={}",
            topic,
            notification_summary_log(&notification)
        );
        if topic.device_uuid != notification.device_uuid || topic.feature_uuid != notification.feature_uuid {
            anyhow::bail!("topic does not match signed payload");
        }
        let api_token =
            online::db::sensor::find_sensor_api_token(mongo_db, &notification.device_uuid, &notification.feature_uuid)
                .await?
                .ok_or_else(|| anyhow::anyhow!("registered online sensor not found"))?;
        verify_mqtt_signature(&api_token, &notification)?;
        claim_signed_nonce(con, &notification).await?;
        db_update(con, &api_token, &notification.device_uuid, &notification.feature_uuid).await.inspect_err(|err| {
            error!(target: "app", "process_mqtt_message - cannot insert/update online in db, err = {:?}", err);
        })?;
        info!(
            target: "app",
            "process_mqtt_message - processed MQTT message topic={}, device_uuid={}, feature_uuid={}",
            topic,
            notification.device_uuid,
            notification.feature_uuid
        );
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

#[test]
fn signed_replay_key_scopes_nonce_by_device_and_feature() {
    let key = signed_replay_key("device-a", "feature-b", "nonce-c");
    assert_eq!(key, "signed-replay:v1:device-a:feature-b:nonce-c");
}

#[test]
fn signed_nonce_claim_result_rejects_existing_key() {
    assert!(ensure_signed_nonce_claimed(Some("OK".to_string())).is_ok());
    let err = ensure_signed_nonce_claimed(None).expect_err("duplicate nonce must be rejected");
    assert_eq!(err.to_string(), "replayed signed nonce detected");
}

#[test]
fn signed_payload_binds_online_feature_class() {
    let notification = Notification {
        device_uuid: "246e3256-f0dd-4fcb-82c5-ee20c2267eeb".to_string(),
        feature_uuid: "41cb3f47-894c-45e9-90d9-a4d4de903896".to_string(),
        timestamp: 1_777_630_000,
        nonce: "00112233445566778899aabbccddeeff".to_string(),
        signature: "00".repeat(32),
        payload: serde_json::json!({}),
    };

    let signed_payload = build_signed_mqtt_payload(&notification).expect("signed payload");

    assert_eq!(
        signed_payload,
        "246e3256-f0dd-4fcb-82c5-ee20c2267eeb\n41cb3f47-894c-45e9-90d9-a4d4de903896\nonline\n1777630000\n00112233445566778899aabbccddeeff\n{}"
    );
}

#[test]
fn notification_log_omits_protected_fields() {
    let notification = Notification {
        device_uuid: "246e3256-f0dd-4fcb-82c5-ee20c2267eeb".to_string(),
        feature_uuid: "41cb3f47-894c-45e9-90d9-a4d4de903896".to_string(),
        timestamp: 1_777_630_000,
        nonce: "secret-nonce".to_string(),
        signature: "secret-signature".to_string(),
        payload: serde_json::json!({}),
    };

    let log_line = notification_summary_log(&notification);

    assert!(log_line.contains("device_uuid=246e3256-f0dd-4fcb-82c5-ee20c2267eeb"));
    assert!(log_line.contains("feature_uuid=41cb3f47-894c-45e9-90d9-a4d4de903896"));
    assert!(log_line.contains("payload={}"));
    assert!(!log_line.contains("timestamp"));
    assert!(!log_line.contains("nonce"));
    assert!(!log_line.contains("signature"));
    assert!(!log_line.contains("secret-nonce"));
    assert!(!log_line.contains("secret-signature"));
}

#[test]
fn validate_signed_envelope_rejects_malformed_nonce() {
    let notification = Notification {
        device_uuid: "246e3256-f0dd-4fcb-82c5-ee20c2267eeb".to_string(),
        feature_uuid: "41cb3f47-894c-45e9-90d9-a4d4de903896".to_string(),
        timestamp: 1_777_630_000,
        nonce: "00112233-4455-6677-8899-aabbccddeeff".to_string(),
        signature: "00".repeat(32),
        payload: serde_json::json!({}),
    };

    let err = validate_signed_envelope(&notification).expect_err("malformed nonce must fail validation");

    assert_eq!(err.to_string(), "nonce must be 32 lowercase hex characters");
}

#[test]
fn validate_signed_envelope_rejects_malformed_signature() {
    let notification = Notification {
        device_uuid: "246e3256-f0dd-4fcb-82c5-ee20c2267eeb".to_string(),
        feature_uuid: "41cb3f47-894c-45e9-90d9-a4d4de903896".to_string(),
        timestamp: 1_777_630_000,
        nonce: "00112233445566778899aabbccddeeff".to_string(),
        signature: "not-hex".to_string(),
        payload: serde_json::json!({}),
    };

    let err = validate_signed_envelope(&notification).expect_err("malformed signature must fail validation");

    assert_eq!(err.to_string(), "signature must be 64 lowercase hex characters");
}

#[cfg(test)]
mod tests_integration;
