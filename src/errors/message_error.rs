use thiserror::Error;

// custom error, based on 'thiserror' library
#[derive(Error, Debug)]
pub enum MessageError {
    #[error("Received empty message error")]
    EmptyMessageError,
    #[error("Cannot parse message payload error")]
    ParseMessageError,
}
