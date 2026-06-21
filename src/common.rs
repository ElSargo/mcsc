use crate::actions::{AuthAction, AuthToken};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const NONCE_BYTES: usize = 32;
const SIGNATURE_CONTEXT: &[u8] = b"mcsc-v1-auth";
const MAX_CLOCK_SKEW: Duration = Duration::from_secs(5 * 60);

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("unable to get secure random bytes")]
    Random(#[from] getrandom::Error),
    #[error("invalid signing key")]
    InvalidKey,
    #[error("system clock is before the Unix epoch")]
    Clock,
}

/// Returns a random sequence of [A-Za-z].
pub fn random_letters(len: usize) -> Result<String, getrandom::Error> {
    let mut bytes = vec![0; len];
    getrandom::fill(&mut bytes)?;

    let mut string = String::with_capacity(len);
    for byte in bytes {
        string.push(ALPHABET[byte as usize % ALPHABET.len()] as char);
    }

    Ok(string)
}

pub fn signed_token(
    secret: &str,
    action: AuthAction,
    request_body: &[u8],
) -> Result<AuthToken, AuthError> {
    let mut nonce = vec![0; NONCE_BYTES];
    getrandom::fill(&mut nonce)?;

    let issued_at_unix_ms = unix_time_ms()?;
    let signature = sign(secret, action, &nonce, issued_at_unix_ms, request_body)?;

    Ok(AuthToken {
        nonce,
        signature,
        issued_at_unix_ms,
    })
}

pub fn verify_signed_token(
    secret: &str,
    action: AuthAction,
    token: &AuthToken,
    request_body: &[u8],
) -> bool {
    if !token_time_is_fresh(token.issued_at_unix_ms) {
        return false;
    }

    let Ok(mac) = mac(
        secret,
        action,
        &token.nonce,
        token.issued_at_unix_ms,
        request_body,
    ) else {
        return false;
    };

    mac.verify_slice(&token.signature).is_ok()
}

fn sign(
    secret: &str,
    action: AuthAction,
    nonce: &[u8],
    issued_at_unix_ms: u64,
    request_body: &[u8],
) -> Result<Vec<u8>, AuthError> {
    let mac = mac(secret, action, nonce, issued_at_unix_ms, request_body)?;
    Ok(mac.finalize().into_bytes().to_vec())
}

fn mac(
    secret: &str,
    action: AuthAction,
    nonce: &[u8],
    issued_at_unix_ms: u64,
    request_body: &[u8],
) -> Result<HmacSha256, AuthError> {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| AuthError::InvalidKey)?;

    mac.update(SIGNATURE_CONTEXT);
    mac.update(&(action as i32).to_be_bytes());
    mac.update(&issued_at_unix_ms.to_be_bytes());
    mac.update(&(nonce.len() as u64).to_be_bytes());
    mac.update(nonce);
    mac.update(&(request_body.len() as u64).to_be_bytes());
    mac.update(request_body);

    Ok(mac)
}

fn unix_time_ms() -> Result<u64, AuthError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AuthError::Clock)?;

    Ok(duration.as_millis() as u64)
}

fn token_time_is_fresh(issued_at_unix_ms: u64) -> bool {
    let Ok(now) = unix_time_ms() else {
        return false;
    };

    let skew = now.abs_diff(issued_at_unix_ms);
    skew <= MAX_CLOCK_SKEW.as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-that-is-long-enough-for-hmac";

    #[test]
    fn signed_token_verifies_matching_request() {
        let token = signed_token(SECRET, AuthAction::Command, b"say hello").unwrap();

        assert!(verify_signed_token(
            SECRET,
            AuthAction::Command,
            &token,
            b"say hello"
        ));
    }

    #[test]
    fn signed_token_rejects_tampered_request_body() {
        let token = signed_token(SECRET, AuthAction::Command, b"say hello").unwrap();

        assert!(!verify_signed_token(
            SECRET,
            AuthAction::Command,
            &token,
            b"say goodbye"
        ));
    }

    #[test]
    fn signed_token_rejects_stale_timestamp() {
        let mut token = signed_token(SECRET, AuthAction::Launch, b"").unwrap();
        token.issued_at_unix_ms = 0;

        assert!(!verify_signed_token(
            SECRET,
            AuthAction::Launch,
            &token,
            b""
        ));
    }
}
