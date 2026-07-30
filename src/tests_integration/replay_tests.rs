use dotenvy::dotenv;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde_json::{Value, json};
use uuid::Uuid;

use alarm_receiver::models::notification::Notification;

use crate::{claim_signed_nonce, signed_replay_key};

fn notification_with_nonce(device_uuid: &str, feature_uuid: &str, nonce: &str) -> Notification<Value> {
    Notification {
        device_uuid: device_uuid.to_string(),
        feature_uuid: feature_uuid.to_string(),
        timestamp: 1_777_630_000,
        nonce: nonce.to_string(),
        signature: "00".repeat(32), // fake signature
        payload: json!({ "value": 1 }),
    }
}

// Replaying a valid signed payload would repeat the original side effect even though the
// HMAC is still valid (attackers can use this for their purposes),
// so this verifies Redis rejects the same signed nonce after first use.
#[test_log::test(tokio::test)]
async fn claim_signed_nonce_rejects_duplicate_with_real_redis() {
    dotenv().ok();
    let redis_url = std::env::var("REPLAY_REDIS_URI").unwrap();
    let redis_client = redis::Client::open(redis_url).expect("valid Redis URL");
    let con: ConnectionManager = redis_client.get_connection_manager().await.expect("Redis connection");

    let device_uuid = Uuid::new_v4().to_string();
    let feature_uuid = Uuid::new_v4().to_string();
    let nonce = Uuid::new_v4().simple().to_string();
    let notification = notification_with_nonce(&device_uuid, &feature_uuid, &nonce);
    let key = signed_replay_key(&device_uuid, &feature_uuid, &nonce);

    let mut cleanup_con = con.clone();
    let _: usize = cleanup_con.del(&key).await.expect("pre-test cleanup");

    claim_signed_nonce(&con, &notification).await.expect("first nonce claim should pass");
    let err = claim_signed_nonce(&con, &notification).await.expect_err("second nonce claim should be replay");
    assert_eq!(err.to_string(), "replayed signed nonce detected");

    let _: usize = cleanup_con.del(&key).await.expect("post-test cleanup");
}
