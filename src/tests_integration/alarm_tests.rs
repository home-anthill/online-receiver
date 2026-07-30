use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use redis::{AsyncCommands, aio::ConnectionManager};
use serde_json::json;
use uuid::Uuid;

use alarm_receiver::db::alarm::{alarm_event_key, insert_alarm_event, pending_alarms_key};
use alarm_receiver::models::notification::Notification;

#[test_log::test(tokio::test)]
async fn insert_alarm_event_writes_hash_and_pending_index_to_test_database() {
    let client = redis::Client::open("redis://localhost:6379/15").expect("valid Redis test URI");
    let con: ConnectionManager = client.get_connection_manager().await.expect("Redis test connection");
    let device_uuid = Uuid::new_v4().to_string();
    let feature_uuid = Uuid::new_v4().to_string();
    let api_token = Uuid::new_v4().to_string();
    let nonce = Uuid::new_v4().simple().to_string();
    let notification = Notification {
        device_uuid: device_uuid.clone(),
        feature_uuid: feature_uuid.clone(),
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock").as_secs() as i64,
        nonce: nonce.clone(),
        signature: "00".repeat(32),
        payload: json!({ "value": 1 }),
    };
    let event_key = alarm_event_key(&device_uuid, &feature_uuid, "motion", &nonce, true);
    let mut cleanup = con.clone();
    let _: usize = cleanup.del(&event_key).await.expect("pre-test event cleanup");
    let _: usize = cleanup.zrem(pending_alarms_key(true), &event_key).await.expect("pre-test index cleanup");

    let inserted_key =
        insert_alarm_event(&con, &api_token, "motion", &notification, true).await.expect("insert alarm event");

    let hash: HashMap<String, String> = cleanup.hgetall(&event_key).await.expect("read alarm hash");
    let pending: Vec<String> = cleanup.zrange(pending_alarms_key(true), 0, -1).await.expect("read pending index");
    assert_eq!(event_key, inserted_key);
    assert_eq!("motion", hash["alarmType"]);
    assert_eq!(r#"{"value":1}"#, hash["payload"]);
    assert!(pending.contains(&event_key));

    let _: usize = cleanup.del(&event_key).await.expect("post-test event cleanup");
    let _: usize = cleanup.zrem(pending_alarms_key(true), &event_key).await.expect("post-test index cleanup");
}
