use std::fs::{OpenOptions, read};
use std::io::Write;
use std::{env, time::Duration};

use tracing::{debug, error, info, warn};

use paho_mqtt::{
    ConnectOptions, ConnectOptionsBuilder, CreateOptions, CreateOptionsBuilder, Message, SslOptions, SslOptionsBuilder,
};

use crate::errors::mqtt_error::MqttError;
use crate::mqtt::COMBINED_CA_FILES_PATH;
use crate::mqtt::mqtt_config::MqttConfig;

pub struct MqttOptions {
    pub create_opts: CreateOptions,
    pub conn_opts: ConnectOptions,
}

impl MqttOptions {
    pub fn new(mqtt_config: &MqttConfig) -> Result<Self, anyhow::Error> {
        let mqtt_uri = if mqtt_config.tls {
            format!("ssl://{}:{}", mqtt_config.url, mqtt_config.port)
        } else {
            format!("tcp://{}:{}", mqtt_config.url, mqtt_config.port)
        };
        info!(target: "app", "mqtt_uri = {}", &mqtt_uri);

        // Create CA file in 'COMBINED_CA_FILES_PATH' merging 'root_ca' and 'mqtt_cert_file',
        // otherwise, paho.mqtt.rust won't be able to connect.
        if mqtt_config.tls {
            info!(target: "app", "Preparing MQTT CA file");
            Self::merge_ca_files(&mqtt_config.root_ca_file, &mqtt_config.cert_file)?;
        }

        let create_options =
            CreateOptionsBuilder::new().server_uri(mqtt_uri).client_id(&mqtt_config.client_id).finalize();

        info!(target: "app", "Creating MQTT ConnectOptions...");
        let conn_opts = Self::build_connect_options(
            mqtt_config.auth,
            &mqtt_config.user,
            &mqtt_config.password,
            mqtt_config.tls,
            &mqtt_config.cert_file,
            &mqtt_config.key_file,
            &mqtt_config.ca_files_path,
        )?;

        Ok(Self { create_opts: create_options, conn_opts })
    }

    fn merge_ca_files(root_ca: &str, mqtt_cert_file: &str) -> Result<(), anyhow::Error> {
        // Re-create a combined CA file by atomically truncating (or creating) the target file.
        // Using truncate(true) avoids the TOCTOU race of a separate exists() + remove_file() check.
        let mut combined_root_ca =
            OpenOptions::new().write(true).create(true).truncate(true).open(COMBINED_CA_FILES_PATH)?;
        debug!(target: "app", "merge_ca_files - {} file created or truncated", COMBINED_CA_FILES_PATH);
        let root_ca_vec = read(root_ca)?;
        let mqtt_cert_file_vec = read(mqtt_cert_file)?;
        combined_root_ca.write_all(&root_ca_vec)?;
        combined_root_ca.write_all(b"\n")?;
        combined_root_ca.write_all(&mqtt_cert_file_vec)?;
        Ok(())
    }

    fn build_connect_options(
        mqtt_auth: bool,
        mqtt_user: &str,
        mqtt_password: &str,
        mqtt_tls: bool,
        mqtt_cert_file: &str,
        mqtt_key_file: &str,
        combined_ca_files_path: &str,
    ) -> Result<ConnectOptions, anyhow::Error> {
        // Define the set of options for the connection
        let lwt = Message::new("online/lwt", "Subscriber lost connection", 1);
        let mut new_con_builder = ConnectOptionsBuilder::new();
        let connect_options_builder = new_con_builder
            .keep_alive_interval(Duration::from_secs(20))
            // Online heartbeats are ephemeral. Start with a clean session so
            // the broker does not retain stale subscriptions across restarts.
            .clean_session(true)
            .will_message(lwt);

        if mqtt_auth {
            warn!(target: "app", "build_connect_options - MQTT authentication is enabled, setting username and password");
            connect_options_builder.user_name(mqtt_user).password(mqtt_password);
        }

        if mqtt_tls {
            warn!(target: "app", "build_connect_options - MQTT TLS is enabled, creating ConnectOptions with certificates");
            let ssl_options = Self::build_ssl_options(mqtt_cert_file, mqtt_key_file, combined_ca_files_path)
                .inspect_err(|err| {
                    error!(target: "app", "build_connect_options - Cannot create MQTT ConnectOptions with certificates, err = {:?}", err);
                })?;
            connect_options_builder.ssl_options(ssl_options);
        }
        Ok(connect_options_builder.finalize())
    }

    fn build_ssl_options(
        mqtt_cert_file: &str,
        mqtt_key_file: &str,
        combined_ca_files_path: &str,
    ) -> Result<SslOptions, anyhow::Error> {
        // I need COMBINED_CA_FILES_PATH, check function `merge_ca_files` above.
        let mut trust_store = env::current_dir()?;
        trust_store.push(combined_ca_files_path);
        let mut key_store = env::current_dir()?;
        key_store.push(mqtt_cert_file);
        let mut private_key = env::current_dir()?;
        private_key.push(mqtt_key_file);
        if !trust_store.exists() {
            error!(target: "app", "build_ssl_options - trust_store file does not exist: {:?}", trust_store);
            return Err(MqttError::FileNotFound("trust_store".to_string()).into());
        }
        if !key_store.exists() {
            error!(target: "app", "build_ssl_options - key_store file does not exist: {:?}", key_store);
            return Err(MqttError::FileNotFound("key_store".to_string()).into());
        }
        if !private_key.exists() {
            error!(target: "app", "build_ssl_options - private_key file does not exist: {:?}", private_key);
            return Err(MqttError::FileNotFound("private_key".to_string()).into());
        }

        debug!(target: "app", "build_ssl_options - trust_store {:?}", trust_store);
        debug!(target: "app", "build_ssl_options - key_store {:?}", key_store);
        debug!(target: "app", "build_ssl_options - private_key {:?}", private_key);

        let mut ssl_builder = SslOptionsBuilder::new();
        ssl_builder.trust_store(trust_store).map_err(|e| MqttError::SslConfigError(e.to_string()))?;
        ssl_builder.key_store(key_store).map_err(|e| MqttError::SslConfigError(e.to_string()))?;
        ssl_builder.private_key(private_key).map_err(|e| MqttError::SslConfigError(e.to_string()))?;
        let ssl_opts = ssl_builder.finalize();
        Ok(ssl_opts)
    }
}
