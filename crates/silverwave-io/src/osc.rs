//! OSC 1.0/1.1 Wire Codec (`OscPacket`, `OscBundle`, `OscMessage`), SuperDirt `/dirt/play`
//! NTP Timetag Encoder (`OSC-01`), `/ctrl` State Receiver (`OSC-02`), Remote Stream Control
//! (`OSC-03`), and Custom `OscShape` Address Interpolation (`OSC-04`).

use crate::types::{ControlValue, DispatchEvent, IoError, OutputSink, StateMap, StreamId};
use silverwave_core::Rational;
use std::net::SocketAddr;
use std::sync::Mutex;

/// NTP epoch offset from Unix epoch (`1900-01-01` to `1970-01-01` = `2_208_988_800` seconds — `Section 3.2`, `OSC-01`).
pub const NTP_UNIX_OFFSET_SECS: u64 = 2_208_988_800;

/// 64-bit OSC NTP timetag (`seconds`, `fractional` — `Section 3.2`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OscTime {
    pub seconds: u32,
    pub fractional: u32,
}

impl OscTime {
    /// Converts Unix epoch seconds `unix_secs` into an `OscTime` NTP timetag (`OSC-01`).
    pub fn from_unix_secs(unix_secs: f64) -> Self {
        let clamped = unix_secs.max(0.0);
        let whole = clamped.floor() as u64;
        let frac = clamped - clamped.floor();
        let ntp_secs = whole.wrapping_add(NTP_UNIX_OFFSET_SECS) as u32;
        let ntp_frac = (frac * ((1u64 << 32) as f64)).round() as u32;
        Self {
            seconds: ntp_secs,
            fractional: ntp_frac,
        }
    }

    /// Converts `self` back to Unix epoch seconds (`OSC-01`).
    pub fn to_unix_secs(self) -> f64 {
        let whole = (self.seconds as u64).saturating_sub(NTP_UNIX_OFFSET_SECS) as f64;
        let frac = (self.fractional as f64) / ((1u64 << 32) as f64);
        whole + frac
    }
}

/// Typed OSC 1.0 argument value (`Section 3.2`).
#[derive(Clone, Debug, PartialEq)]
pub enum OscType {
    Int(i32),
    Float(f32),
    String(String),
    Blob(Vec<u8>),
}

/// OSC Message (`addr` + `args` — `Section 3.1`).
#[derive(Clone, Debug, PartialEq)]
pub struct OscMessage {
    pub addr: String,
    pub args: Vec<OscType>,
}

/// OSC Bundle (`timetag` + `content` — `Section 3.1`).
#[derive(Clone, Debug, PartialEq)]
pub struct OscBundle {
    pub timetag: OscTime,
    pub content: Vec<OscPacket>,
}

/// Top-level OSC packet (`Message` or `Bundle` — `Section 3.1`).
#[derive(Clone, Debug, PartialEq)]
pub enum OscPacket {
    Message(OscMessage),
    Bundle(OscBundle),
}

/// Encodes an `OscPacket` into its binary 4-byte aligned OSC 1.0 wire representation.
pub fn encode_osc_packet(packet: &OscPacket) -> Vec<u8> {
    let mut out = Vec::new();
    match packet {
        OscPacket::Message(msg) => {
            write_osc_string(&mut out, &msg.addr);
            let mut tags = String::with_capacity(msg.args.len() + 1);
            tags.push(',');
            for a in &msg.args {
                match a {
                    OscType::Int(_) => tags.push('i'),
                    OscType::Float(_) => tags.push('f'),
                    OscType::String(_) => tags.push('s'),
                    OscType::Blob(_) => tags.push('b'),
                }
            }
            write_osc_string(&mut out, &tags);
            for a in &msg.args {
                match a {
                    OscType::Int(i) => out.extend_from_slice(&i.to_be_bytes()),
                    OscType::Float(f) => out.extend_from_slice(&f.to_be_bytes()),
                    OscType::String(s) => write_osc_string(&mut out, s),
                    OscType::Blob(b) => {
                        out.extend_from_slice(&(b.len() as i32).to_be_bytes());
                        out.extend_from_slice(b);
                        while out.len() % 4 != 0 {
                            out.push(0);
                        }
                    }
                }
            }
        }
        OscPacket::Bundle(bundle) => {
            write_osc_string(&mut out, "#bundle");
            out.extend_from_slice(&bundle.timetag.seconds.to_be_bytes());
            out.extend_from_slice(&bundle.timetag.fractional.to_be_bytes());
            for elem in &bundle.content {
                let elem_bytes = encode_osc_packet(elem);
                out.extend_from_slice(&(elem_bytes.len() as i32).to_be_bytes());
                out.extend_from_slice(&elem_bytes);
            }
        }
    }
    out
}

