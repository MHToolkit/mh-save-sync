use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha256 = Hmac<Sha256>;

pub const CRYPTO_SUITE_V1: u16 = 1;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("invalid secret length")]
    InvalidSecretLength,
    #[error("hkdf expansion failed")]
    Hkdf,
    #[error("aead operation failed")]
    Aead,
    #[error("ed25519 verification failed")]
    Signature,
    #[error("cbor encoding failed: {0}")]
    Cbor(String),
    #[error("mnemonic error: {0}")]
    Mnemonic(String),
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct AccountKeys {
    pub auth: [u8; 32],
    pub root_signing_seed: [u8; 32],
    pub wrapping: [u8; 32],
    pub dedupe: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedBlob {
    pub suite: u16,
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCertificateBody {
    pub cert_version: u16,
    #[serde(with = "serde_bytes")]
    pub account_root_public: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub cert_id: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub device_public: Vec<u8>,
    pub issued_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub capabilities: u64,
    pub crypto_suites: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCertificate {
    pub body: DeviceCertificateBody,
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

pub fn generate_recovery_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    secret
}

pub fn recovery_phrase_from_secret(secret: &[u8; 32]) -> Result<String, CryptoError> {
    let mnemonic = bip39::Mnemonic::from_entropy_in(bip39::Language::English, secret)
        .map_err(|e| CryptoError::Mnemonic(e.to_string()))?;
    Ok(mnemonic.to_string())
}

pub fn secret_from_recovery_phrase(phrase: &str) -> Result<[u8; 32], CryptoError> {
    let mnemonic = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, phrase)
        .map_err(|e| CryptoError::Mnemonic(e.to_string()))?;
    let entropy = mnemonic.to_entropy();
    if entropy.len() != 32 {
        return Err(CryptoError::InvalidSecretLength);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&entropy);
    Ok(out)
}

pub fn derive_account_keys(secret: &[u8; 32]) -> Result<AccountKeys, CryptoError> {
    let salt = Sha256::digest(b"mh-save-sync/account-root/v1");
    let hk = Hkdf::<Sha256>::new(Some(&salt), secret);
    let mut auth = [0u8; 32];
    let mut root_signing_seed = [0u8; 32];
    let mut wrapping = [0u8; 32];
    let mut dedupe = [0u8; 32];
    hk.expand(b"mh-save-sync/auth/v1", &mut auth)
        .map_err(|_| CryptoError::Hkdf)?;
    hk.expand(b"mh-save-sync/root-signing-seed/v1", &mut root_signing_seed)
        .map_err(|_| CryptoError::Hkdf)?;
    hk.expand(b"mh-save-sync/content-wrapping/v1", &mut wrapping)
        .map_err(|_| CryptoError::Hkdf)?;
    hk.expand(b"mh-save-sync/dedupe/v1", &mut dedupe)
        .map_err(|_| CryptoError::Hkdf)?;
    Ok(AccountKeys {
        auth,
        root_signing_seed,
        wrapping,
        dedupe,
    })
}

pub fn account_handle(keys: &AccountKeys) -> String {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(&keys.auth).expect("HMAC accepts arbitrary key length");
    mac.update(b"mh-save-sync/account-handle/v1");
    hex::encode(&mac.finalize().into_bytes()[0..20])
}

pub fn account_root_signing_key(keys: &AccountKeys) -> SigningKey {
    SigningKey::from_bytes(&keys.root_signing_seed)
}

pub fn chunk_id(keys: &AccountKeys, plaintext_chunk: &[u8]) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&keys.dedupe)
        .expect("HMAC accepts arbitrary key length");
    mac.update(b"mh-save-sync/chunk-id/v1\0");
    mac.update(plaintext_chunk);
    hex::encode(mac.finalize().into_bytes())
}

fn aead_key(keys: &AccountKeys, label: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&keys.wrapping)
        .expect("HMAC accepts arbitrary key length");
    mac.update(label);
    let digest = mac.finalize().into_bytes();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest[..32]);
    key
}

pub fn encrypt_bytes(
    keys: &AccountKeys,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<EncryptedBlob, CryptoError> {
    let key_bytes = aead_key(keys, b"mh-save-sync/aead-key/v1");
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            chacha20poly1305::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Aead)?;
    Ok(EncryptedBlob {
        suite: CRYPTO_SUITE_V1,
        nonce,
        ciphertext,
    })
}

