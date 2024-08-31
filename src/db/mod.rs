use crate::config::Env;
use log::{error, info, warn};
use mongodb::bson::doc;
use mongodb::options::{ClientOptions, ServerApi, ServerApiVersion};
use mongodb::{Client, Database};
use std::env;
use std::future::Future;

pub mod online;

pub async fn connect(env_config: &Env) -> mongodb::error::Result<Database> {
    let mongo_uri = env_config.mongo_uri.clone();

    let mongo_db_name = if env::var("ENV") == Ok(String::from("testing")) {
        warn!("TESTING ENVIRONMENT - forcing mongo_db_name = 'online_test'");
        String::from("online_test")
    } else {
        env_config.mongo_db_name.clone()
    };

    let mut client_options = ClientOptions::parse(mongo_uri).await?;
    // Set the server_api field of the client_options object to Stable API version 1
    let server_api = ServerApi::builder().version(ServerApiVersion::V1).build();
    client_options.server_api = Some(server_api);
    // Set app_name
    client_options.app_name = Some("online".to_string());

    // Create a new client and connect to the server
    let client = Client::with_options(client_options)?;
    let database = client.database(mongo_db_name.as_str());

    info!(target: "app", "Pinging MongoDB server...");
    retry_connect_mongodb(|| async { database.run_command(doc! { "ping": 1 }).await }, 50).await?;

    Ok(database)
}

async fn retry_connect_mongodb<T, E, Fut, F>(mut f: F, retries: usize) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut count = 0;
    loop {
        let result = f().await;
        if result.is_ok() {
            info!(target: "app", "MongoDB connected!");
            return result;
        } else {
            if count >= retries {
                error!(target: "app", "Cannot connect to MongoDB, max tries reached");
                return result;
            }
            count += 1;
            warn!(target: "app", "MongoDB ping failed (count={}), retrying...", count);
        }
    }
}
