use anyhow::bail;
use log::{debug, error, info, warn};
use mongodb::Database;
use online::config::{init, Env};
use online::db::connect;
use online::db::online::insert_online;
use online::errors::message_error::MessageError;
use online::models::notification::Notification;
use online::models::payload_trait::OnlineMqttPayload;
use online::mqtt::mqtt_client::MqttClient;
use online::mqtt::mqtt_config::MqttConfig;
use online::mqtt::mqtt_options::MqttOptions;
use online::mqtt::{get_bytes_from_payload, get_string_payload};
use paho_mqtt::Message;
use std::time::Duration;

const TOPICS: &[&str] = &["online/+"];

#[tokio::main]
async fn main() {
    // 1. Init logger and env
    let env: Env = init();

    // 2. Init and connect to the Database
    let database: Database = connect(&env).await.unwrap_or_else(|error| {
        error!(target: "app", "MongoDB - cannot connect {:?}", error);
        panic!("cannot connect to MongoDB:: {:?}", error)
    });

    // 3. Init and connect to MQTT
    info!(target: "app", "Initializing MQTT...");
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    match MqttClient::new(MqttOptions::new(&mqtt_config)) {
        Ok(mut mqtt_client) => {
            mqtt_client.connect().await;
            if let Err(err) = mqtt_client.subscribe(TOPICS).await {
                error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
                panic!("unknown error, because MQTT cannot subscribe to TOPICS");
            }
            // 4. Wait for incoming MQTT messages
            info!(target: "app", "Waiting for incoming MQTT messages");
            while let Some(msg_opt) = mqtt_client.get_next_message().await {
                let _ = process_mqtt_message(&msg_opt, &mut mqtt_client, &database).await;
            }
        }
        Err(err) => {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("unknown error, cannot create MQTT client");
        }
    }
}

async fn process_mqtt_message(
    msg_opt: &Option<Message>,
    mqtt_client: &mut MqttClient,
    database: &Database,
) -> Result<(), anyhow::Error> {
    if let Some(msg) = msg_opt {
        debug!(target: "app", "process_mqtt_message - MQTT message received");
        let msg_byte: Vec<u8> = get_bytes_from_payload(msg);
        // return this if
        if msg_byte.is_empty() {
            // msg is not valid, because empty
            debug!(target: "app", "process_mqtt_message - Empty msg_byte received");
            Err(anyhow::Error::from(MessageError::EmptyMessageError))
        } else {
            match serde_json::from_str::<Notification<OnlineMqttPayload>>(get_string_payload(msg).as_str()) {
                Ok(res) => match insert_online(database, &res.uuid, &res.api_token, true).await {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        error!(target: "app", "process_mqtt_message - cannot insert/update online in db, err = {:?}", &err);
                        bail!("Cannot insert/update online in db")
                    }
                },
                Err(err) => {
                    error!(target: "app", "process_mqtt_message - cannot parse message as Notification, err = {:?}", &err);
                    // quickest way to return anyhow error from string as explained here https://docs.rs/anyhow/latest/anyhow/
                    // it's equivalent to `return Err(anyhow!("message"))`
                    Err(anyhow::Error::from(MessageError::ParseMessageError))
                }
            }
        }
    } else {
        // msg_opt="None" means we were disconnected. Try to reconnect...
        warn!(target: "app", "process_mqtt_message - Lost connection. Attempting reconnect in 5 seconds...");
        while let Err(err) = mqtt_client.reconnect().await {
            error!(target: "app", "process_mqtt_message - Error reconnecting: {:?}, retrying in 5 seconds...", err);
            tokio::time::sleep(Duration::from_millis(5000)).await;
        }
        Ok(())
    }
}
