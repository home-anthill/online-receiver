use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnlineMqttPayload {
    // at the moment we don't need a payload
}

pub trait PayloadTrait {}

impl PayloadTrait for OnlineMqttPayload {}
