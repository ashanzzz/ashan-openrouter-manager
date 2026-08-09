use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::error::AppError;

pub struct Crypto { key: [u8; 32] }

impl Crypto {
    pub fn new(master: &str) -> Self {
        let digest = Sha256::digest(master.as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest);
        Self { key }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, AppError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mut combined = nonce_bytes.to_vec();
        combined.extend(ciphertext);
        Ok(STANDARD.encode(combined))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<String, AppError> {
        let bytes = STANDARD.decode(encoded).map_err(|e| anyhow::anyhow!(e))?;
        if bytes.len() < 13 { return Err(AppError::bad("invalid encrypted secret")); }
        let (nonce, ciphertext) = bytes.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let plaintext = cipher.decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| AppError::Unauthorized("APP_MASTER_KEY cannot decrypt stored secrets".into()))?;
        String::from_utf8(plaintext).map_err(|e| AppError::Internal(e.into()))
    }
}
