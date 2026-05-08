use std::time::Duration;

use mongodb::bson::doc;
use mongodb::{Database, options::ClientOptions};
use serde::Deserialize;
use tracing::error;

use crate::utils_api_token::decrypt_api_token;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SensorDocument {
    api_token_encrypted: String,
}

pub async fn connect_mongodb(mongodb_url: &str) -> mongodb::error::Result<Database> {
    let mut options = ClientOptions::parse(mongodb_url).await?;
    options.app_name = Some("online-receiver".to_string());
    let client = mongodb::Client::with_options(options)?;
    Ok(client.database("sensors"))
}

pub async fn find_sensor_api_token(
    db: &Database,
    device_uuid: &str,
    feature_uuid: &str,
) -> mongodb::error::Result<Option<String>> {
    let collection = db.collection::<SensorDocument>("sensors");
    let sensor_doc = collection
        .find_one(doc! {
            "deviceUuid": device_uuid,
            "featureUuid": feature_uuid,
        })
        .projection(doc! {"apiTokenEncrypted": 1})
        .max_time(Duration::from_secs(30))
        .await
        .inspect_err(|err| {
            error!(target: "app", "find_sensor_api_token - MongoDB error: {:?}", err);
        })?;
    Ok(sensor_doc.and_then(|doc| decrypt_api_token(&doc.api_token_encrypted).ok()))
}
