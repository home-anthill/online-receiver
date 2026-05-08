use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use redis::{AsyncCommands, aio::ConnectionManager};
use tracing::{debug, error};
use uuid::Uuid;

use crate::errors::redis_error::RedisError;

const FCM_BY_API_TOKEN_KEY: &str = "fcm_by_api_token";

fn is_valid_uuid_v4(s: &str) -> bool {
    Uuid::parse_str(s).map(|u| u.get_version_num() == 4).unwrap_or(false)
}

pub async fn insert_or_update_online(
    con: &ConnectionManager,
    api_token: &str,
    device_uuid: &str,
    feature_uuid: &str,
) -> Result<(), anyhow::Error> {
    debug!(target: "app", "insert_or_update_online - called");

    if !is_valid_uuid_v4(api_token) {
        return Err(RedisError::InvalidUuidError.into());
    }
    if !is_valid_uuid_v4(device_uuid) {
        return Err(RedisError::InvalidUuidError.into());
    }
    if !is_valid_uuid_v4(feature_uuid) {
        return Err(RedisError::InvalidUuidError.into());
    }

    let mut con = con.clone();

    let db_key = from_uuid_to_db_key(device_uuid, feature_uuid);

    let is_exists: bool = con.exists(&db_key).await.map_err(|e| {
        error!(target: "app", "insert_or_update_online - Redis exists error: {:?}", e);
        RedisError::IsExistsError
    })?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis().to_string();
    let fcm_token: Option<String> = match con.hget(FCM_BY_API_TOKEN_KEY, api_token).await {
        Ok(val) => val,
        Err(e) => {
            error!(target: "app", "insert_or_update_online - Redis hget fcmToken error: {:?}", e);
            None
        }
    };
    let mut fields = vec![("apiToken", api_token), ("modifiedAt", timestamp.as_str())];
    if !is_exists {
        fields.push(("createdAt", timestamp.as_str()));
    }
    if let Some(fcm_token) = fcm_token.as_deref() {
        fields.push(("fcmToken", fcm_token));
    }

    let (): () = con.hset_multiple(&db_key, &fields).await.map_err(|e| {
        error!(target: "app", "insert_or_update_online - Redis hset error: {:?}", e);
        RedisError::HSetError
    })?;
    debug!(target: "app", "insert_or_update_online - hset completed");
    Ok(())
}

pub fn from_uuid_to_db_key(device_uuid: &str, feature_uuid: &str) -> String {
    let prefix = if env::var("ENV").as_deref() == Ok("testing") { "test" } else { "online" };
    format!("{}_{}_feature_{}", prefix, device_uuid, feature_uuid)
}
