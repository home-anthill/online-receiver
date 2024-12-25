use log::{debug, info};
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use redis::{aio::MultiplexedConnection, AsyncCommands, Value};

pub async fn insert_or_update_online(con: &MultiplexedConnection, uuid: &str, api_token: &str) -> Option<()> {
    info!(target: "app", "insert_or_update_online - Called");
    let mut con = con.clone();

    let db_key = from_uuid_to_db_key(uuid);

    let is_exists: Value = con.exists(db_key.as_str()).await.unwrap();
    let field_to_set = if is_exists == Value::Int(1) {
        "modifiedAt"
    } else {
        "createdAt"
    };

    let set_response: Value = con
        .hset_multiple(
            db_key.as_str(),
            &[
                ("apiToken", api_token),
                (
                    field_to_set,
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                        .to_string()
                        .as_str(),
                ),
            ],
        )
        .await
        .unwrap();

    debug!(target: "app", "insert_or_update_online - set_response = {:?}", set_response);
    if set_response == Value::Okay {
        Some(())
    } else {
        // TODO ATTENTION I should return a custom DbError here Err(....) and not None
        log::error!(target: "app", "insert_or_update_online - Cannot create or update online in db");
        None
    }
}

pub fn from_uuid_to_db_key(uuid: &str) -> String {
    let env = env::var("ENV").ok().unwrap_or("".to_string());
    if env == "testing" { "test-" } else { "online-" }.to_owned() + uuid
}
