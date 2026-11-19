//! Authenticated encryption for saves and graph nodes.
//!
//! A project key (any text) is hashed to a 32-byte key. Each seal gets a fresh
//! nonce. Opening a changed seal fails, so a tampered save is refused.

use std::sync::atomic::{AtomicU64, Ordering};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use sha2::{Digest, Sha256};

const HEADER: &str = "seal enc 1";

static NONCE_SEQ: AtomicU64 = AtomicU64::new(1);

/// Whether `text` is a sealed envelope this module writes.
pub fn is_envelope(text: &str) -> bool {
    text.lines().next().is_some_and(|line| line.trim() == HEADER)
}

/// Seal `plain` with `secret`. An empty secret is refused.
pub fn seal(plain: &str, secret: &str) -> Result<String, String> {
    seal_with_nonce(plain, secret, &fresh_nonce())
}

/// Seal with a chosen nonce. Tests use this so a fixture can be rebuilt.
pub fn seal_with_nonce(plain: &str, secret: &str, nonce: &[u8; 12]) -> Result<String, String> {
    let cipher = cipher(secret)?;
    let nonce = Nonce::from_slice(nonce);
    let ct =
        cipher.encrypt(nonce, plain.as_bytes()).map_err(|_| "encryption failed".to_string())?;
    Ok(format!("{HEADER}\nnonce {}\ndata {}\n", hex(nonce.as_slice()), encode_b64(&ct)))
}

/// Open an envelope. A wrong key and a changed body both fail.
pub fn open(text: &str, secret: &str) -> Result<String, String> {
    if !is_envelope(text) {
        return Err("this is not encrypted data".into());
    }
    let mut nonce = None;
    let mut data = None;
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("nonce ") {
            nonce = Some(parse_hex(rest.trim())?);
        } else if let Some(rest) = line.strip_prefix("data ") {
            data = Some(decode_b64(rest.trim())?);
        } else {
            return Err(format!("the encrypted data has an unknown line {line:?}"));
        }
    }
    let nonce = nonce.ok_or("the encrypted data has no nonce")?;
    if nonce.len() != 12 {
        return Err("the encrypted data has a damaged nonce".into());
    }
    let data = data.ok_or("the encrypted data has no body")?;
    let cipher = cipher(secret)?;
    let nonce = Nonce::from_slice(&nonce);
    let plain = cipher
        .decrypt(nonce, data.as_ref())
        .map_err(|_| "this data was changed or the project key is wrong".to_string())?;
    String::from_utf8(plain).map_err(|_| "the decrypted data is not text".into())
}

fn cipher(secret: &str) -> Result<ChaCha20Poly1305, String> {
    if secret.is_empty() {
        return Err("set a project key before encrypting".into());
    }
    let dig = Sha256::digest(secret.as_bytes());
    let key = Key::from_slice(&dig);
    Ok(ChaCha20Poly1305::new(key))
}

fn fresh_nonce() -> [u8; 12] {
    let n = NONCE_SEQ.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut out = [0u8; 12];
    out[..8].copy_from_slice(&t.to_le_bytes());
    out[8..].copy_from_slice(&(n as u32).to_le_bytes());
    out
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn parse_hex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the encrypted data has a damaged nonce".into());
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16).unwrap();
        let lo = (bytes[i + 1] as char).to_digit(16).unwrap();
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Ok(out)
}

/// Standard base64, with padding.
pub fn encode_b64(data: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(A[((n >> 6) & 63) as usize] as char);
        out.push(A[(n & 63) as usize] as char);
        i += 3;
    }
    if i < data.len() {
        let left = data.len() - i;
        let b0 = data[i] as u32;
        let b1 = if left > 1 { data[i + 1] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        if left > 1 {
            out.push(A[((n >> 6) & 63) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}

/// Decode standard base64. Padding is optional. Whitespace is ignored.
pub fn decode_b64(text: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u8, String> {
        Ok(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("the encrypted data has a damaged body".into()),
        })
    }
    let bytes: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return Err("the encrypted data has a damaged body".into());
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let pad = bytes[i..i + 4].iter().filter(|b| **b == b'=').count();
        if pad > 2 || bytes[i..i + 4 - pad].contains(&b'=') {
            return Err("the encrypted data has a damaged body".into());
        }
        let take = 4 - pad;
        let mut n = 0u32;
        for k in 0..take {
            n = (n << 6) | val(bytes[i + k])? as u32;
        }
        n <<= 6 * pad;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad == 0 {
            out.push(n as u8);
        }
        i += 4;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_a_changed_body_is_refused() {
        let sealed = seal("hello save", "project key").unwrap();
        assert!(is_envelope(&sealed));
        assert!(!sealed.contains("hello save"));
        assert_eq!(open(&sealed, "project key").unwrap(), "hello save");
        assert!(open(&sealed, "other key").unwrap_err().contains("changed or the project key"));
        let mut damaged = sealed.clone();
        let at = damaged.rfind(|c: char| c.is_ascii_alphanumeric()).unwrap();
        damaged.replace_range(at..at + 1, if damaged.as_bytes()[at] == b'A' { "B" } else { "A" });
        assert!(open(&damaged, "project key").is_err());
    }

    #[test]
    fn a_damaged_envelope_is_refused() {
        assert!(open("seal enc 1\ngarbage\n", "k").unwrap_err().contains("unknown line"));
        assert!(open("seal enc 1\nnonce zz\ndata AAAA\n", "k").is_err());
        assert!(seal("", "").is_err());
        assert_eq!(decode_b64("aGVsbG8=").unwrap(), b"hello");
        assert!(decode_b64("!!!!").is_err());
    }
}