fn write_osc_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// Scheduling mode for outgoing OSC packets (`Section 3.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OscScheduleMode {
    Live,
    PreBundleStamp,
    PreMessageStamp,
}

/// Target endpoint configuration for `OscEngine` (`Section 3.1`).
#[derive(Clone, Debug)]
pub struct OscTarget {
    pub name: String,
    pub remote_addr: SocketAddr,
    pub local_bind_addr: Option<SocketAddr>,
    pub latency_secs: f64,
    pub schedule: OscScheduleMode,
    pub handshake: bool,
    pub bus_port: Option<u16>,
}

impl OscTarget {
    /// Default SuperDirt target (`127.0.0.1:57120`, `latency = 0.2`, `PreBundleStamp`).
    pub fn superdirt_default() -> Self {
        Self {
            name: "superdirt".to_string(),
            remote_addr: "127.0.0.1:57120".parse().unwrap(),
            local_bind_addr: None,
            latency_secs: 0.2,
            schedule: OscScheduleMode::PreBundleStamp,
            handshake: true,
            bus_port: None,
        }
    }
}

/// Default value specification for a positional OSC argument (`Section 3.4`).
#[derive(Clone, Debug, PartialEq)]
pub enum OscArgDefault {
    Required,
    Str(String),
    Float(f32),
    Int(i32),
    Bool(bool),
    Blob(Vec<u8>),
}

/// Positional OSC argument descriptor (`Section 3.4`).
#[derive(Clone, Debug, PartialEq)]
pub struct OscArgSpec {
    pub param_name: String,
    pub default: OscArgDefault,
}

/// Payload formatting mode for `OscShape` (`Section 3.4`).
#[derive(Clone, Debug, PartialEq)]
pub enum OscPayloadMode {
    ArgList(Vec<OscArgSpec>),
    NamedPairs { required_keys: Vec<String> },
}

/// OSC message address & payload template (`Section 3.4`, `OSC-04`).
#[derive(Clone, Debug, PartialEq)]
pub struct OscShape {
    pub address_pattern: String,
    pub payload_mode: OscPayloadMode,
}

impl OscShape {
    /// Canonical SuperDirt `/dirt/play` named-pair shape (`Section 3.2`).
    pub fn superdirt() -> Self {
        Self {
            address_pattern: "/dirt/play".to_string(),
            payload_mode: OscPayloadMode::NamedPairs {
                required_keys: Vec::new(),
            },
        }
    }
}

/// Stream control command decoded from incoming OSC (`/mute`, `/solo`, `/hush` — `Section 3.3`, `OSC-03`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteStreamCommand {
    Mute(StreamId),
    Unmute(StreamId),
    MuteAll,
    UnmuteAll,
    Solo(StreamId),
    Unsolo(StreamId),
    Hush,
}

/// OSC Engine trait (`Section 3.4`).
pub trait OscEngine: OutputSink {
    fn add_target(&mut self, target: OscTarget, shapes: Vec<OscShape>) -> Result<(), IoError>;
    fn encode_event(
        target: &OscTarget,
        shape: &OscShape,
        event: &DispatchEvent,
    ) -> Result<Option<OscPacket>, IoError>;
}

/// Concrete `OscEngine` supporting `/dirt/play`, custom `OscShape` interpolation, `/ctrl` state updates, and `/mute`/`/solo`/`/hush` (`OSC-01`..`OSC-04`).
#[derive(Default)]
pub struct SilverWaveOscEngine {
    pub targets: Vec<(OscTarget, Vec<OscShape>)>,
    pub sent_packets: Mutex<Vec<OscPacket>>,
}

