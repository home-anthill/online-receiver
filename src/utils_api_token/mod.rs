use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Nonce};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

const API_TOKEN_NONCE_SIZE: usize = 12;

pub fn decrypt_api_token(encrypted: &str) -> Result<String, String> {
    let raw = URL_SAFE_NO_PAD.decode(encrypted).map_err(|err| err.to_string())?;
    if raw.len() <= API_TOKEN_NONCE_SIZE {
        return Err("encrypted api token is too short".to_string());
    }
    let cipher = Aes256Gcm::new_from_slice(&api_token_encryption_key()?).map_err(|err| err.to_string())?;
    let nonce = Nonce::<Aes256Gcm>::try_from(&raw[..API_TOKEN_NONCE_SIZE]).map_err(|err| err.to_string())?;
    let plaintext = cipher.decrypt(&nonce, &raw[API_TOKEN_NONCE_SIZE..]).map_err(|err| err.to_string())?;
    String::from_utf8(plaintext).map_err(|err| err.to_string())
}

fn api_token_encryption_key() -> Result<[u8; 32], String> {
    let key =
        std::env::var("API_TOKEN_ENCRYPTION_KEY").map_err(|_| "API_TOKEN_ENCRYPTION_KEY is required".to_string())?;
    if let Ok(decoded) = URL_SAFE_NO_PAD.decode(&key)
        && decoded.len() == 32
    {
        return Ok(decoded.try_into().expect("length checked"));
    }
    if let Ok(decoded) = STANDARD.decode(&key)
        && decoded.len() == 32
    {
        return Ok(decoded.try_into().expect("length checked"));
    }
    if key.len() == 32 {
        return Ok(key.into_bytes().try_into().expect("length checked"));
    }
    Err("API_TOKEN_ENCRYPTION_KEY must be 32 raw bytes or base64-encoded 32 bytes".to_string())
}
