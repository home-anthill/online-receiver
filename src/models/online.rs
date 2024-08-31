use mongodb::bson::oid::ObjectId;
use mongodb::bson::DateTime;
use serde::{Deserialize, Serialize};

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnlineDocument {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub uuid: String,
    pub apiToken: String,
    pub createdAt: DateTime,
    pub modifiedAt: DateTime,
    pub online: bool,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Online {
    pub _id: String,
    pub uuid: String,
    pub apiToken: String,
    pub createdAt: String,
    pub modifiedAt: String,
    pub online: bool,
}