impl SilverWaveOscEngine {
    /// Creates a new `SilverWaveOscEngine`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes an incoming `/ctrl` or remote stream control `OscMessage` (`OSC-02`, `OSC-03`).
    pub fn handle_incoming_message(
        msg: &OscMessage,
        state_map: &mut StateMap,
        stream_cmds: &mut Vec<RemoteStreamCommand>,
    ) {
        match msg.addr.as_str() {
            "/ctrl" if msg.args.len() >= 2 => {
                let key = match &msg.args[0] {
                    OscType::String(s) => s.clone(),
                    OscType::Int(i) => i.to_string(),
                    _ => return,
                };
                let val = match &msg.args[1] {
                    OscType::Float(f) => ControlValue::Float(*f as f64),
                    OscType::Int(i) => ControlValue::Int(*i as i64),
                    OscType::String(s) => ControlValue::Str(s.clone()),
                    OscType::Blob(b) => ControlValue::Bytes(b.clone()),
                };
                state_map.insert(key, val);
            }
            "/mute" if !msg.args.is_empty() => {
                if let Some(id) = osc_arg_to_stream_id(&msg.args[0]) {
                    stream_cmds.push(RemoteStreamCommand::Mute(id));
                }
            }
            "/unmute" if !msg.args.is_empty() => {
                if let Some(id) = osc_arg_to_stream_id(&msg.args[0]) {
                    stream_cmds.push(RemoteStreamCommand::Unmute(id));
                }
            }
            "/muteAll" => stream_cmds.push(RemoteStreamCommand::MuteAll),
            "/unmuteAll" => stream_cmds.push(RemoteStreamCommand::UnmuteAll),
            "/solo" if !msg.args.is_empty() => {
                if let Some(id) = osc_arg_to_stream_id(&msg.args[0]) {
                    stream_cmds.push(RemoteStreamCommand::Solo(id));
                }
            }
            "/unsolo" if !msg.args.is_empty() => {
                if let Some(id) = osc_arg_to_stream_id(&msg.args[0]) {
                    stream_cmds.push(RemoteStreamCommand::Unsolo(id));
                }
            }
            "/hush" => stream_cmds.push(RemoteStreamCommand::Hush),
            _ => {}
        }
    }
}

fn osc_arg_to_stream_id(arg: &OscType) -> Option<StreamId> {
    match arg {
        OscType::String(s) => Some(StreamId(s.clone())),
        OscType::Int(i) => Some(StreamId(i.to_string())),
        _ => None,
    }
}

fn interpolate_osc_address(template: &str, event: &DispatchEvent) -> Option<String> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut key = String::new();
            for next_c in chars.by_ref() {
                if next_c == '}' {
                    break;
                }
                key.push(next_c);
            }
            let val = event.value.get(&key)?;
            match val {
                ControlValue::Str(s) => out.push_str(s),
                ControlValue::Int(i) => out.push_str(&i.to_string()),
                ControlValue::Float(f) => out.push_str(&f.to_string()),
                _ => return None,
            }
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn control_value_to_osc(val: &ControlValue) -> OscType {
    match val {
        ControlValue::Float(f) => OscType::Float(*f as f32),
        ControlValue::Int(i) => OscType::Int(*i as i32),
        ControlValue::Str(s) => OscType::String(s.clone()),
        ControlValue::Bool(b) => OscType::Int(if *b { 1 } else { 0 }),
        ControlValue::Bytes(b) => OscType::Blob(b.clone()),
        ControlValue::Note(n) => OscType::Float(n.midi_note as f32),
    }
}

impl OutputSink for SilverWaveOscEngine {
    fn name(&self) -> &str {
        "osc"
    }

    fn send_batch(&self, events: &[DispatchEvent]) -> Result<(), IoError> {
        let mut guard = self
            .sent_packets
            .lock()
            .map_err(|_| IoError::Osc("osc packet log poisoned".to_string()))?;
        for (target, shapes) in &self.targets {
            for shape in shapes {
                for ev in events {
                    if let Some(pkt) = Self::encode_event(target, shape, ev)? {
                        guard.push(pkt);
                    }
                }
            }
        }
        Ok(())
    }

