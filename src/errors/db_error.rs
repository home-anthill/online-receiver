use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbError {
    pub message: String,
}

impl DbError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}
