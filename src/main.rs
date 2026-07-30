use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, KeyInit, Mac};
use mongodb::Database;
use paho_mqtt::Message;
use redis::aio::ConnectionManager;
use rocket::{self, catchers, routes};
use serde_json::Value;
use sha2::Sha256;
use tracing::{error, info, warn};

use alarm_receiver::catchers as app_catchers;
use alarm_receiver::config::{AppEnv, Env, init};
use alarm_receiver::models::notification::Notification;
use alarm_receiver::mqtt::get_string_payload;
use alarm_receiver::mqtt::mqtt_client::MqttClient;
use alarm_receiver::mqtt::mqtt_config::MqttConfig;
use alarm_receiver::mqtt::mqtt_options::MqttOptions;
use alarm_receiver::routes as app_routes;

const TOPICS: &[&str] = &["online/+/features/+", "alarms/+/features/+/+"];
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
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(message);
    match hex::decode(expected_hex) {
        Ok(expected_bytes) => mac.verify_slice(&expected_bytes).is_ok(),
        Err(_) => {
            let _ = mac.finalize();
            false
        }
    }
}

fn build_signed_mqtt_payload(notification: &Notification<Value>, feature_name: &str) -> Result<String, anyhow::Error> {
    let payload_json = serde_json::to_string(&notification.payload)?;
    Ok(format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        notification.device_uuid,
        notification.feature_uuid,
        feature_name,
        notification.timestamp,
        notification.nonce,
        payload_json
    ))
}

fn verify_mqtt_signature(
    api_token: &str,
    feature_name: &str,
    notification: &Notification<Value>,
) -> Result<(), anyhow::Error> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    if (now - notification.timestamp).abs() > SIGNED_MESSAGE_MAX_SKEW_SECS {
        anyhow::bail!("message timestamp is outside the allowed freshness window");
    }
    let signed_payload = build_signed_mqtt_payload(notification, feature_name)?;
    if !verify_hmac(api_token, signed_payload.as_bytes(), &notification.signature) {
        anyhow::bail!("invalid signed MQTT payload");
    }
    Ok(())
}

fn alarm_type_for_topic(
    topic: &alarm_receiver::models::topic::Topic,
    registered_feature_name: &str,
) -> Result<Option<String>, anyhow::Error> {
    if topic.family == "online" {
        if topic.alarm_type.is_none() && registered_feature_name == "online" {
            return Ok(None);
        }
        anyhow::bail!("MQTT topic is not valid for the registered feature");
    }

    let Some(alarm_type) = topic.alarm_type.as_deref() else {
        anyhow::bail!("MQTT topic is not valid for the registered feature");
    };

    let is_thermostat_mode_error = alarm_type == "thermostat-mode-error" && registered_feature_name == "mode";

    if topic.family != "alarms" || (alarm_type != registered_feature_name && !is_thermostat_mode_error) {
        anyhow::bail!("MQTT topic is not valid for the registered feature");
    }

    Ok(Some(alarm_type.to_string()))
}

