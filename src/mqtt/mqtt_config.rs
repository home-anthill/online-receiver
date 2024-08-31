use std::string::String;

use crate::config::Env;
use crate::mqtt::COMBINED_CA_FILES_PATH;

pub struct MqttConfig {
    pub url: String,
    pub port: u16,
    pub client_id: String,
    pub auth: bool,
    pub user: String,
    pub password: String,
    pub tls: bool,
    pub root_ca_file: String,
    pub cert_file: String,
    pub key_file: String,
    pub ca_files_path: String,
}

impl MqttConfig {
    pub fn new(env: &Env) -> Self {
        Self {
            url: env.mqtt_url.clone(),
            port: env.mqtt_port,
            client_id: env.mqtt_client_id.clone(),
            auth: env.mqtt_auth,
            user: env.mqtt_user.clone(),
            password: env.mqtt_password.clone(),
            tls: env.mqtt_tls,
            root_ca_file: env.root_ca.clone(),
            cert_file: env.mqtt_cert_file.clone(),
            key_file: env.mqtt_key_file.clone(),
            ca_files_path: COMBINED_CA_FILES_PATH.to_string(),
        }
    }
}
