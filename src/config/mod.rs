use std::env;
use std::fmt;

use dotenvy::dotenv;
use serde::Deserialize;
use tracing::info;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::writer::MakeWriterExt;

/// Which runtime environment the application is running in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnv {
    Testing,
    Production,
}

impl AppEnv {
    /// Reads the `ENV` environment variable. Returns `Testing` only when the
    /// value is exactly `"testing"`; any other value (including absent) is
    /// treated as `Production`.
    pub fn from_env() -> Self {
        match env::var("ENV").as_deref() {
            Ok("testing") => Self::Testing,
            _ => Self::Production,
        }
    }

    pub fn is_testing(&self) -> bool {
        matches!(self, Self::Testing)
    }
}

#[derive(Deserialize)]
pub struct Env {
    pub log_level: Option<String>,
    pub mongodb_url: String,
    pub redis_uri: String,
    pub redis_replay_uri: Option<String>,
    pub redis_username: String,
    pub redis_password: String,
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

impl fmt::Debug for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Env")
            .field("log_level", &self.log_level)
            .field("mongodb_url = {}", &"[REDACTED]")
            .field("redis_uri = {}", &"[REDACTED]")
            .field("redis_replay_uri = {}", &"[REDACTED]")
            .field("redis_username = {}", &self.redis_username)
            .field("redis_password = {}", &"[REDACTED]")
            .field("mqtt_url = {}", &self.mqtt_url)
            .field("mqtt_port = {}", &self.mqtt_port)
            .field("mqtt_client_id = {}", &self.mqtt_client_id)
            .field("mqtt_auth = {}", &self.mqtt_auth)
            .field("mqtt_user = {}", &"[REDACTED]")
            .field("mqtt_tls = {}", &self.mqtt_tls)
            .field("root_ca = {}", &self.root_ca)
            .field("mqtt_cert_file = {}", &self.mqtt_cert_file)
            .field("mqtt_key_file = {}", &self.mqtt_key_file)
            .finish()
    }
}

pub fn init() -> (Env, AppEnv) {
    // Load the .env file
    dotenv().ok();
    let env = envy::from_env::<Env>().expect("failed to parse environment variables");
    let app_env = AppEnv::from_env();

    // Configure logging if not in test env.
    // We use set_global_default (not .init()) intentionally: .init() would also install
    // a LogTracer bridge for the `log` crate, which prevents Rocket from installing its
    // own RocketLogger. Without RocketLogger, Rocket's startup output (routes, config,
    // launched URL) is silently dropped. By skipping LogTracer, Rocket gets to install
    // its own logger and prints its startup info directly to stdout.
    if !app_env.is_testing() {
        let stdout_max_level =
            env.log_level.as_deref().and_then(|s| s.parse::<tracing::Level>().ok()).unwrap_or(tracing::Level::DEBUG);
        let stdout = std::io::stdout.with_filter(|meta| meta.target() == "app").with_max_level(stdout_max_level);
        let debug_file = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("info")
            .filename_suffix("log")
            .max_log_files(5)
            .build("./logs")
            .expect("initializing rolling info_file appender failed")
            .with_max_level(tracing::Level::INFO);
        let error_file = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("error")
            .filename_suffix("log")
            .max_log_files(5)
            .build("./logs")
            .expect("initializing rolling error_file appender failed")
            .with_filter(|meta| meta.target() == "app")
            .with_max_level(tracing::Level::ERROR);
        let writer = debug_file.and(error_file).and(stdout);
        let subscriber = tracing_subscriber::fmt()
            .compact()
            .with_writer(writer)
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("Unable to install global subscriber");
    }

    info!(target: "app", "Starting application...");

    // Print .env vars
    print_env(&env);
    (env, app_env)
}

fn print_env(env: &Env) {
    info!(target: "app", "log_level = {}", env.log_level.as_deref().unwrap_or("debug"));
    info!(target: "app", "mongodb_url = [REDACTED]");
    info!(target: "app", "redis_uri = [REDACTED]");
    info!(target: "app", "redis_replay_uri = [REDACTED]");
    info!(target: "app", "redis_username = {}", env.redis_username);
    info!(target: "app", "redis_password = {}", !env.redis_password.is_empty());
    info!(target: "app", "mqtt_url = {}", env.mqtt_url);
    info!(target: "app", "mqtt_port = {}", env.mqtt_port);
    info!(target: "app", "mqtt_client_id = {}", env.mqtt_client_id);
    info!(target: "app", "mqtt_auth = {}", env.mqtt_auth);
    info!(target: "app", "mqtt_user = [REDACTED]");
    info!(target: "app", "mqtt_password = [REDACTED]");
    info!(target: "app", "mqtt_tls = {}", env.mqtt_tls);
    info!(target: "app", "root_ca = {}", env.root_ca);
    info!(target: "app", "mqtt_cert_file = {}", env.mqtt_cert_file);
    info!(target: "app", "mqtt_key_file = {}", env.mqtt_key_file);
}