fn validate_alarm_payload(alarm_type: &str, payload: &Value) -> Result<(), anyhow::Error> {
    let value = payload.get("value").and_then(Value::as_f64);
    match alarm_type {
        "motion" if value != Some(1.0) => anyhow::bail!("motion alarm payload value must be 1"),
        "thermostat-mode-error" if value != Some(-1.0) => {
            anyhow::bail!("thermostat mode error payload value must be -1")
        }
        _ => Ok(()),
    }
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

#[cfg(test)]
fn redis_uri_for_database(redis_uri: &str, database: u8) -> String {
    let Some(scheme_end) = redis_uri.find("://") else {
        return redis_uri.to_string();
    };
    let authority_start = scheme_end + 3;
    let query_start = redis_uri[authority_start..].find('?').map(|index| authority_start + index);
    let path_start = redis_uri[authority_start..].find('/').map(|index| authority_start + index);
    let end_before_query = query_start.unwrap_or(redis_uri.len());
    let authority_end = match path_start {
        Some(index) if index < end_before_query => index,
        _ => end_before_query,
    };
    let query = query_start.map(|index| &redis_uri[index..]).unwrap_or("");
    format!("{}{}/{}{}", &redis_uri[..authority_start], &redis_uri[authority_start..authority_end], database, query)
}

fn redis_url_with_credentials(redis_uri: &str, redis_username: &str, redis_password: &str) -> String {
    if redis_password.is_empty() {
        return redis_uri.to_string();
    }
    match redis_uri.find("://") {
        Some(scheme_end) => format!(
            "{scheme}{username}:{password}@{rest}",
            scheme = &redis_uri[..scheme_end + 3],
            username = urlencoding::encode(redis_username),
            password = urlencoding::encode(redis_password),
            rest = &redis_uri[scheme_end + 3..],
        ),
        None => {
            warn!(target: "app", "REDIS_URI has no recognizable scheme (missing '://'), skipping credential injection");
            redis_uri.to_string()
        }
    }
}

fn notification_summary_log(notification: &Notification<Value>) -> String {
    format!(
        "device_uuid={}, feature_uuid={}, payload={}",
        notification.device_uuid, notification.feature_uuid, notification.payload
    )
}

#[cfg(test)]
fn valid_signed_notification() -> Notification<Value> {
    Notification {
        device_uuid: "246e3256-f0dd-4fcb-82c5-ee20c2267eeb".to_string(),
        feature_uuid: "41cb3f47-894c-45e9-90d9-a4d4de903896".to_string(),
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock").as_secs() as i64,
        nonce: "00112233445566778899aabbccddeeff".to_string(),
        signature: "00".repeat(32),
        payload: serde_json::json!({ "value": 1 }),
    }
}

#[cfg(test)]
fn sign_notification(api_token: &str, feature_name: &str, notification: &mut Notification<Value>) {
    let signed_payload = build_signed_mqtt_payload(notification, feature_name).expect("signed payload");
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(api_token.as_bytes()).expect("HMAC accepts any key length");
    mac.update(signed_payload.as_bytes());
    notification.signature = hex::encode(mac.finalize().into_bytes());
}

#[rocket::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), rocket::Error> {
    // 1. Init logger and env
    let (env, app_env): (Env, AppEnv) = init();

    // 2. Init and connect to Redis
    // If credentials are configured, inject them into the URI:
    //   redis://host:port -> redis://username:password@host:port
    if !env.redis_username.is_empty() && env.redis_password.is_empty() {
        warn!(target: "app", "REDIS_USERNAME is set but REDIS_PASSWORD is empty — no authentication will be attempted");
    }
    let online_redis_url = redis_url_with_credentials(&env.online_redis_uri, &env.redis_username, &env.redis_password);
    let replay_redis_url = redis_url_with_credentials(&env.replay_redis_uri, &env.redis_username, &env.redis_password);
    let alarms_redis_url = redis_url_with_credentials(&env.alarms_redis_uri, &env.redis_username, &env.redis_password);

    let online_redis_client = redis::Client::open(online_redis_url).expect("invalid Redis URI");
    let online_con: ConnectionManager =
        online_redis_client.get_connection_manager().await.expect("failed to connect to Redis");
    let replay_redis_client = redis::Client::open(replay_redis_url).expect("invalid replay Redis URI");
    let replay_con: ConnectionManager =
        replay_redis_client.get_connection_manager().await.expect("failed to connect to replay Redis");
    let alarms_redis_client = redis::Client::open(alarms_redis_url).expect("invalid alarms Redis URI");
    let alarms_con: ConnectionManager =
        alarms_redis_client.get_connection_manager().await.expect("failed to connect to alarms Redis");

    let mongo_db =
        alarm_receiver::db::sensor::connect_mongodb(&env.mongodb_url).await.expect("failed to connect to MongoDB");

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
            let _ = process_mqtt_message(
                &msg_opt,
                &mut mqtt_client,
                &online_con,
                &replay_con,
                &alarms_con,
                &mongo_db,
                app_env.is_testing(),
            )
            .await
            .inspect_err(|err| {
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
    replay_con: &ConnectionManager,
    alarms_con: &ConnectionManager,
    mongo_db: &Database,
    is_testing: bool,
) -> Result<(), anyhow::Error> {
    if let Some(msg) = msg_opt {
        use alarm_receiver::db::online::insert_or_update_online as db_update;
        info!(
            target: "app",
            "process_mqtt_message - received MQTT message topic={}, payload_bytes={}",
            msg.topic(),
            msg.payload().len()
        );
        let payload_str = get_string_payload(msg)?;
        let notification: Notification<Value> = serde_json::from_str(&payload_str)?;
        validate_signed_envelope(&notification)?;
        let topic = alarm_receiver::models::topic::Topic::new(msg.topic())?;
        info!(
            target: "app",
            "process_mqtt_message - parsed MQTT message topic={}, notification={}",
            topic,
            notification_summary_log(&notification)
        );
        if topic.device_uuid != notification.device_uuid || topic.feature_uuid != notification.feature_uuid {
            anyhow::bail!("topic does not match signed payload");
        }
        let sensor_auth = alarm_receiver::db::sensor::find_sensor_auth(
            mongo_db,
            &notification.device_uuid,
            &notification.feature_uuid,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("registered sensor not found"))?;
        let alarm_type = alarm_type_for_topic(&topic, &sensor_auth.feature_name)?;
        verify_mqtt_signature(&sensor_auth.api_token, &sensor_auth.feature_name, &notification)?;
        if let Some(alarm_type) = &alarm_type {
            validate_alarm_payload(alarm_type, &notification.payload)?;
        }
        claim_signed_nonce(replay_con, &notification).await?;
        if let Some(alarm_type) = alarm_type {
            alarm_receiver::db::alarm::insert_alarm_event(
                alarms_con,
                &sensor_auth.api_token,
                &alarm_type,
                &notification,
                is_testing,
            )
            .await
            .inspect_err(|err| {
                error!(target: "app", "process_mqtt_message - cannot insert alarm event in db, err = {:?}", err);
            })?;
        } else {
            db_update(con, &sensor_auth.api_token, &notification.device_uuid, &notification.feature_uuid)
                .await
                .inspect_err(|err| {
                    error!(target: "app", "process_mqtt_message - cannot insert/update online in db, err = {:?}", err);
                })?;
        }
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

#[cfg(test)]
#[test_log::test]
fn signed_replay_key_scopes_nonce_by_device_and_feature() {
    let key = signed_replay_key("device-a", "feature-b", "nonce-c");
    assert_eq!(key, "signed-replay:v1:device-a:feature-b:nonce-c");
}

#[cfg(test)]
#[test_log::test]
fn signed_nonce_claim_result_rejects_existing_key() {
    assert!(ensure_signed_nonce_claimed(Some("OK".to_string())).is_ok());
    let err = ensure_signed_nonce_claimed(None).expect_err("duplicate nonce must be rejected");
    assert_eq!(err.to_string(), "replayed signed nonce detected");
}

#[cfg(test)]
#[test_log::test]
fn redis_uri_for_database_sets_replay_database() {
    assert_eq!(redis_uri_for_database("redis://localhost:6379/0", 2), "redis://localhost:6379/2");
    assert_eq!(
        redis_uri_for_database("redis://redis.example:6379?protocol=3", 2),
        "redis://redis.example:6379/2?protocol=3"
    );
}

#[cfg(test)]
#[test_log::test]
fn redis_url_with_credentials_preserves_database() {
    assert_eq!(
        redis_url_with_credentials("redis://localhost:6379/2", "redis user", "pa:ss"),
        "redis://redis%20user:pa%3Ass@localhost:6379/2"
    );
}

#[cfg(test)]
#[test_log::test]
fn signed_payload_binds_online_feature_class() {
    let notification = Notification {
        device_uuid: "246e3256-f0dd-4fcb-82c5-ee20c2267eeb".to_string(),
        feature_uuid: "41cb3f47-894c-45e9-90d9-a4d4de903896".to_string(),
        timestamp: 1_777_630_000,
        nonce: "00112233445566778899aabbccddeeff".to_string(),
        signature: "00".repeat(32),
        payload: serde_json::json!({}),
    };

    let signed_payload = build_signed_mqtt_payload(&notification, "online").expect("signed payload");

    assert_eq!(
        signed_payload,
        "246e3256-f0dd-4fcb-82c5-ee20c2267eeb\n41cb3f47-894c-45e9-90d9-a4d4de903896\nonline\n1777630000\n00112233445566778899aabbccddeeff\n{}"
    );
}

#[cfg(test)]
#[test_log::test]
fn signed_payload_binds_registered_mode_feature() {
    let notification = Notification {
        device_uuid: "246e3256-f0dd-4fcb-82c5-ee20c2267eeb".to_string(),
        feature_uuid: "41cb3f47-894c-45e9-90d9-a4d4de903896".to_string(),
        timestamp: 1_777_630_000,
        nonce: "00112233445566778899aabbccddeeff".to_string(),
        signature: "00".repeat(32),
        payload: serde_json::json!({ "value": -1 }),
    };

    let signed_payload = build_signed_mqtt_payload(&notification, "mode").expect("signed payload");

    assert!(signed_payload.contains("\nmode\n1777630000\n"));
}

#[cfg(test)]
#[test_log::test]
fn alarm_topic_must_match_registered_feature() {
    let motion = alarm_receiver::models::topic::Topic::new(
        "alarms/246e3256-f0dd-4fcb-82c5-ee20c2267eeb/features/41cb3f47-894c-45e9-90d9-a4d4de903896/motion",
    )
    .unwrap();
    let thermostat = alarm_receiver::models::topic::Topic::new(
        "alarms/246e3256-f0dd-4fcb-82c5-ee20c2267eeb/features/41cb3f47-894c-45e9-90d9-a4d4de903896/thermostat-mode-error",
    )
    .unwrap();

    assert_eq!(alarm_type_for_topic(&motion, "motion").unwrap().as_deref(), Some("motion"));
    assert_eq!(alarm_type_for_topic(&thermostat, "mode").unwrap().as_deref(), Some("thermostat-mode-error"));
    assert!(alarm_type_for_topic(&thermostat, "temperature").is_err());
}

#[cfg(test)]
#[test_log::test]
fn known_alarm_payloads_require_trigger_values() {
    validate_alarm_payload("motion", &serde_json::json!({ "value": 1 })).unwrap();
    validate_alarm_payload("thermostat-mode-error", &serde_json::json!({ "value": -1 })).unwrap();
    assert!(validate_alarm_payload("motion", &serde_json::json!({ "value": 0 })).is_err());
    assert!(validate_alarm_payload("thermostat-mode-error", &serde_json::json!({ "value": 0 })).is_err());
}

#[cfg(test)]
#[test_log::test]
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

#[cfg(test)]
#[test_log::test]
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

#[cfg(test)]
#[test_log::test]
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
#[test_log::test]
fn validate_signed_envelope_accepts_valid_envelope() {
    let notification = valid_signed_notification();

    validate_signed_envelope(&notification).expect("valid envelope should pass");
}

#[cfg(test)]
#[test_log::test]
fn validate_signed_envelope_rejects_non_positive_timestamp() {
    let mut notification = valid_signed_notification();
    notification.timestamp = 0;

    let err = validate_signed_envelope(&notification).expect_err("zero timestamp must fail validation");

    assert_eq!(err.to_string(), "timestamp must be positive");
}

#[cfg(test)]
#[test_log::test]
fn validate_signed_envelope_rejects_uppercase_nonce() {
    let mut notification = valid_signed_notification();
    notification.nonce = "00112233445566778899AABBCCDDEEFF".to_string();

    let err = validate_signed_envelope(&notification).expect_err("uppercase nonce must fail validation");

    assert_eq!(err.to_string(), "nonce must be 32 lowercase hex characters");
}

#[cfg(test)]
#[test_log::test]
fn validate_signed_envelope_rejects_uppercase_signature() {
    let mut notification = valid_signed_notification();
    notification.signature = "AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899".to_string();

    let err = validate_signed_envelope(&notification).expect_err("uppercase signature must fail validation");

    assert_eq!(err.to_string(), "signature must be 64 lowercase hex characters");
}

#[cfg(test)]
#[test_log::test]
fn verify_hmac_rejects_wrong_signature() {
    let is_valid = verify_hmac("secret", b"message", &"00".repeat(32));

    assert!(!is_valid);
}

#[cfg(test)]
#[test_log::test]
fn verify_hmac_rejects_non_hex_signature() {
    let is_valid = verify_hmac("secret", b"message", "not-hex");

    assert!(!is_valid);
}

#[cfg(test)]
#[test_log::test]
fn verify_mqtt_signature_accepts_current_signed_payload() {
    let api_token = "sensor-api-token";
    let mut notification = valid_signed_notification();
    sign_notification(api_token, "online", &mut notification);

    verify_mqtt_signature(api_token, "online", &notification).expect("valid signature should pass");
}

#[cfg(test)]
#[test_log::test]
fn verify_mqtt_signature_rejects_wrong_api_token() {
    let mut notification = valid_signed_notification();
    sign_notification("correct-token", "online", &mut notification);

    let err = verify_mqtt_signature("wrong-token", "online", &notification)
        .expect_err("wrong API token must fail validation");

    assert_eq!(err.to_string(), "invalid signed MQTT payload");
}

#[cfg(test)]
#[test_log::test]
fn verify_mqtt_signature_rejects_stale_timestamp() {
    let mut notification = valid_signed_notification();
    notification.timestamp = 1;

    let err = verify_mqtt_signature("sensor-api-token", "online", &notification)
        .expect_err("stale message must fail validation");

    assert_eq!(err.to_string(), "message timestamp is outside the allowed freshness window");
}

#[cfg(test)]
mod tests_integration;