pub fn decrypt_bytes(
    keys: &AccountKeys,
    aad: &[u8],
    blob: &EncryptedBlob,
) -> Result<Vec<u8>, CryptoError> {
    if blob.suite != CRYPTO_SUITE_V1 {
        return Err(CryptoError::Aead);
    }
    let key_bytes = aead_key(keys, b"mh-save-sync/aead-key/v1");
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    cipher
        .decrypt(
            XNonce::from_slice(&blob.nonce),
            chacha20poly1305::aead::Payload {
                msg: &blob.ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Aead)
}

pub fn deterministic_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, CryptoError> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out).map_err(|e| CryptoError::Cbor(e.to_string()))?;
    Ok(out)
}

pub fn issue_device_certificate(
    account_key: &SigningKey,
    device_public: &VerifyingKey,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    capabilities: u64,
) -> Result<DeviceCertificate, CryptoError> {
    let mut cert_id = vec![0u8; 16];
    OsRng.fill_bytes(&mut cert_id);
    let body = DeviceCertificateBody {
        cert_version: 1,
        account_root_public: account_key.verifying_key().to_bytes().to_vec(),
        cert_id,
        device_public: device_public.to_bytes().to_vec(),
        issued_at_unix_seconds,
        expires_at_unix_seconds,
        capabilities,
        crypto_suites: vec![CRYPTO_SUITE_V1],
    };
    let mut msg = b"mh-save-sync/device-certificate/v1\0".to_vec();
    msg.extend(deterministic_cbor(&body)?);
    let sig = account_key.sign(&msg);
    Ok(DeviceCertificate {
        body,
        signature: sig.to_bytes().to_vec(),
    })
}

pub fn verify_device_certificate(cert: &DeviceCertificate) -> Result<(), CryptoError> {
    let root: [u8; 32] = cert
        .body
        .account_root_public
        .as_slice()
        .try_into()
        .map_err(|_| CryptoError::Signature)?;
    let root = VerifyingKey::from_bytes(&root).map_err(|_| CryptoError::Signature)?;
    let sig_bytes: [u8; 64] = cert
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| CryptoError::Signature)?;
    let sig = Signature::from_bytes(&sig_bytes);
    let mut msg = b"mh-save-sync/device-certificate/v1\0".to_vec();
    msg.extend(deterministic_cbor(&cert.body)?);
    root.verify(&msg, &sig).map_err(|_| CryptoError::Signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_round_trip_and_hkdf_domains_are_stable() {
        let secret = [7u8; 32];
        let phrase = recovery_phrase_from_secret(&secret).unwrap();
        assert_eq!(secret_from_recovery_phrase(&phrase).unwrap(), secret);
        let keys = derive_account_keys(&secret).unwrap();
        assert_ne!(keys.auth, keys.wrapping);
        assert_eq!(account_handle(&keys).len(), 40);
    }

    #[test]
    fn aead_wrong_aad_fails_closed_and_chunk_id_is_account_scoped() {
        let a = derive_account_keys(&[1u8; 32]).unwrap();
        let b = derive_account_keys(&[2u8; 32]).unwrap();
        let plaintext = b"hunter save bytes";
        assert_ne!(chunk_id(&a, plaintext), chunk_id(&b, plaintext));
        let blob = encrypt_bytes(&a, b"aad-1", plaintext).unwrap();
        assert_eq!(decrypt_bytes(&a, b"aad-1", &blob).unwrap(), plaintext);
        assert!(decrypt_bytes(&a, b"aad-2", &blob).is_err());
        assert!(decrypt_bytes(&b, b"aad-1", &blob).is_err());
    }

    #[test]
    fn device_certificate_verifies_and_tamper_fails() {
        let keys = derive_account_keys(&[3u8; 32]).unwrap();
        let root = account_root_signing_key(&keys);
        let device = SigningKey::from_bytes(&[4u8; 32]);
        let mut cert = issue_device_certificate(&root, &device.verifying_key(), 1, 2, 7).unwrap();
        verify_device_certificate(&cert).unwrap();
        cert.body.capabilities = 8;
        assert!(verify_device_certificate(&cert).is_err());
    }
}
