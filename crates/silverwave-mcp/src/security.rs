//! Filesystem Sandboxing (`PathSandboxGuard`), SSRF Network Guard, Token-Bucket Rate Limiter
//! (`RateLimiterPolicy`), and Zero-Allocation Audio Command Queue (`07-SPEC` §5.1–5.3 & §6.4).

#![forbid(unsafe_code)]

use crate::protocol::{ERR_RATE_LIMIT_EXCEEDED, ERR_SANDBOX_VIOLATION, McpProtocolError};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Rate limit category per MCP tool (`07-SPEC` §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolRateCategory {
    RealtimeControl,
    PatternEvaluation,
    HeavyBackgroundIo,
    ReadOnlyQuery,
}

/// Filesystem sandbox and SSRF guard trait (`07-SPEC` §5.3).
pub trait PathSandboxGuard: Send + Sync {
    fn validate_read_path(&self, raw_path: &Path) -> Result<PathBuf, McpProtocolError>;
    fn validate_export_path(&self, raw_path: &Path) -> Result<PathBuf, McpProtocolError>;
    fn validate_remote_url(&self, raw_url: &str) -> Result<String, McpProtocolError>;
}

/// Default enforcement implementation of `PathSandboxGuard` (`07-SPEC` §5.2).
#[derive(Debug, Clone)]
pub struct DefaultSandboxGuard {
    pub allowed_roots: Vec<PathBuf>,
    pub allow_external_network: bool,
}

impl Default for DefaultSandboxGuard {
    fn default() -> Self {
        Self {
            allowed_roots: vec![PathBuf::from("/tmp/silverwave_sandbox")],
            allow_external_network: true,
        }
    }
}

impl DefaultSandboxGuard {
    /// Creates a new `DefaultSandboxGuard`.
    pub fn new(allowed_roots: Vec<PathBuf>, allow_external_network: bool) -> Self {
        Self {
            allowed_roots,
            allow_external_network,
        }
    }

    fn verify_clean_components(raw_path: &Path) -> Result<(), McpProtocolError> {
        let s = raw_path.to_string_lossy();
        if s.contains('\0') || s.starts_with(r"\\.\") || s.starts_with(r"\\?\") {
            return Err(McpProtocolError::new(
                ERR_SANDBOX_VIOLATION,
                "Path contains forbidden NUL byte or device namespace",
            ));
        }
        for comp in raw_path.components() {
            if matches!(comp, Component::ParentDir) {
                return Err(McpProtocolError::new(
                    ERR_SANDBOX_VIOLATION,
                    "Parent directory traversal ('..') is forbidden",
                ));
            }
        }
        Ok(())
    }

    fn check_under_roots(&self, candidate: &Path) -> Result<PathBuf, McpProtocolError> {
        for root in &self.allowed_roots {
            if candidate.starts_with(root) {
                return Ok(candidate.to_path_buf());
            }
        }
        Err(McpProtocolError::new(
            ERR_SANDBOX_VIOLATION,
            format!(
                "Path '{}' is outside configured sandbox roots",
                candidate.display()
            ),
        ))
    }
}

impl PathSandboxGuard for DefaultSandboxGuard {
    fn validate_read_path(&self, raw_path: &Path) -> Result<PathBuf, McpProtocolError> {
        Self::verify_clean_components(raw_path)?;
        self.check_under_roots(raw_path)
    }

    fn validate_export_path(&self, raw_path: &Path) -> Result<PathBuf, McpProtocolError> {
        Self::verify_clean_components(raw_path)?;
        let checked = self.check_under_roots(raw_path)?;
        let file_name = checked.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.starts_with('.') {
            return Err(McpProtocolError::new(
                ERR_SANDBOX_VIOLATION,
                "Refusing to export to hidden dotfile",
            ));
        }
        let ext = checked
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "wav" | "flac" | "mp4" | "webm") {
            return Err(McpProtocolError::new(
                ERR_SANDBOX_VIOLATION,
                "Export path must end in .wav, .flac, .mp4, or .webm",
            ));
        }
        Ok(checked)
    }

    fn validate_remote_url(&self, raw_url: &str) -> Result<String, McpProtocolError> {
        if !self.allow_external_network {
            return Err(McpProtocolError::new(
                ERR_SANDBOX_VIOLATION,
                "External network fetches are disabled",
            ));
        }
        if !raw_url.starts_with("https://") {
            return Err(McpProtocolError::new(
                ERR_SANDBOX_VIOLATION,
                "Only HTTPS URLs are permitted for remote sample manifests",
            ));
        }
        let host_part = raw_url
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        // Bracketed IPv6 (`[::1]`/`[::1]:443`) must be unwrapped before any `:` split.
        let host_only = host_part
            .strip_prefix('[')
            .and_then(|h| h.split(']').next())
            .unwrap_or_else(|| host_part.split(':').next().unwrap_or(""));
        // IPv4-mapped IPv6 (`::ffff:127.0.0.1`) is re-checked against the IPv4 rules.
        let host_only = host_only.strip_prefix("::ffff:").unwrap_or(host_only);
        if host_only == "localhost"
            || host_only == "127.0.0.1"
            || host_only == "::1"
            || host_only.starts_with("10.")
            || host_only.starts_with("192.168.")
            || host_only.starts_with("169.254.")
            || host_only.starts_with("172.16.")
        {
            return Err(McpProtocolError::new(
                ERR_SANDBOX_VIOLATION,
                "SSRF guard blocked loopback or RFC 1918 private network address",
            ));
        }
        Ok(raw_url.to_string())
    }
}

