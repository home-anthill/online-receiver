use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::debug;

use crate::models::message::Message;
use crate::models::notification::Notification;
use crate::models::payload_trait::{OnlineMqttPayload, PayloadTrait};
use crate::models::topic::Topic;

pub mod message;
pub mod notification;
pub mod payload_trait;
pub mod topic;

pub fn get_msg_byte(topic: &Topic, payload_str: &str) -> Result<Vec<u8>, anyhow::Error> {
    debug!(target: "app", "get_msg_byte - payload_str: {}", payload_str);
    message_payload_to_bytes::<OnlineMqttPayload>(payload_str, topic)
}

fn message_payload_to_bytes<T>(payload_str: &str, topic: &Topic) -> Result<Vec<u8>, anyhow::Error>
where
    T: DeserializeOwned + Serialize + Clone + PayloadTrait + Sized,
{
    let val = serde_json::from_str::<Notification<T>>(payload_str)?;
    debug!(target: "app", "message_payload_to_bytes - parsed from JSON string, returning as byte array");
    let serialized = Message::<T>::new_as_json(
        val.api_token.clone(),
        val.device_uuid.clone(),
        val.feature_uuid.clone(),
        topic.clone(),
        val.payload,
    )?;
    Ok(serialized.into_bytes())
}
