use dotenvy::dotenv;
use log::info;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Env {
    pub mongo_uri: String,
    pub mongo_db_name: String,
    pub mqtt_url: String,
    pub mqtt_port: u16,
    pub mqtt_client_id: String,
    pub mqtt_auth: bool,
    pub mqtt_user: String,
    pub mqtt_password: String,
    pub mqtt_tls: bool,
    pub root_ca: String,
    pub mqtt_cert_file: String,
    pub mqtt_key_file: String,
}

pub fn init() -> Env {
    // Init logger if not in testing environment
    let _ = log4rs::init_file("log4rs.yaml", Default::default());
    info!(target: "app", "Starting application...");
    // Load the .env file
    dotenv().ok();
    let env = envy::from_env::<Env>().ok().unwrap();
    // Print .env vars
    print_env(&env);
    env
}

fn print_env(env: &Env) {
    let mongo_uri = env.mongo_uri.clone();
    let mongo_db_name = env.mongo_db_name.clone();
    let mqtt_url = env.mqtt_url.clone();
    let mqtt_port = env.mqtt_port;
    let mqtt_client_id = env.mqtt_client_id.clone();
    let mqtt_auth = env.mqtt_auth;
    let mqtt_user = env.mqtt_user.clone();
    let mqtt_password = env.mqtt_password.clone();
    let mqtt_tls = env.mqtt_tls;
    let root_ca = env.root_ca.clone();
    let mqtt_cert_file = env.mqtt_cert_file.clone();
    let mqtt_key_file = env.mqtt_key_file.clone();
    info!(target: "app", "env = {:?}", env);
    info!(target: "app", "mongo_uri = {}", mongo_uri);
    info!(target: "app", "mongo_db_name = {}", mongo_db_name);
    info!(target: "app", "mqtt_url = {}", mqtt_url);
    info!(target: "app", "mqtt_port = {}", mqtt_port);
    info!(target: "app", "mqtt_client_id = {}", mqtt_client_id);
    info!(target: "app", "mqtt_auth = {}", mqtt_auth);
    info!(target: "app", "mqtt_user = {}", mqtt_user);
    info!(target: "app", "mqtt_password = {}", mqtt_password);
    info!(target: "app", "mqtt_tls = {}", mqtt_tls);
    info!(target: "app", "root_ca = {}", root_ca);
    info!(target: "app", "mqtt_cert_file = {}", mqtt_cert_file);
    info!(target: "app", "mqtt_key_file = {}", mqtt_key_file);
}
