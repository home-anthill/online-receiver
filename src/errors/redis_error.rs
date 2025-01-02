use thiserror::Error;

// custom error, based on 'thiserror' library
#[derive(Error, Debug)]
pub enum RedisError {
    #[error("Cannot check if key exists error")]
    IsExistsError,
    #[error("Cannot HSet values error")]
    HSetError,
    #[error("HSet values result error")]
    HSetResultError,
}
