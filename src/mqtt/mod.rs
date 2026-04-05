use paho_mqtt::Message;
use tracing::{debug, error};

use crate::errors::message_error::MessageError;
use crate::models::get_msg_byte;
use crate::models::topic::Topic;

pub mod mqtt_client;
pub mod mqtt_config;
pub mod mqtt_options;

const COMBINED_CA_FILES_PATH: &str = "/tmp/rootca_and_cert.pem";
const MAX_PAYLOAD_BYTES: usize = 65_536; // 64 KiB

pub fn get_bytes_from_payload(msg: &Message) -> Result<Vec<u8>, anyhow::Error> {
    let payload = get_string_payload(msg)?;
    let topic = Topic::new(msg.topic()).inspect_err(|err| {
        error!(target: "app", "get_bytes_from_payload - cannot parse MQTT topic '{}': {:?}", msg.topic(), err);
    })?;
    debug!(target: "app", "get_bytes_from_payload - MQTT message topic = {}", &topic);
    get_msg_byte(&topic, &payload)
}

pub fn get_string_payload(msg: &Message) -> Result<String, anyhow::Error> {
    if msg.payload().len() > MAX_PAYLOAD_BYTES {
        error!(target: "app", "get_string_payload - MQTT payload too large: {} bytes (max {})", msg.payload().len(), MAX_PAYLOAD_BYTES);
        return Err(anyhow::Error::from(MessageError::PayloadTooLargeError));
    }
    match std::str::from_utf8(msg.payload()) {
        Ok(res) => {
            debug!(target: "app", "get_string_payload - MQTT utf8 payload_str: {}", res);
            Ok(res.to_string())
        }
        Err(err) => {
            error!(target: "app", "get_string_payload - Cannot read MQTT message payload as utf8. Error = {:?}", err);
            Err(anyhow::Error::from(MessageError::ParseMessageError))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::init;
    use crate::models::get_msg_byte;
    use crate::models::topic::Topic;
    use crate::mqtt::get_bytes_from_payload;
    use paho_mqtt::Message;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::str::from_utf8;

    fn get_expected_json_string(api_token: &str, device_uuid: &str, feature_uuid: &str, topic: &Topic) -> String {
        json!({
            "apiToken": api_token,
            "deviceUuid": device_uuid,
            "featureUuid": feature_uuid,
            "topic": {
                "family": topic.family,
                "deviceUuid": topic.device_uuid,
                "featureUuid": topic.feature_uuid,
            },
            "payload": {}
        })
        .to_string()
    }

    #[test]
    #[test_log::test]
    fn ok_get_bytes_from_payload() {
        // init logger and env
        let _ = init();

        // create a paho_mqtt::Message
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "6ba7ed96-a041-44a5-8b90-98e66eacfeee";
        let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
        let topic = Topic::new(&format!("online/{}/features/{}", device_uuid, feature_uuid)).unwrap();
        let msg_payload = r#"{"apiToken":""#.to_owned()
            + api_token
            + r#"", "deviceUuid":""#
            + device_uuid
            + r#"", "featureUuid":""#
            + feature_uuid
            + r#"","payload":{}}"#;
        let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, msg_payload.as_str()).unwrap();
        let message = Message::new(format!("online/{}/features/{}", device_uuid, feature_uuid), msg_byte_arr, 0);

        // call function get_bytes_from_payload
        let bytes = get_bytes_from_payload(&message).unwrap();

        // check result
        let result = from_utf8(bytes.as_slice()).unwrap();
        let expected_value = get_expected_json_string(api_token, device_uuid, feature_uuid, &topic);
        assert_eq!(result.to_string(), expected_value);
    }
}
