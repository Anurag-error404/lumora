//! Cryptographic primitives for the privacy vault.
//!
//! Design: envelope encryption. A random 32-byte **master key** (MK) encrypts
//! every file. The MK itself is wrapped (encrypted) with a **key-encryption
//! key** (KEK) derived from the user password via Argon2id. Only the wrapped
//! MK, the salt, and the KDF parameters are ever persisted — the password and
//! the plaintext MK never touch disk. Unlocking works by re-deriving the KEK
//! and verifying that the wrapped MK decrypts (the AEAD tag authenticates it),
//! so an incorrect password fails cleanly without a stored password hash.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroize;

use crate::error::{AppError, AppResult};

pub const MASTER_KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;
/// Blob layout: magic (4) + nonce (24) + ciphertext(+16 byte tag).
const MAGIC: &[u8; 4] = b"PV01";

/// 20 bytes = 160 bits of entropy, which encodes to exactly 32 base32 chars.
const RECOVERY_BYTES: usize = 20;
/// Crockford base32: no I, L, O or U, so codes can't be misread when copied.
const B32_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Argon2id cost parameters (persisted so unlock uses the same derivation even
/// if crate defaults change). Defaults follow OWASP guidance (~19 MiB, t=2).
#[derive(Debug, Clone, Copy)]
pub struct KdfParams {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            m_cost: 19_456,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

/// Fill a buffer with cryptographically secure random bytes.
fn random_bytes(buf: &mut [u8]) -> AppResult<()> {
    getrandom::fill(buf).map_err(|e| AppError::msg(format!("random source failed: {e}")))
}

pub fn random_salt() -> AppResult<[u8; SALT_LEN]> {
    let mut salt = [0u8; SALT_LEN];
    random_bytes(&mut salt)?;
    Ok(salt)
}

pub fn random_master_key() -> AppResult<[u8; MASTER_KEY_LEN]> {
    let mut key = [0u8; MASTER_KEY_LEN];
    random_bytes(&mut key)?;
    Ok(key)
}

/// Derive the key-encryption key from a password + salt using Argon2id.
fn derive_kek(password: &str, salt: &[u8], kdf: KdfParams) -> AppResult<[u8; MASTER_KEY_LEN]> {
    let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(MASTER_KEY_LEN))
        .map_err(|e| AppError::msg(format!("invalid KDF params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut kek = [0u8; MASTER_KEY_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut kek)
        .map_err(|e| AppError::msg(format!("key derivation failed: {e}")))?;
    Ok(kek)
}

fn cipher_for(key: &[u8; MASTER_KEY_LEN]) -> XChaCha20Poly1305 {
    // Key length is a compile-time constant, so this never fails.
    XChaCha20Poly1305::new_from_slice(key).expect("master key is 32 bytes")
}

fn nonce_from(bytes: &[u8]) -> AppResult<XNonce> {
    XNonce::try_from(bytes).map_err(|_| AppError::msg("corrupt vault: bad nonce length"))
}

/// Wrap (encrypt) the master key under a password-derived KEK.
/// Returns `(wrap_nonce, wrapped_key_with_tag)`.
pub fn wrap_master_key(
    password: &str,
    salt: &[u8],
    kdf: KdfParams,
    master_key: &[u8; MASTER_KEY_LEN],
) -> AppResult<(Vec<u8>, Vec<u8>)> {
    let mut kek = derive_kek(password, salt, kdf)?;
    let cipher = cipher_for(&kek);
    kek.zeroize();

    let mut nonce_bytes = [0u8; NONCE_LEN];
    random_bytes(&mut nonce_bytes)?;
    let nonce = nonce_from(&nonce_bytes)?;
    let wrapped = cipher
        .encrypt(&nonce, master_key.as_slice())
        .map_err(|_| AppError::msg("failed to wrap master key"))?;
    Ok((nonce_bytes.to_vec(), wrapped))
}

/// Unwrap (decrypt) the master key. A wrong password fails the AEAD check and
/// returns an "incorrect password" error rather than garbage.
pub fn unwrap_master_key(
    password: &str,
    salt: &[u8],
    kdf: KdfParams,
    wrap_nonce: &[u8],
    wrapped_key: &[u8],
) -> AppResult<[u8; MASTER_KEY_LEN]> {
    let nonce = nonce_from(wrap_nonce)?;
    let mut kek = derive_kek(password, salt, kdf)?;
    let cipher = cipher_for(&kek);
    kek.zeroize();

    let mut plaintext = cipher
        .decrypt(&nonce, wrapped_key)
        .map_err(|_| AppError::msg("incorrect password"))?;
    if plaintext.len() != MASTER_KEY_LEN {
        plaintext.zeroize();
        return Err(AppError::msg("corrupt vault: bad key length"));
    }
    let mut key = [0u8; MASTER_KEY_LEN];
    key.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(key)
}

/// Generate a one-time recovery code, formatted in dash-separated groups of
/// four (e.g. `4T9K-...`). The code itself is the secret: it is never stored,
/// only a second copy of the master key wrapped under a key derived from it.
pub fn generate_recovery_code() -> AppResult<String> {
    let mut raw = [0u8; RECOVERY_BYTES];
    random_bytes(&mut raw)?;

    // 160 bits is divisible by 5, so the encoding comes out even with no padding.
    let mut chars = Vec::with_capacity(RECOVERY_BYTES * 8 / 5);
    let mut bits: u32 = 0;
    let mut pending = 0u32;
    for &byte in raw.iter() {
        bits = (bits << 8) | byte as u32;
        pending += 8;
        while pending >= 5 {
            pending -= 5;
            chars.push(B32_ALPHABET[((bits >> pending) & 0x1f) as usize]);
        }
    }
    raw.zeroize();

    let grouped: Vec<String> = chars
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect();
    Ok(grouped.join("-"))
}

/// Canonicalise user-typed recovery codes so formatting and easily-confused
/// characters don't change the derived key: strip separators, upper-case, and
/// fold I/L to 1 and O to 0 (per Crockford base32).
pub fn normalize_recovery_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            other => other,
        })
        .collect()
}

/// Encrypt file contents into a self-describing blob (`magic || nonce || ct`).
pub fn encrypt_blob(master_key: &[u8; MASTER_KEY_LEN], plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let cipher = cipher_for(master_key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    random_bytes(&mut nonce_bytes)?;
    let nonce = nonce_from(&nonce_bytes)?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| AppError::msg("encryption failed"))?;

    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a blob produced by [`encrypt_blob`].
pub fn decrypt_blob(master_key: &[u8; MASTER_KEY_LEN], blob: &[u8]) -> AppResult<Vec<u8>> {
    let header = MAGIC.len() + NONCE_LEN;
    if blob.len() < header {
        return Err(AppError::msg("corrupt vault file: too short"));
    }
    if &blob[..MAGIC.len()] != MAGIC {
        return Err(AppError::msg("unrecognised vault file format"));
    }
    let nonce = nonce_from(&blob[MAGIC.len()..header])?;
    let cipher = cipher_for(master_key);
    cipher
        .decrypt(&nonce, &blob[header..])
        .map_err(|_| AppError::msg("decryption failed (wrong key or corrupt file)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_roundtrip_and_wrong_password() {
        let salt = random_salt().unwrap();
        let kdf = KdfParams::default();
        let mk = random_master_key().unwrap();
        let (nonce, wrapped) = wrap_master_key("correct horse", &salt, kdf, &mk).unwrap();

        let recovered = unwrap_master_key("correct horse", &salt, kdf, &nonce, &wrapped).unwrap();
        assert_eq!(recovered, mk);

        let err = unwrap_master_key("wrong pass", &salt, kdf, &nonce, &wrapped).unwrap_err();
        assert!(err.to_string().contains("incorrect password"));
    }

    #[test]
    fn recovery_codes_are_unique_and_normalise() {
        let code = generate_recovery_code().unwrap();
        // 32 base32 chars in 8 dash-separated groups of 4.
        assert_eq!(code.len(), 39);
        assert_eq!(code.matches('-').count(), 7);
        assert_ne!(code, generate_recovery_code().unwrap());

        let normalized = normalize_recovery_code(&code);
        assert_eq!(normalized.len(), 32);
        // Lower case, missing dashes, and confusable characters all canonicalise.
        assert_eq!(normalize_recovery_code(&code.to_lowercase()), normalized);
        assert_eq!(normalize_recovery_code(" abcd efgh "), "ABCDEFGH");
        assert_eq!(normalize_recovery_code("o0i1l"), "00111");
    }

    #[test]
    fn recovery_code_unwraps_the_same_master_key() {
        let mk = random_master_key().unwrap();
        let code = generate_recovery_code().unwrap();
        let salt = random_salt().unwrap();
        let kdf = KdfParams::default();
        let (nonce, wrapped) =
            wrap_master_key(&normalize_recovery_code(&code), &salt, kdf, &mk).unwrap();

        // Typed back in lower case without dashes, it still recovers the key.
        let typed = code.to_lowercase().replace('-', "");
        let recovered = unwrap_master_key(
            &normalize_recovery_code(&typed),
            &salt,
            kdf,
            &nonce,
            &wrapped,
        )
        .unwrap();
        assert_eq!(recovered, mk);
    }

    #[test]
    fn blob_roundtrip_and_tamper_detection() {
        let mk = random_master_key().unwrap();
        let data = b"the quick brown fox jumps over the lazy dog".repeat(10);
        let blob = encrypt_blob(&mk, &data).unwrap();
        // Ciphertext must not contain the plaintext.
        assert!(!blob.windows(4).any(|w| w == b"quic"));
        let decrypted = decrypt_blob(&mk, &blob).unwrap();
        assert_eq!(decrypted, data);

        // A different key cannot decrypt.
        let other = random_master_key().unwrap();
        assert!(decrypt_blob(&other, &blob).is_err());

        // Flipping a ciphertext byte is detected by the AEAD tag.
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(decrypt_blob(&mk, &tampered).is_err());
    }
}
