use std::fs::{read, remove_file, File};
use std::io::Write;
use std::path::Path;
use std::string::String;
use std::{env, time::Duration};

use log::{debug, error, info, warn};
use paho_mqtt::{
    ConnectOptions, ConnectOptionsBuilder, CreateOptions, CreateOptionsBuilder, Message, SslOptions, SslOptionsBuilder,
};

use crate::errors::mqtt_error::MqttError;
use crate::mqtt::mqtt_config::MqttConfig;
use crate::mqtt::COMBINED_CA_FILES_PATH;

pub struct MqttOptions {
    pub create_opts: CreateOptions,
    pub conn_opts: ConnectOptions,
}

impl MqttOptions {
    pub fn new(mqtt_config: &MqttConfig) -> Self {
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
            let merge_result = Self::merge_ca_files(&mqtt_config.root_ca_file, &mqtt_config.cert_file);
            if let Err(err) = merge_result {
                error!(target: "app", "cannot merge MQTT CA files, err = {:?}", err);
                panic!("cannot merge MQTT CA files");
            }
        }

        let create_options = CreateOptionsBuilder::new()
            .server_uri(mqtt_uri)
            .client_id(&mqtt_config.client_id)
            .finalize();

        info!(target: "app", "Creating MQTT ConnectOptions...");
        let conn_opts_result = Self::build_connect_options(
            &mqtt_config.auth,
            &mqtt_config.user,
            &mqtt_config.password,
            &mqtt_config.tls,
            &mqtt_config.cert_file,
            &mqtt_config.key_file,
            &mqtt_config.ca_files_path,
        );

        if let Err(err) = conn_opts_result {
            error!(target: "app", "cannot instantiate MqttOptions, err = {:?}", err);
            panic!("cannot instantiate MqttOptions!");
        }

        Self {
            create_opts: create_options,
            conn_opts: conn_opts_result.unwrap(),
        }
    }

    fn merge_ca_files(root_ca: &String, mqtt_cert_file: &String) -> Result<(), anyhow::Error> {
        // re-create a new file appending two certificates:
        // - ROOT_CA file (ISRG_Root_X1.pem in case of Let's Encrypt)
        // - MQTT_CERT_FILE file (cert.pem in case of Let's Encrypt)
        if Path::new(COMBINED_CA_FILES_PATH).exists() {
            remove_file(COMBINED_CA_FILES_PATH)?;
        }
        let mut combined_root_ca = File::create(COMBINED_CA_FILES_PATH)?;
        debug!(target: "app", "merge_ca_files - {} file created", COMBINED_CA_FILES_PATH);
        let root_ca_vec = read(root_ca)?;
        let mqtt_cert_file_vec = read(mqtt_cert_file)?;
        combined_root_ca.write_all(&root_ca_vec)?;
        combined_root_ca.write_all(b"\n")?;
        combined_root_ca.write_all(&mqtt_cert_file_vec)?;
        Ok(())
    }

    fn build_connect_options(
        mqtt_auth: &bool,
        mqtt_user: &String,
        mqtt_password: &String,
        mqtt_tls: &bool,
        mqtt_cert_file: &str,
        mqtt_key_file: &str,
        combined_ca_files_path: &str,
    ) -> Result<ConnectOptions, anyhow::Error> {
        // Define the set of options for the connection
        let lwt = Message::new("test", "Subscriber lost connection", 1);
        let mut new_con_builder = ConnectOptionsBuilder::new();
        let connect_options_builder = new_con_builder
            .keep_alive_interval(Duration::from_secs(20))
            // Using a "persistent" (non-clean) session
            // so the broker keeps subscriptions and messages through reconnects
            .clean_session(false)
            .will_message(lwt);

        if *mqtt_auth {
            warn!(target: "app", "build_connect_options - MQTT authentication is enabled, setting username and password");
            connect_options_builder.user_name(mqtt_user).password(mqtt_password);
        }

        if *mqtt_tls {
            warn!(target: "app", "build_connect_options - MQTT TLS is enabled, creating ConnectOptions with certificates");
            match Self::build_ssl_options(mqtt_cert_file, mqtt_key_file, combined_ca_files_path) {
                Ok(ssl_options) => {
                    debug!(target: "app", "build_connect_options - MQTT ConnectOptions with SSL created successfully");
                    connect_options_builder.ssl_options(ssl_options);
                }
                Err(err) => {
                    error!(target: "app", "build_connect_options - Cannot create MQTT ConnectOptions with certificates, err = {:?}", err);
                    return Err(err);
                }
            }
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
            return Err(anyhow::Error::from(MqttError::FileNotFound("trust_store".to_string())));
        }
        if !key_store.exists() {
            error!(target: "app", "build_ssl_options - key_store file does not exist: {:?}", key_store);
            return Err(anyhow::Error::from(MqttError::FileNotFound("key_store".to_string())));
        }
        if !private_key.exists() {
            error!(target: "app", "build_ssl_options - private_key file does not exist: {:?}", private_key);
            return Err(anyhow::Error::from(MqttError::FileNotFound("private_key".to_string())));
        }

        debug!(target: "app", "build_ssl_options - trust_store {:?}", trust_store);
        debug!(target: "app", "build_ssl_options - key_store {:?}", key_store);
        debug!(target: "app", "build_ssl_options - private_key {:?}", private_key);

        let ssl_opts = SslOptionsBuilder::new()
            .trust_store(trust_store)
            .unwrap()
            .key_store(key_store)
            .unwrap()
            .private_key(private_key)
            .unwrap()
            .finalize();
        Ok(ssl_opts)
    }
}
