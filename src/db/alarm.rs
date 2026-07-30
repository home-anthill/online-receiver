use std::time::{SystemTime, UNIX_EPOCH};

use redis::aio::ConnectionManager;
use tracing::error;
use uuid::Uuid;

use crate::errors::redis_error::RedisError;
use crate::models::notification::Notification;

const ALARM_RETENTION_SECONDS: usize = 24 * 60 * 60;

fn is_valid_uuid_v4(value: &str) -> bool {
    Uuid::parse_str(value).map(|uuid| uuid.get_version_num() == 4).unwrap_or(false)
}

pub fn alarm_event_key(
    device_uuid: &str,
    feature_uuid: &str,
    alarm_type: &str,
    nonce: &str,
    is_testing: bool,
) -> String {
    let prefix = if is_testing { "test-alarm" } else { "alarm" };
    format!("{prefix}:{device_uuid}:feature:{feature_uuid}:type:{alarm_type}:nonce:{nonce}")
}

pub fn pending_alarms_key(is_testing: bool) -> &'static str {
    if is_testing { "test-alarms:pending" } else { "alarms:pending" }
}

pub async fn insert_alarm_event(
    con: &ConnectionManager,
    api_token: &str,
    alarm_type: &str,
    notification: &Notification<serde_json::Value>,
    is_testing: bool,
) -> Result<String, anyhow::Error> {
    if !is_valid_uuid_v4(api_token)
        || !is_valid_uuid_v4(&notification.device_uuid)
        || !is_valid_uuid_v4(&notification.feature_uuid)
    {
        return Err(RedisError::InvalidUuidError.into());
    }

    let received_at: u64 = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis().try_into().unwrap_or(u64::MAX);
    let created_at = u64::try_from(notification.timestamp).unwrap_or_default().saturating_mul(1000);
    let payload = serde_json::to_string(&notification.payload)?;
    let key = alarm_event_key(
        &notification.device_uuid,
        &notification.feature_uuid,
        alarm_type,
        &notification.nonce,
        is_testing,
    );
    let mut con = con.clone();
    let mut pipe = redis::pipe();
    pipe.atomic()
        .cmd("HSET")
        .arg(&key)
        .arg("id")
        .arg(&key)
        .arg("apiToken")
        .arg(api_token)
        .arg("deviceUuid")
        .arg(&notification.device_uuid)
        .arg("featureUuid")
        .arg(&notification.feature_uuid)
        .arg("alarmType")
        .arg(alarm_type)
        .arg("payload")
        .arg(payload)
        .arg("createdAt")
        .arg(created_at)
        .arg("receivedAt")
        .arg(received_at)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(ALARM_RETENTION_SECONDS)
        .cmd("ZADD")
        .arg(pending_alarms_key(is_testing))
        .arg(received_at)
        .arg(&key);
    pipe.query_async::<()>(&mut con).await.inspect_err(|err| {
        error!(target: "app", "insert_alarm_event - Redis transaction failed: {:?}", err);
    })?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::{alarm_event_key, pending_alarms_key};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn alarm_keys_are_isolated_from_online_state() {
        assert_eq!(
            "alarm:device:feature:feature:type:motion:nonce:nonce",
            alarm_event_key("device", "feature", "motion", "nonce", false)
        );
        assert_eq!("alarms:pending", pending_alarms_key(false));
        assert_eq!("test-alarms:pending", pending_alarms_key(true));
    }
}
