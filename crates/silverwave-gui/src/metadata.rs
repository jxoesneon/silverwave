//! Sketch Metadata Parser (`@title`, `@by`, `@genre`, `@license`, `@details` — `05-SPEC` §4.5, `UI-05`)
//! and URL-Safe Base64 `ShareCodec` (`#code=<payload>` — `05-SPEC` §4.6, `UI-06`).

#![forbid(unsafe_code)]

use crate::span_map::ByteSpan;

/// Maximum decompressed share URL payload size (`1 MiB` DoS guard — `05-SPEC` §4.6, `UI-06`).
pub const MAX_SHARE_PAYLOAD_BYTES: usize = 1_048_576;

/// Unified error type for `silverwave-gui`, `silverwave-wasm`, and `silverwave-lsp` (`05-SPEC` §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplError {
    ParseError {
        message: String,
        span: Option<ByteSpan>,
    },
    EvalError {
        message: String,
        span: Option<ByteSpan>,
    },
    AudioBackendError {
        message: String,
    },
    ExportError {
        message: String,
    },
    ProtocolError {
        code: i32,
        message: String,
    },
    ShareDecodeError {
        message: String,
    },
}

/// Extracted document metadata header (`05-SPEC` §4.5, `UI-05`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocMetadata {
    pub title: Option<String>,
    pub by: Option<String>,
    pub genre: Option<String>,
    pub license: Option<String>,
    pub details: Option<String>,
    pub extra: Vec<(String, String)>,
}

/// Trait for extracting `@title`, `@by`, `@genre`, `@license`, `@details` from comments (`05-SPEC` §4.5).
pub trait MetadataParser {
    fn parse_metadata(source: &str) -> DocMetadata;
}

/// Concrete `MetadataParser` supporting `// @key val` and `/* @key val */` across LF and CRLF (`UI-05`).
pub struct SilverWaveMetadataParser;

impl MetadataParser for SilverWaveMetadataParser {
    fn parse_metadata(source: &str) -> DocMetadata {
        let mut meta = DocMetadata::default();
        for raw_line in source.lines() {
            let trimmed = raw_line.trim();
            let cleaned = trimmed
                .strip_prefix("//")
                .or_else(|| trimmed.strip_prefix("/*"))
                .or_else(|| trimmed.strip_prefix('*'))
                .map(|s| s.trim_end_matches("*/").trim())
                .unwrap_or("");
            if let Some(rest) = cleaned.strip_prefix('@') {
                let mut parts = rest.splitn(2, char::is_whitespace);
                let key = parts.next().unwrap_or("").trim().to_ascii_lowercase();
                let val = parts.next().unwrap_or("").trim().to_string();
                if val.is_empty() {
                    continue;
                }
                match key.as_str() {
                    "title" => meta.title = Some(val),
                    "by" | "author" => meta.by = Some(val),
                    "genre" => meta.genre = Some(val),
                    "license" => meta.license = Some(val),
                    "details" | "description" => meta.details = Some(val),
                    other if !other.is_empty() => meta.extra.push((other.to_string(), val)),
                    _ => {}
                }
            }
        }
        meta
    }
}

/// Trait for deterministic `#code=<base64url>` URL state sharing (`05-SPEC` §4.6, `UI-06`).
pub trait ShareCodec {
    fn encode_url_fragment(source: &str) -> String;
    fn decode_url_fragment(fragment: &str) -> Result<String, ReplError>;
}

/// URL-safe Base64 (`RFC 4648 §5`, no padding) `ShareCodec` with `1 MiB` zip-bomb / size guard (`UI-06`).
pub struct SilverWaveShareCodec;

const B64_URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(B64_URL_ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_URL_ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(B64_URL_ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push(B64_URL_ALPHABET[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(B64_URL_ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_URL_ALPHABET[((n >> 12) & 0x3F) as usize] as char);
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(B64_URL_ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_URL_ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(B64_URL_ALPHABET[((n >> 6) & 0x3F) as usize] as char);
    }
    out
}

fn b64_val(b: u8) -> Option<u32> {
    match b {
        b'A'..=b'Z' => Some((b - b'A') as u32),
        b'a'..=b'z' => Some((b - b'a' + 26) as u32),
        b'0'..=b'9' => Some((b - b'0' + 52) as u32),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn base64url_decode(encoded: &str) -> Result<Vec<u8>, ReplError> {
    let clean = encoded.trim_end_matches('=');
    if clean.len() % 4 == 1 {
        return Err(ReplError::ShareDecodeError {
            message: "invalid base64url length".to_string(),
        });
    }
    let bytes = clean.as_bytes();
    let mut out = Vec::with_capacity((bytes.len() * 3) / 4);
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let a = b64_val(bytes[i]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let b = b64_val(bytes[i + 1]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let c = b64_val(bytes[i + 2]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let d = b64_val(bytes[i + 3]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let n = (a << 18) | (b << 12) | (c << 6) | d;
        out.push(((n >> 16) & 0xFF) as u8);
        out.push(((n >> 8) & 0xFF) as u8);
        out.push((n & 0xFF) as u8);
        if out.len() > MAX_SHARE_PAYLOAD_BYTES {
            return Err(ReplError::ShareDecodeError {
                message: "share payload exceeds 1 MiB limit".to_string(),
            });
        }
        i += 4;
    }
    let rem = bytes.len() - i;
    if rem == 2 {
        let a = b64_val(bytes[i]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let b = b64_val(bytes[i + 1]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let n = (a << 18) | (b << 12);
        out.push(((n >> 16) & 0xFF) as u8);
    } else if rem == 3 {
        let a = b64_val(bytes[i]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let b = b64_val(bytes[i + 1]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let c = b64_val(bytes[i + 2]).ok_or_else(|| ReplError::ShareDecodeError {
            message: "invalid base64url character".to_string(),
        })?;
        let n = (a << 18) | (b << 12) | (c << 6);
        out.push(((n >> 16) & 0xFF) as u8);
        out.push(((n >> 8) & 0xFF) as u8);
    }
    if out.len() > MAX_SHARE_PAYLOAD_BYTES {
        return Err(ReplError::ShareDecodeError {
            message: "share payload exceeds 1 MiB limit".to_string(),
        });
    }
    Ok(out)
}

impl ShareCodec for SilverWaveShareCodec {
    fn encode_url_fragment(source: &str) -> String {
        format!("#code={}", base64url_encode(source.as_bytes()))
    }

    fn decode_url_fragment(fragment: &str) -> Result<String, ReplError> {
        let trimmed = fragment.trim();
        let payload = trimmed
            .strip_prefix("#code=")
            .or_else(|| trimmed.strip_prefix("code="))
            .or_else(|| trimmed.strip_prefix("#sw="))
            .unwrap_or(trimmed);
        if payload.is_empty() {
            return Err(ReplError::ShareDecodeError {
                message: "empty share fragment".to_string(),
            });
        }
        let raw = base64url_decode(payload)?;
        String::from_utf8(raw).map_err(|_| ReplError::ShareDecodeError {
            message: "decoded share payload is not valid UTF-8".to_string(),
        })
    }
}
