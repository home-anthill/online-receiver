use std::string::String;
use std::time::Duration;

use futures::stream::StreamExt;
use log::{error, info};
use paho_mqtt::{AsyncClient, AsyncReceiver, ConnectOptions, Message, ServerResponse};

use crate::mqtt::mqtt_options::MqttOptions;

pub struct MqttClient {
    conn_opts: ConnectOptions,
    client: AsyncClient,
    pub message_stream: AsyncReceiver<Option<Message>>,
}

impl MqttClient {
    pub fn new(options: MqttOptions) -> Result<Self, anyhow::Error> {
        let mut client: AsyncClient = AsyncClient::new(options.create_opts)?;
        // Get message stream before connecting
        let message_stream = client.get_stream(25);
        Ok(Self {
            conn_opts: options.conn_opts,
            client,
            message_stream,
        })
    }

    pub async fn connect(&mut self) {
        info!(target: "app", "connect - Connecting to the MQTT server with ConnectOptions...");
        while let Err(err) = self.client.connect(self.conn_opts.clone()).await {
            error!(target: "app", "connect - MQTT Connection error, retying in 30 seconds. Error = {:?}", err);
            tokio::time::sleep(Duration::from_millis(30000)).await;
        }
        info!(target: "app", "connect - MQTT Connection succeeded");
    }

    pub async fn reconnect(&self) -> paho_mqtt::Result<ServerResponse> {
        info!(target: "app", "reconnect - Reconnecting to the MQTT server...");
        self.client.reconnect().await
    }

    pub async fn subscribe(&mut self, topics_list: &[&str]) -> Result<(), paho_mqtt::Error> {
        info!(target: "app", "subscribe - Subscribing to the topics: {:?}", topics_list);

        let topics: Vec<String> = topics_list.iter().map(|s| s.to_string()).collect();
        info!(target: "app", "subscribe - Subscribing to MQTT topics: {:?}", topics);
        let qos = vec![0; topics.len()];
        // We subscribe to the topic(s) we want here.
        match self.client.subscribe_many(&topics, &qos).await {
            Ok(_) => {
                info!(target: "app", "subscribe - Subscription to the topics completed");
                Ok(())
            }
            Err(err) => {
                error!(target: "app", "subscribe - Cannot subscribe to topics. Error = {:?}", err);
                Err(err)
            }
        }
    }

    pub async fn get_next_message(&mut self) -> Option<Option<Message>> {
        self.message_stream.next().await
    }

    pub async fn disconnect(&mut self) -> paho_mqtt::Result<ServerResponse> {
        self.client.disconnect(None).await
    }
}
