use log::{debug, error};
use serde::{Deserialize, Serialize};

use crate::models::message::Message;
use crate::models::notification::Notification;
use crate::models::payload_trait::{OnlineMqttPayload, PayloadTrait};
use crate::models::topic::Topic;

pub mod message;
pub mod notification;
pub mod online;
pub mod payload_trait;
pub mod topic;

pub fn get_msg_byte(topic: &Topic, payload_str: &str) -> Vec<u8> {
    debug!(target: "app", "get_msg_byte - payload_str: {}", payload_str);
    message_payload_to_bytes::<OnlineMqttPayload>(payload_str, topic)
}

fn message_payload_to_bytes<'a, T>(payload_str: &'a str, topic: &Topic) -> Vec<u8>
where
    T: Deserialize<'a> + Serialize + Clone + PayloadTrait + Sized,
{
    // deserialize to a Notification (with turbofish operator "::<Notification>")
    let parsed_result = serde_json::from_str::<Notification<T>>(payload_str);
    match parsed_result {
        Ok(val) => {
            debug!(target: "app", "message_payload_to_bytes - parsed from JSON string, returning as byte array");
            let serialized =
                Message::<T>::new_as_json(val.uuid.clone(), val.api_token.clone(), topic.clone(), val.payload);
            serialized.into_bytes()
        }
        Err(err) => {
            error!(target: "app", "message_payload_to_bytes - cannot parse JSON from string, returning empty data. Err = {:?}", &err);
            vec![]
        }
    }
}