    fn panic(&self) -> Result<(), IoError> {
        if let Ok(mut guard) = self.sent_packets.lock() {
            guard.push(OscPacket::Message(OscMessage {
                addr: "/hush".to_string(),
                args: Vec::new(),
            }));
        }
        Ok(())
    }

    fn close(&self) -> Result<(), IoError> {
        Ok(())
    }
}

impl OscEngine for SilverWaveOscEngine {
    fn add_target(&mut self, target: OscTarget, shapes: Vec<OscShape>) -> Result<(), IoError> {
        self.targets.push((target, shapes));
        Ok(())
    }

    fn encode_event(
        target: &OscTarget,
        shape: &OscShape,
        event: &DispatchEvent,
    ) -> Result<Option<OscPacket>, IoError> {
        let Some(addr) = interpolate_osc_address(&shape.address_pattern, event) else {
            return Ok(None);
        };
        let mut args = Vec::new();

        if target.schedule == OscScheduleMode::PreMessageStamp {
            let stamped = (event.onset_secs + target.latency_secs).max(0.0);
            let sec = stamped.floor() as i32;
            let usec = ((stamped - stamped.floor()) * 1_000_000.0).round() as i32;
            args.push(OscType::Int(sec));
            args.push(OscType::Int(usec));
        }

        match &shape.payload_mode {
            OscPayloadMode::ArgList(specs) => {
                for spec in specs {
                    if let Some(v) = event.value.get(&spec.param_name) {
                        args.push(control_value_to_osc(v));
                    } else {
                        match &spec.default {
                            OscArgDefault::Required => return Ok(None),
                            OscArgDefault::Str(s) => args.push(OscType::String(s.clone())),
                            OscArgDefault::Float(f) => args.push(OscType::Float(*f)),
                            OscArgDefault::Int(i) => args.push(OscType::Int(*i)),
                            OscArgDefault::Bool(b) => {
                                args.push(OscType::Int(if *b { 1 } else { 0 }))
                            }
                            OscArgDefault::Blob(b) => args.push(OscType::Blob(b.clone())),
                        }
                    }
                }
            }
            OscPayloadMode::NamedPairs { required_keys } => {
                for rk in required_keys {
                    if !event.value.contains_key(rk) {
                        return Ok(None);
                    }
                }
                // Mandatory SuperDirt core fields (`cps`, `cycle`, `delta`, `orbit` — `Section 3.2`, `OSC-01`)
                let cycle_start: Rational = event.whole.unwrap_or(event.part).start;
                let orbit_val = event
                    .value
                    .get("orbit")
                    .and_then(ControlValue::as_i64)
                    .unwrap_or(0) as i32;
                args.push(OscType::String("cps".to_string()));
                args.push(OscType::Float(event.cps as f32));
                args.push(OscType::String("cycle".to_string()));
                args.push(OscType::Float(cycle_start.to_f64() as f32));
                args.push(OscType::String("delta".to_string()));
                args.push(OscType::Float(event.duration_secs as f32));
                args.push(OscType::String("orbit".to_string()));
                args.push(OscType::Int(orbit_val));

                for (k, v) in &event.value {
                    if matches!(k.as_str(), "cps" | "cycle" | "delta" | "orbit") {
                        continue;
                    }
                    args.push(OscType::String(k.clone()));
                    args.push(control_value_to_osc(v));
                }
            }
        }

        let msg = OscMessage { addr, args };
        match target.schedule {
            OscScheduleMode::PreBundleStamp => {
                let timetag = OscTime::from_unix_secs(event.onset_secs + target.latency_secs);
                Ok(Some(OscPacket::Bundle(OscBundle {
                    timetag,
                    content: vec![OscPacket::Message(msg)],
                })))
            }
            OscScheduleMode::Live | OscScheduleMode::PreMessageStamp => {
                Ok(Some(OscPacket::Message(msg)))
            }
        }
    }
}
