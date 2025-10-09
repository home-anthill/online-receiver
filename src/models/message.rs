use serde::{Deserialize, Serialize};

use crate::models::payload_trait::PayloadTrait;
use crate::models::topic::Topic;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Message<T>
where
    T: PayloadTrait + Sized + Serialize,
{
    pub api_token: String,
    pub device_uuid: String,
    pub feature_uuid: String,
    pub topic: Topic,
    pub payload: T,
}

impl<T> Message<T>
where
    T: PayloadTrait + Sized + Serialize,
{
    pub fn new(api_token: String, device_uuid: String, feature_uuid: String, topic: Topic, payload: T) -> Message<T> {
        Self {
            api_token,
            device_uuid,
            feature_uuid,
            topic,
            payload,
        }
    }
    pub fn new_as_json(
        api_token: String,
        device_uuid: String,
        feature_uuid: String,
        topic: Topic,
        payload: T,
    ) -> String {
        let message = Self::new(api_token, device_uuid, feature_uuid, topic, payload);
        serde_json::to_string(&message).unwrap()
    }
}
