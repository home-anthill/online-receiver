use log::{debug, error};
use paho_mqtt::Message;
use std::string::String;

use crate::models::get_msg_byte;
use crate::models::topic::Topic;

pub mod mqtt_client;
pub mod mqtt_config;
pub mod mqtt_options;

const COMBINED_CA_FILES_PATH: &str = "./rootca_and_cert.pem";

pub fn get_bytes_from_payload(msg: &Message) -> Vec<u8> {
    let payload: String = get_string_payload(msg);
    let topic: Topic = Topic::new(msg.topic());
    debug!(target: "app", "get_bytes_from_payload - MQTT message topic = {}", &topic);
    let msg_byte: Vec<u8> = get_msg_byte(&topic, &payload);
    msg_byte
}

pub fn get_string_payload(msg: &Message) -> String {
    match std::str::from_utf8(msg.payload()) {
        Ok(res) => {
            debug!(target: "app", "get_string_payload - MQTT utf8 payload_str: {}", res);
            res.to_string()
        }
        Err(err) => {
            // this shouldn't happen, because payload in Message is a Vec<u8>
            error!(target: "app", "get_string_payload - Cannot read MQTT message payload as utf8. Error = {:?}", err);
            "".to_string()
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

    fn get_expected_json_string(uuid: &str, api_token: &str, topic: &Topic) -> String {
        json!({
            "uuid": uuid,
            "apiToken": api_token,
            "topic": {
                "root": topic.root,
                "deviceId": topic.device_id,
            },
            "payload": {}
        })
        .to_string()
    }

    #[test]
    fn ok_get_bytes_from_payload() {
        // init logger and env
        let _ = init();

        // create a paho_mqtt::Message
        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
        let topic: Topic = Topic::new(format!("online/{}", uuid).as_str());
        let msg_payload = r#"{"uuid":""#.to_owned() + uuid + r#"", "apiToken":""# + api_token + r#"","payload":{}}"#;
        let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, msg_payload.as_str());
        let message = Message::new(format!("online/{}", uuid), msg_byte_arr, 0);

        // call function get_bytes_from_payload
        let bytes = get_bytes_from_payload(&message);

        // check result
        let result = from_utf8(bytes.as_slice()).unwrap();
        let expected_value = get_expected_json_string(uuid, api_token, &topic);
        assert_eq!(result.to_string(), expected_value);
    }
}
