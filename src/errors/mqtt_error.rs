use thiserror::Error;

// custom error, based on 'thiserror' library
#[derive(Error, Debug)]
pub enum MqttError {
    #[error("file {0} not found error")]
    FileNotFound(String),
}
