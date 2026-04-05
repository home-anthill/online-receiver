use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errors::message_error::MessageError;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub family: String,
    pub device_uuid: String,
    pub feature_uuid: String,
}

impl Topic {
    pub fn new(topic: &str) -> Result<Self, MessageError> {
        // topic form is:
        //  online/device_uuid/features/feature_uuid
        let items: Vec<&str> = topic.split('/').collect();
        if items.len() < 4 {
            return Err(MessageError::ParseMessageError);
        }
        Ok(Self { family: items[0].to_string(), device_uuid: items[1].to_string(), feature_uuid: items[3].to_string() })
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}/{}/features/{}", self.family, self.device_uuid, self.feature_uuid)
    }
}

#[cfg(test)]
mod tests {
    use crate::models::topic::Topic;
    use pretty_assertions::assert_eq;

    #[test]
    #[test_log::test]
    fn check_topic_display() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "b6505821-3ac9-45e6-9018-72d3ecb9b591";
        let topic = Topic::new(&format!("online/{}/features/{}", device_uuid, feature_uuid)).unwrap();
        let expected = topic.to_string();
        assert_eq!(format!("online/{}/features/{}", device_uuid, feature_uuid), expected);
    }

    #[test]
    #[test_log::test]
    fn check_topic_invalid() {
        let result = Topic::new("invalid/topic");
        assert!(result.is_err());
    }
}
