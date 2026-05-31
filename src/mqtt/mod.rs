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
            debug!(target: "app", "get_string_payload - MQTT utf8 payload accepted, payload_bytes={}", msg.payload().len());
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
    use crate::models::get_msg_byte;
    use crate::models::topic::Topic;
    use crate::mqtt::{MAX_PAYLOAD_BYTES, get_bytes_from_payload, get_string_payload};
    use paho_mqtt::Message;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::str::from_utf8;

    fn get_expected_json_string(device_uuid: &str, feature_uuid: &str, topic: &Topic) -> String {
        json!({
            "deviceUuid": device_uuid,
            "featureUuid": feature_uuid,
            "timestamp": 1777630000i64,
            "nonce": "00112233445566778899aabbccddeeff",
            "signature": "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
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
    fn ok_get_bytes_from_payload() {
        // create a paho_mqtt::Message
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "6ba7ed96-a041-44a5-8b90-98e66eacfeee";
        let topic = Topic::new(&format!("online/{}/features/{}", device_uuid, feature_uuid)).unwrap();
        let msg_payload = r#"{"deviceUuid":""#.to_owned()
            + device_uuid
            + r#"", "featureUuid":""#
            + feature_uuid
            + r#"", "timestamp":1777630000, "nonce":"00112233445566778899aabbccddeeff", "signature":"aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"#
            + r#"","payload":{}}"#;
        let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, msg_payload.as_str()).unwrap();
        let message = Message::new(format!("online/{}/features/{}", device_uuid, feature_uuid), msg_byte_arr, 0);

        // call function get_bytes_from_payload
        let bytes = get_bytes_from_payload(&message).unwrap();

        // check result
        let result = from_utf8(bytes.as_slice()).unwrap();
        let expected_value = get_expected_json_string(device_uuid, feature_uuid, &topic);
        assert_eq!(result.to_string(), expected_value);
    }

    #[test]
    fn get_string_payload_rejects_non_utf8_payload() {
        let message = Message::new("online/device/features/feature", vec![0xff, 0xfe], 0);

        let err = get_string_payload(&message).expect_err("invalid UTF-8 must be rejected");

        assert_eq!(err.to_string(), "Cannot parse message payload error");
    }

    #[test]
    fn get_string_payload_rejects_payload_over_size_limit() {
        let message = Message::new("online/device/features/feature", vec![b'a'; MAX_PAYLOAD_BYTES + 1], 0);

        let err = get_string_payload(&message).expect_err("oversized payload must be rejected");

        assert_eq!(err.to_string(), "Message payload exceeds maximum allowed size");
    }

    #[test]
    fn get_bytes_from_payload_rejects_invalid_topic() {
        let message = Message::new("invalid/topic", "{}", 0);

        let err = get_bytes_from_payload(&message).expect_err("invalid topic must be rejected");

        assert_eq!(err.to_string(), "Cannot parse message payload error");
    }

    #[test]
    fn get_bytes_from_payload_rejects_invalid_json_payload() {
        let message = Message::new("online/device/features/feature", "not-json", 0);

        let err = get_bytes_from_payload(&message).expect_err("invalid JSON must be rejected");

        assert!(err.to_string().contains("expected ident"));
    }
}
