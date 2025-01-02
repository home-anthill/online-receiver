use log::debug;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use redis::{aio::ConnectionManager, AsyncCommands, RedisResult};

use crate::errors::redis_error::RedisError;

pub async fn insert_or_update_online(
    con: &ConnectionManager,
    uuid: &str,
    api_token: &str,
) -> Result<(), anyhow::Error> {
    debug!(target: "app", "insert_or_update_online - called");
    let mut con = con.clone();

    let db_key = from_uuid_to_db_key(uuid);

    let is_exists_res: RedisResult<Value> = con.exists(db_key.as_str()).await;
    if is_exists_res.is_err() {
        return Err(anyhow::Error::from(RedisError::IsExistsError));
    }
    let is_exists: Value = is_exists_res?;
    let field_to_set = if is_exists == Value::Int(1) {
        "modifiedAt"
    } else {
        "createdAt"
    };

    let hset_res: RedisResult<Value> = con
        .hset_multiple(
            db_key.as_str(),
            &[
                ("apiToken", api_token),
                (
                    field_to_set,
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)?
                        .as_millis()
                        .to_string()
                        .as_str(),
                ),
            ],
        )
        .await;
    if hset_res.is_err() {
        return Err(anyhow::Error::from(RedisError::HSetError));
    }
    let hset: Value = hset_res?;
    debug!(target: "app", "insert_or_update_online - hset = {:?}", hset);
    if hset == Value::Okay {
        Ok(())
    } else {
        Err(anyhow::Error::from(RedisError::HSetResultError))
    }
}

pub fn from_uuid_to_db_key(uuid: &str) -> String {
    let env = env::var("ENV").ok().unwrap_or("".to_string());
    if env == "testing" { "test-" } else { "online-" }.to_owned() + uuid
}
