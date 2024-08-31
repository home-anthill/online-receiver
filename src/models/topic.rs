use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub root: String,
    pub device_id: String,
}

impl Topic {
    pub fn new(topic: &str) -> Self {
        let items: Vec<&str> = topic.split('/').collect();
        Self {
            root: items.first().unwrap().to_string(),
            device_id: items.get(1).unwrap().to_string(),
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(self.root.as_str())?;
        fmt.write_str("/")?;
        fmt.write_str(self.device_id.as_str())?;
        Ok(())
    }
}
