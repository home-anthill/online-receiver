use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errors::message_error::MessageError;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub family: String,
    pub device_uuid: String,
    pub feature_uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alarm_type: Option<String>,
}

impl Topic {
    pub fn new(topic: &str) -> Result<Self, MessageError> {
        // topic forms are:
        //  online/device_uuid/features/feature_uuid
        //  alarms/device_uuid/features/feature_uuid/alarm-type
        let items: Vec<&str> = topic.split('/').collect();
        let (family, alarm_type) = match items.as_slice() {
            ["online", _, "features", _] => ("online", None),
            ["alarms", _, "features", _, alarm_type] if is_valid_alarm_type(alarm_type) => {
                ("alarms", Some((*alarm_type).to_string()))
            }
            _ => return Err(MessageError::ParseMessageError),
        };
        if items[1].is_empty() || items[3].is_empty() {
            return Err(MessageError::ParseMessageError);
        }
        Ok(Self {
            family: family.to_string(),
            device_uuid: items[1].to_string(),
            feature_uuid: items[3].to_string(),
            alarm_type,
        })
    }
}

fn is_valid_alarm_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && value.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
}

impl fmt::Display for Topic {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}/{}/features/{}", self.family, self.device_uuid, self.feature_uuid)?;
        if let Some(alarm_type) = &self.alarm_type {
            write!(fmt, "/{alarm_type}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::models::topic::Topic;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn check_topic_display() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "b6505821-3ac9-45e6-9018-72d3ecb9b591";
        let topic = Topic::new(&format!("online/{}/features/{}", device_uuid, feature_uuid)).unwrap();
        let expected = topic.to_string();
        assert_eq!(format!("online/{}/features/{}", device_uuid, feature_uuid), expected);
    }

    #[test_log::test]
    fn check_topic_invalid() {
        let result = Topic::new("invalid/topic");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn parses_alarm_topic() {
        let topic = Topic::new(
            "alarms/246e3256-f0dd-4fcb-82c5-ee20c2267eeb/features/b6505821-3ac9-45e6-9018-72d3ecb9b591/thermostat-mode-error",
        )
        .unwrap();

        assert_eq!(topic.family, "alarms");
        assert_eq!(topic.alarm_type.as_deref(), Some("thermostat-mode-error"));
        assert_eq!(
            topic.to_string(),
            "alarms/246e3256-f0dd-4fcb-82c5-ee20c2267eeb/features/b6505821-3ac9-45e6-9018-72d3ecb9b591/thermostat-mode-error"
        );
    }

    #[test_log::test]
    fn rejects_extra_segments_and_invalid_alarm_types() {
        assert!(Topic::new("online/device/features/feature/extra").is_err());
        assert!(Topic::new("alarms/device/features/feature/Motion").is_err());
        assert!(Topic::new("alarms/device/features/feature/-motion").is_err());
        assert!(Topic::new("other/device/features/feature").is_err());
    }
}
