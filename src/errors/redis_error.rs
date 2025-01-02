use thiserror::Error;

// custom error, based on 'thiserror' library
#[derive(Error, Debug)]
pub enum RedisError {
    #[error("Cannot check if key exists error")]
    IsExistsError,
    #[error("Cannot set values error")]
    HsetError,
    #[error("Set values result error")]
    HsetResultError,
}
