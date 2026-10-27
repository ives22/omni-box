use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

/// Derive a 32-byte encryption key from a master password and 16-byte salt using Argon2id
pub fn derive_key_argon2(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut output_key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut output_key)
        .map_err(|e| format!("Argon2 derivation failed: {}", e))?;
    Ok(output_key)
}

pub fn generate_random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

/// Encrypt plaintext bytes using AES-256-GCM
pub fn encrypt_aes256_gcm(
    key: &[u8; 32],
    plaintext: &[u8],
) -> Result<([u8; 12], Vec<u8>), String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce_bytes = generate_random_bytes::<12>();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encryption failed: {}", e))?;
    Ok((nonce_bytes, ciphertext))
}

/// Decrypt ciphertext bytes using AES-256-GCM
pub fn decrypt_aes256_gcm(
    key: &[u8; 32],
    nonce_bytes: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    if nonce_bytes.len() != 12 {
        return Err("Invalid nonce length".to_string());
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "主密码错误或密文已损坏".to_string())
}

/// Decode Base32 (RFC 4648) secret string for TOTP
fn decode_base32(input: &str) -> Result<Vec<u8>, String> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=' && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();

    let mut bits: u32 = 0;
    let mut value: u32 = 0;
    let mut out = Vec::new();

    for ch in cleaned.bytes() {
        let idx = alphabet
            .iter()
            .position(|&b| b == ch)
            .ok_or_else(|| format!("Invalid Base32 char: {}", ch as char))?;
        value = (value << 5) | (idx as u32);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((value >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// Generate RFC 6238 TOTP 6-digit code and remaining seconds
pub fn generate_totp_code(secret_base32: &str, period: u64, digits: u32) -> Result<(String, u64), String> {
    let secret = decode_base32(secret_base32)?;
    if secret.is_empty() {
        return Err("Empty secret".to_string());
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let counter = now / period;
    let remaining = period - (now % period);
    let counter_bytes = counter.to_be_bytes();

    let mut mac = <HmacSha1 as Mac>::new_from_slice(&secret).map_err(|e| e.to_string())?;
    mac.update(&counter_bytes);
    let hash = mac.finalize().into_bytes();

    let offset = (hash[hash.len() - 1] & 0x0f) as usize;
    let binary = (((hash[offset] & 0x7f) as u32) << 24)
        | ((hash[offset + 1] as u32) << 16)
        | ((hash[offset + 2] as u32) << 8)
        | (hash[offset + 3] as u32);

    let modulo = 10_u32.pow(digits);
    let otp = binary % modulo;
    let formatted = format!("{:0width$}", otp, width = digits as usize);

    Ok((formatted, remaining))
}
