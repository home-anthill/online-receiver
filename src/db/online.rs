use log::{debug, info};
use std::time::{SystemTime, UNIX_EPOCH};

use redis::{aio::MultiplexedConnection, AsyncCommands, Value};

pub async fn insert_online(con: &MultiplexedConnection, uuid: &str, online: bool) -> Option<()> {
    info!(target: "app", "insert_online - Called");
    let mut con = con.clone();

    let key = "online-".to_owned() + uuid;
    let online_value = if online { 1u64 } else { 0u64 };

    let is_exists: Value = con.exists(key.as_str()).await.unwrap();
    let field_to_set = if is_exists == Value::Int(1) {
        "modifiedAt"
    } else {
        "createdAt"
    };

    let set_response: Value = con
        .hset_multiple(
            key.as_str(),
            &[
                ("online", online_value),
                (
                    field_to_set,
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                ),
            ],
        )
        .await
        .unwrap();

    debug!(target: "app", "insert_online - set_response = {:?}", set_response);

    if set_response == Value::Okay {
        Some(())
    } else {
        // TODO ATTENTION I should return a custom DbError here Err(....) and not Ok.
        log::error!(target: "app", "insert_online - Cannot find and update online");
        None
    }
}
