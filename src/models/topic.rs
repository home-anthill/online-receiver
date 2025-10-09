use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub family: String,
    pub device_uuid: String,
    pub feature_uuid: String,
}

impl Topic {
    pub fn new(topic: &str) -> Self {
        // topic form is:
        //  online/device_uuid/features/device_uuid
        let items: Vec<&str> = topic.split('/').collect();
        Self {
            family: items.first().unwrap().to_string(),
            device_uuid: items.get(1).unwrap().to_string(),
            feature_uuid: items.last().unwrap().to_string(),
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(self.family.as_str())?;
        fmt.write_str("/")?;
        fmt.write_str(self.device_uuid.as_str())?;
        fmt.write_str("/features/")?;
        fmt.write_str(self.feature_uuid.as_str())?;
        Ok(())
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
        let topic: Topic = Topic::new(format!("online/{}/features/{}", device_uuid, feature_uuid).as_str());
        let expected = topic.to_string();
        assert_eq!(format!("online/{}/features/{}", device_uuid, feature_uuid), expected);
    }
}