/// Token-bucket rate limiter policy trait (`07-SPEC` §5.3).
pub trait RateLimiterPolicy: Send + Sync {
    fn check_and_consume(
        &mut self,
        client_id: &str,
        category: ToolRateCategory,
    ) -> Result<(), McpProtocolError>;
}

/// Token-bucket rate limiter enforcing per-client burst quotas (`07-SPEC` §5.3 & §6.4).
#[derive(Debug, Clone)]
pub struct TokenBucketRateLimiter {
    pub capacity: u32,
    pub refill_per_call: u32,
    buckets: BTreeMap<(String, ToolRateCategory), u32>,
}

impl Default for TokenBucketRateLimiter {
    fn default() -> Self {
        Self::new(240, 120)
    }
}

impl TokenBucketRateLimiter {
    /// Creates a new `TokenBucketRateLimiter` with `capacity` burst tokens.
    pub fn new(capacity: u32, refill_per_call: u32) -> Self {
        Self {
            capacity: capacity.max(1),
            refill_per_call,
            buckets: BTreeMap::new(),
        }
    }

    /// Replenishes `tokens` for `(client_id, category)`.
    pub fn replenish(&mut self, client_id: &str, category: ToolRateCategory, tokens: u32) {
        let entry = self
            .buckets
            .entry((client_id.to_string(), category))
            .or_insert(self.capacity);
        *entry = (*entry + tokens).min(self.capacity);
    }
}

impl RateLimiterPolicy for TokenBucketRateLimiter {
    fn check_and_consume(
        &mut self,
        client_id: &str,
        category: ToolRateCategory,
    ) -> Result<(), McpProtocolError> {
        let entry = self
            .buckets
            .entry((client_id.to_string(), category))
            .or_insert(self.capacity);
        if *entry == 0 {
            return Err(McpProtocolError::new(
                ERR_RATE_LIMIT_EXCEEDED,
                "Token-bucket rate limit exceeded for client",
            ));
        }
        *entry -= 1;
        Ok(())
    }
}

/// Maximum number of commands drained per real-time audio callback block (`07-SPEC` §5.1).
pub const MAX_COMMANDS_PER_AUDIO_BLOCK: usize = 64;

/// Fixed-size value struct sent over the pre-allocated ring buffer to the audio thread (`07-SPEC` §5.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RtAudioCommand {
    SetCps(f64),
    UpdateSlider { index: u16, value: f64 },
    ConfigureOrbitGain { orbit: u8, gain_db: f64 },
    TriggerPanic,
}

/// Pre-allocated fixed-capacity command queue verifying zero heap allocations on the audio callback (`07-SPEC` §5.1 & §6.4).
#[derive(Debug, Clone)]
pub struct PreallocatedAudioCommandRing {
    buffer: [Option<RtAudioCommand>; MAX_COMMANDS_PER_AUDIO_BLOCK],
    head: usize,
    tail: usize,
    len: usize,
}

impl Default for PreallocatedAudioCommandRing {
    fn default() -> Self {
        Self::new()
    }
}

impl PreallocatedAudioCommandRing {
    /// Creates an empty fixed-capacity command ring (`64` slots).
    pub const fn new() -> Self {
        Self {
            buffer: [None; MAX_COMMANDS_PER_AUDIO_BLOCK],
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    /// Pushes an `RtAudioCommand` without heap allocation.
    pub fn push(&mut self, cmd: RtAudioCommand) -> bool {
        if self.len >= MAX_COMMANDS_PER_AUDIO_BLOCK {
            return false;
        }
        self.buffer[self.tail] = Some(cmd);
        self.tail = (self.tail + 1) % MAX_COMMANDS_PER_AUDIO_BLOCK;
        self.len += 1;
        true
    }

    /// Drains up to `MAX_COMMANDS_PER_AUDIO_BLOCK` commands on the audio thread with zero heap allocations.
    pub fn drain_audio_block<F: FnMut(RtAudioCommand)>(&mut self, mut f: F) -> usize {
        let mut drained = 0;
        while self.len > 0 && drained < MAX_COMMANDS_PER_AUDIO_BLOCK {
            if let Some(cmd) = self.buffer[self.head].take() {
                f(cmd);
            }
            self.head = (self.head + 1) % MAX_COMMANDS_PER_AUDIO_BLOCK;
            self.len -= 1;
            drained += 1;
        }
        drained
    }
}
