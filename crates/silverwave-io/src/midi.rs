//! MIDI 1.0/2.0 & MPE Engine (`@strudel/midi` Parity — `04-SPEC` Section 4, `MID-01`..`MID-08`),
//! Sample-Accurate Intra-Timestamp Priority State Machine (`PP-12`), External 24 PPQN Clock PLL (`PP-12`),
//! Active Note-Off Safety Tracker (`MID-08`), and Steam Deck / Gamepad / `DeviceMotion` Bridge (`CK-10`).

use crate::types::{ControlMap, ControlValue, DispatchEvent, IoError, OutputSink, StreamId};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Metadata for an enumerated MIDI port (`Section 4.1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
    pub is_virtual: bool,
}

/// Handle to an opened MIDI port (`Section 4.1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MidiPortHandle(pub String);

/// Cross-platform MIDI backend trait (`Section 4.1`).
pub trait MidiBackend: Send + Sync {
    fn list_outputs(&self) -> Result<Vec<MidiPortInfo>, IoError>;
    fn list_inputs(&self) -> Result<Vec<MidiPortInfo>, IoError>;
    fn open_output(&mut self, port_selector: Option<&str>) -> Result<MidiPortHandle, IoError>;
}

/// Deterministic intra-timestamp MIDI message priority (`04-SPEC` Section 8.3, `PP-12`):
/// `RetriggerNoteOff (0) -> BankSelectMsbCc0 (1) -> BankSelectLsbCc32 (2) -> ProgramChange (3) -> ControlChangeAndPitchBend (4) -> NoteOn (5) -> ScheduledNoteOff (6)`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MidiEventOrderingPriority {
    RetriggerNoteOff = 0,
    BankSelectMsbCc0 = 1,
    BankSelectLsbCc32 = 2,
    ProgramChange = 3,
    ControlChangeAndPitchBend = 4,
    NoteOn = 5,
    ScheduledNoteOff = 6,
}

/// Timestamped outbound MIDI wire packet with deterministic `MidiEventOrderingPriority`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledMidiPacket {
    pub time_secs: f64,
    pub priority: MidiEventOrderingPriority,
    pub bytes: Vec<u8>,
}

/// `midimaps` CC scaling curve (`Section 4.3`, `MID-02`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MidiScaleCurve {
    Linear,
    Exponential(f64),
}

/// `midimaps` CC mapping definition (`Section 4.3`, `MID-02`).
#[derive(Clone, Debug, PartialEq)]
pub struct MidiCcMapping {
    pub cc: u8,
    pub min: f64,
    pub max: f64,
    pub curve: MidiScaleCurve,
}

/// Named collection of `MidiCcMapping` entries (`Section 4.3`).
#[derive(Clone, Debug, Default)]
pub struct MidiMap {
    pub name: String,
    pub default_channel: Option<u8>,
    pub controls: BTreeMap<String, MidiCcMapping>,
}

/// Registry trait for `MidiMap`s (`Section 4.3`, `MID-02`).
pub trait MidiMapRegistry: Send + Sync {
    fn register_map(&mut self, map: MidiMap);
    fn get_map(&self, name: &str) -> Option<&MidiMap>;
    fn scale_to_u7(mapping: &MidiCcMapping, value: f64) -> u8;
}

/// In-memory `MidiMapRegistry` implementation (`MID-02`).
#[derive(Default)]
pub struct InMemoryMidiMapRegistry {
    maps: BTreeMap<String, MidiMap>,
}

impl InMemoryMidiMapRegistry {
    /// Creates an empty `InMemoryMidiMapRegistry`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl MidiMapRegistry for InMemoryMidiMapRegistry {
    fn register_map(&mut self, map: MidiMap) {
        self.maps.insert(map.name.clone(), map);
    }

    fn get_map(&self, name: &str) -> Option<&MidiMap> {
        self.maps.get(name)
    }

    fn scale_to_u7(mapping: &MidiCcMapping, value: f64) -> u8 {
        // Normalize `value`: if `value` is in `[0.0, 1.0]` and `min >= 0.0`, treat as normalized `0..1`
        let norm = if (0.0..=1.0).contains(&value) && (mapping.max - mapping.min).abs() > 1.0 {
            value
        } else if (mapping.max - mapping.min).abs() > 1.0e-9 {
            ((value - mapping.min) / (mapping.max - mapping.min)).clamp(0.0, 1.0)
        } else {
            value.clamp(0.0, 1.0)
        };
        let shaped = match mapping.curve {
            MidiScaleCurve::Linear => norm,
            MidiScaleCurve::Exponential(exp) => norm.powf(exp.max(0.01)),
        };
        // If `min == 0.0` and `max == 1.0` (or default 0..127), map `[0, 1]` to `0..=127`;
        // otherwise map into `[min, max]` in 7-bit CC domain (`MID-02`).
        let scaled = if mapping.min >= 0.0 && mapping.max <= 127.0 && mapping.max > 1.0 {
            mapping.min + (mapping.max - mapping.min) * shaped
        } else {
            shaped * 127.0
        };
        scaled.round().clamp(0.0, 127.0) as u8
    }
}

/// Active incoming MIDI note state for `midikeys` (`Section 4.4`, `MID-07`).
#[derive(Clone, Debug, PartialEq)]
pub struct MidiInputNote {
    pub channel: u8,
    pub note: u8,
    pub velocity: f64,
    pub active: bool,
}

/// Incoming MIDI state tracker for `midin` and `midikeys` (`Section 4.4`, `MID-07`).
pub trait MidiInputState: Send + Sync {
    fn get_cc(&self, channel: u8, cc: u8) -> f64;
    fn active_keys(&self, channel: Option<u8>) -> Vec<MidiInputNote>;
    fn ingest_raw_midi(&self, bytes: &[u8]);
}

/// Thread-safe `MidiInputState` implementation (`MID-07`).
#[derive(Default)]
pub struct InMemoryMidiInputState {
    ccs: Mutex<BTreeMap<(u8, u8), f64>>,
    keys: Mutex<BTreeMap<(u8, u8), MidiInputNote>>,
}

impl InMemoryMidiInputState {
    /// Creates a new `InMemoryMidiInputState`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl MidiInputState for InMemoryMidiInputState {
    fn get_cc(&self, channel: u8, cc: u8) -> f64 {
        let ch = channel.saturating_sub(1) & 0x0F;
        self.ccs
            .lock()
            .ok()
            .and_then(|g| g.get(&(ch, cc)).copied())
            .unwrap_or(0.0)
    }

    fn active_keys(&self, channel: Option<u8>) -> Vec<MidiInputNote> {
        let filter_ch = channel.map(|c| c.saturating_sub(1) & 0x0F);
        self.keys
            .lock()
            .map(|g| {
                g.values()
                    .filter(|k| k.active && filter_ch.map(|fc| k.channel == fc + 1).unwrap_or(true))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn ingest_raw_midi(&self, bytes: &[u8]) {
        if bytes.len() < 2 {
            return;
        }
        let status = bytes[0] & 0xF0;
        let ch = bytes[0] & 0x0F;
        match status {
            0xB0 if bytes.len() >= 3 => {
                let cc = bytes[1] & 0x7F;
                let val = (bytes[2] & 0x7F) as f64 / 127.0;
                if let Ok(mut g) = self.ccs.lock() {
                    g.insert((ch, cc), val);
                }
            }
            0x90 if bytes.len() >= 3 => {
                let note = bytes[1] & 0x7F;
                let vel = bytes[2] & 0x7F;
                if let Ok(mut g) = self.keys.lock() {
                    if vel == 0 {
                        g.remove(&(ch, note));
                    } else {
                        g.insert(
                            (ch, note),
                            MidiInputNote {
                                channel: ch + 1,
                                note,
                                velocity: (vel as f64) / 127.0,
                                active: true,
                            },
                        );
                    }
                }
            }
            0x80 if bytes.len() >= 3 => {
                let note = bytes[1] & 0x7F;
                if let Ok(mut g) = self.keys.lock() {
                    g.remove(&(ch, note));
                }
            }
            _ => {}
        }
    }
}

/// Unique key identifying an active MIDI note (`Section 4.5`, `MID-08`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActiveNoteKey {
    pub port_id: String,
    pub channel: u8,
    pub note: u8,
}

/// Active note entry with scheduled `off_time_secs` and owning `stream_id` (`Section 4.5`).
#[derive(Clone, Debug)]
pub struct ActiveNoteEntry {
    pub key: ActiveNoteKey,
    pub off_time_secs: f64,
    pub stream_id: StreamId,
}

/// Trait for tracking active MIDI notes and guaranteeing zero stuck notes (`Section 4.5`, `MID-08`).
pub trait ActiveNoteTracker: Send + Sync {
    fn track_note_on(&self, key: ActiveNoteKey, off_time_secs: f64, stream_id: &StreamId);
    fn drain_due_note_offs(&self, now_secs: f64) -> Vec<ActiveNoteKey>;
    fn drain_stream_note_offs(&self, stream_id: &StreamId) -> Vec<ActiveNoteKey>;
    fn drain_all_note_offs(&self) -> Vec<ActiveNoteKey>;
}

/// Thread-safe `ActiveNoteTracker` implementation (`MID-08`).
#[derive(Default)]
pub struct InMemoryActiveNoteTracker {
    notes: Mutex<BTreeMap<ActiveNoteKey, (f64, StreamId)>>,
}

impl InMemoryActiveNoteTracker {
    /// Creates an empty `InMemoryActiveNoteTracker`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of currently held notes.
    pub fn active_count(&self) -> usize {
        self.notes.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Checks if `key` is currently active (for zero-hack retrigger `NoteOff` injection — `PP-12`).
    pub fn remove_if_active(&self, key: &ActiveNoteKey) -> bool {
        self.notes
            .lock()
            .ok()
            .and_then(|mut g| g.remove(key))
            .is_some()
    }
}

impl ActiveNoteTracker for InMemoryActiveNoteTracker {
    fn track_note_on(&self, key: ActiveNoteKey, off_time_secs: f64, stream_id: &StreamId) {
        if let Ok(mut g) = self.notes.lock() {
            g.insert(key, (off_time_secs, stream_id.clone()));
        }
    }

    fn drain_due_note_offs(&self, now_secs: f64) -> Vec<ActiveNoteKey> {
        let mut due = Vec::new();
        if let Ok(mut g) = self.notes.lock() {
            let keys: Vec<ActiveNoteKey> = g
                .iter()
                .filter(|(_, (t, _))| *t <= now_secs)
                .map(|(k, _)| k.clone())
                .collect();
            for k in keys {
                g.remove(&k);
                due.push(k);
            }
        }
        due
    }

    fn drain_stream_note_offs(&self, stream_id: &StreamId) -> Vec<ActiveNoteKey> {
        let mut out = Vec::new();
        if let Ok(mut g) = self.notes.lock() {
            let keys: Vec<ActiveNoteKey> = g
                .iter()
                .filter(|(_, (_, sid))| sid == stream_id)
                .map(|(k, _)| k.clone())
                .collect();
            for k in keys {
                g.remove(&k);
                out.push(k);
            }
        }
        out
    }

    fn drain_all_note_offs(&self) -> Vec<ActiveNoteKey> {
        if let Ok(mut g) = self.notes.lock() {
            let keys: Vec<ActiveNoteKey> = g.keys().cloned().collect();
            g.clear();
            keys
        } else {
            Vec::new()
        }
    }
}

/// MPE Zone Configuration (`Section 4.6`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpeZoneConfig {
    pub master_channel: u8,
    pub member_channels: u8,
    pub pitch_bend_range_semitones: u8,
}

impl Default for MpeZoneConfig {
    fn default() -> Self {
        Self {
            master_channel: 1,
            member_channels: 15,
            pitch_bend_range_semitones: 48,
        }
    }
}

/// Per-note MPE expression state (`Section 4.6`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MpeNoteExpression {
    pub pitch_bend_semitones: f32,
    pub timbre_cc74: f32,
    pub pressure: f32,
}

/// Round-robin MPE member channel allocator (`Section 4.6`).
#[derive(Clone, Debug)]
pub struct MpeChannelAllocator {
    pub config: MpeZoneConfig,
    next_offset: u8,
}

impl MpeChannelAllocator {
    /// Creates a new `MpeChannelAllocator` for `config`.
    pub const fn new(config: MpeZoneConfig) -> Self {
        Self {
            config,
            next_offset: 0,
        }
    }

    /// Allocates the next MPE member channel (`2..=16` for Lower Zone) and encodes expression bytes.
    pub fn allocate_and_encode(&mut self, expr: &MpeNoteExpression) -> (u8, Vec<[u8; 3]>) {
        let count = self.config.member_channels.clamp(1, 15);
        let ch_1based = if self.config.master_channel == 1 {
            2 + (self.next_offset % count)
        } else {
            16 - 1 - (self.next_offset % count)
        };
        self.next_offset = (self.next_offset + 1) % count;
        let ch_nibble = (ch_1based - 1) & 0x0F;

        let bend_norm = (expr.pitch_bend_semitones
            / (self.config.pitch_bend_range_semitones.max(1) as f32))
            .clamp(-1.0, 1.0);
        let bend_u14 = ((bend_norm + 1.0) * 8191.5).round() as u16;
        let cc74_u7 = (expr.timbre_cc74.clamp(0.0, 1.0) * 127.0).round() as u8;
        let press_u7 = (expr.pressure.clamp(0.0, 1.0) * 127.0).round() as u8;

        let msgs = vec![
            [0xE0 | ch_nibble, (bend_u14 & 0x7F) as u8, ((bend_u14 >> 7) & 0x7F) as u8],
            [0xB0 | ch_nibble, 74, cc74_u7],
            [0xD0 | ch_nibble, press_u7, 0],
        ];
        (ch_1based, msgs)
    }
}

/// 2nd-Order Phase-Locked Loop (PLL) for external 24 PPQN (`0xF8`) MIDI clock slave sync (`< 0.2 ms` jitter — `PP-12`).
#[derive(Clone, Copy, Debug)]
pub struct ExternalMidiClockPll {
    pub estimated_bpm: f64,
    pub last_tick_secs: Option<f64>,
    pub integral_err: f64,
}

impl Default for ExternalMidiClockPll {
    fn default() -> Self {
        Self {
            estimated_bpm: 120.0,
            last_tick_secs: None,
            integral_err: 0.0,
        }
    }
}

impl ExternalMidiClockPll {
    /// Ingests a `0xF8` Timing Clock tick timestamp at `now_secs` and updates `estimated_bpm`.
    pub fn on_clock_tick(&mut self, now_secs: f64) -> f64 {
        if let Some(prev) = self.last_tick_secs {
            let dt = (now_secs - prev).max(1.0e-4);
            let inst_bpm = 60.0 / (dt * 24.0);
            let err = inst_bpm - self.estimated_bpm;
            self.integral_err = (self.integral_err + 0.05 * err).clamp(-20.0, 20.0);
            self.estimated_bpm = (self.estimated_bpm + 0.25 * err + self.integral_err).clamp(20.0, 300.0);
        }
        self.last_tick_secs = Some(now_secs);
        self.estimated_bpm
    }
}

/// Cross-platform Gamepad / HID and `DeviceMotion` bridge (`Section 4.6`, `CK-10`).
pub trait ControllerInputBridge: Send + Sync {
    fn gamepad_axis(&self, pad_id: usize, axis_name: &str) -> f64;
    fn gamepad_button(&self, pad_id: usize, button_name: &str) -> f64;
    fn device_motion(&self, channel: &str) -> f64;
}

/// Concrete Steam Deck / Handheld Gamepad & `DeviceMotion` state bridge (`CK-10`).
#[derive(Default)]
pub struct SteamDeckControllerBridge {
    axes: Mutex<BTreeMap<(usize, String), f64>>,
    buttons: Mutex<BTreeMap<(usize, String), f64>>,
    motion: Mutex<BTreeMap<String, f64>>,
}

impl SteamDeckControllerBridge {
    /// Creates a new `SteamDeckControllerBridge`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates a gamepad axis value (`[-1.0, 1.0]`).
    pub fn set_axis(&self, pad_id: usize, axis_name: &str, value: f64) {
        if let Ok(mut g) = self.axes.lock() {
            g.insert((pad_id, axis_name.to_string()), value.clamp(-1.0, 1.0));
        }
    }

    /// Updates a gamepad button value (`[0.0, 1.0]`).
    pub fn set_button(&self, pad_id: usize, button_name: &str, value: f64) {
        if let Ok(mut g) = self.buttons.lock() {
            g.insert((pad_id, button_name.to_string()), value.clamp(0.0, 1.0));
        }
    }

    /// Updates a `DeviceMotion` sensor channel (`gx`, `gy`, `gz`, `ax`, `ay`, `az`).
    pub fn set_motion(&self, channel: &str, value: f64) {
        if let Ok(mut g) = self.motion.lock() {
            g.insert(channel.to_string(), value.clamp(0.0, 1.0));
        }
    }
}

impl ControllerInputBridge for SteamDeckControllerBridge {
    fn gamepad_axis(&self, pad_id: usize, axis_name: &str) -> f64 {
        self.axes
            .lock()
            .ok()
            .and_then(|g| g.get(&(pad_id, axis_name.to_string())).copied())
            .unwrap_or(0.0)
    }

    fn gamepad_button(&self, pad_id: usize, button_name: &str) -> f64 {
        self.buttons
            .lock()
            .ok()
            .and_then(|g| g.get(&(pad_id, button_name.to_string())).copied())
            .unwrap_or(0.0)
    }

    fn device_motion(&self, channel: &str) -> f64 {
        self.motion
            .lock()
            .ok()
            .and_then(|g| g.get(channel).copied())
            .unwrap_or(0.0)
    }
}

/// Complete `@strudel/midi` Engine with deterministic priority ordering (`PP-12`),
/// active note-off safety (`MID-08`), and `.keepAudio(true)` simultaneous routing (`MID-01`..`MID-08`).
pub struct SilverWaveMidiEngine {
    pub port_id: String,
    pub keep_audio: bool,
    pub tracker: InMemoryActiveNoteTracker,
    pub maps: InMemoryMidiMapRegistry,
    pub input_state: InMemoryMidiInputState,
    pub transmitted: Mutex<Vec<ScheduledMidiPacket>>,
}

impl Default for SilverWaveMidiEngine {
    fn default() -> Self {
        Self::new("default_midi_out", true)
    }
}

impl SilverWaveMidiEngine {
    /// Creates a new `SilverWaveMidiEngine`.
    pub fn new(port_id: &str, keep_audio: bool) -> Self {
        Self {
            port_id: port_id.to_string(),
            keep_audio,
            tracker: InMemoryActiveNoteTracker::new(),
            maps: InMemoryMidiMapRegistry::new(),
            input_state: InMemoryMidiInputState::new(),
            transmitted: Mutex::new(Vec::new()),
        }
    }

    /// Encodes a single `DispatchEvent` into priority-ordered `ScheduledMidiPacket`s (`MID-01`..`MID-06`, `PP-12`).
    pub fn encode_dispatch_event(&self, ev: &DispatchEvent) -> Vec<ScheduledMidiPacket> {
        let mut packets = Vec::new();
        let ch_1based = ev
            .value
            .get("midichan")
            .and_then(ControlValue::as_i64)
            .unwrap_or(1)
            .clamp(1, 16) as u8;
        let ch = ch_1based - 1;
        let onset = ev.onset_secs;

        // 1. `midicmd` real-time single-byte messages (`MID-06`)
        if let Some(cmd) = ev.value.get("midicmd").and_then(ControlValue::as_str) {
            match cmd {
                "clock" | "midiClock" => {
                    // 24 PPQN across 1 beat (`1/4` cycle in 4/4 -> `96` ticks per cycle)
                    let tick_dt = 1.0 / (ev.cps.max(0.01) * 96.0);
                    let num_ticks = ((ev.duration_secs / tick_dt).round() as usize).max(1);
                    for k in 0..num_ticks {
                        packets.push(ScheduledMidiPacket {
                            time_secs: onset + (k as f64) * tick_dt,
                            priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                            bytes: vec![0xF8],
                        });
                    }
                }
                "start" => packets.push(ScheduledMidiPacket {
                    time_secs: onset,
                    priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                    bytes: vec![0xFA],
                }),
                "stop" => packets.push(ScheduledMidiPacket {
                    time_secs: onset,
                    priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                    bytes: vec![0xFC],
                }),
                "continue" => packets.push(ScheduledMidiPacket {
                    time_secs: onset,
                    priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                    bytes: vec![0xFB],
                }),
                _ => {}
            }
        }

        // 2. Bank Select MSB (`CC 0`) and LSB (`CC 32`) (`PP-12`)
        if let Some(msb) = ev.value.get("bankMsb").and_then(ControlValue::as_i64) {
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::BankSelectMsbCc0,
                bytes: vec![0xB0 | ch, 0, (msb.clamp(0, 127)) as u8],
            });
        }
        if let Some(lsb) = ev.value.get("bankLsb").and_then(ControlValue::as_i64) {
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::BankSelectLsbCc32,
                bytes: vec![0xB0 | ch, 32, (lsb.clamp(0, 127)) as u8],
            });
        }

        // 3. Program Change (`progNum` — `MID-02` / `PP-12`)
        if let Some(prog) = ev.value.get("progNum").and_then(ControlValue::as_i64) {
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::ProgramChange,
                bytes: vec![0xC0 | ch, (prog.clamp(0, 127)) as u8],
            });
        }

        // 4. `ccn` / `ccv` (`Section 4.2`)
        if let (Some(ccn), Some(ccv)) = (
            ev.value.get("ccn").and_then(ControlValue::as_i64),
            ev.value.get("ccv").and_then(ControlValue::as_f64),
        ) {
            let val_u7 = (ccv.clamp(0.0, 1.0) * 127.0).round() as u8;
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                bytes: vec![0xB0 | ch, (ccn.clamp(0, 127)) as u8, val_u7],
            });
        }

        // 5. `nrpnn` / `nrpv` 4-CC sequence (`CC 99`, `CC 98`, `CC 6`, `CC 38` — `MID-03`)
        if let (Some(ControlValue::Bytes(nrpnn)), Some(nrpv)) = (
            ev.value.get("nrpnn"),
            ev.value.get("nrpv").and_then(ControlValue::as_i64),
        ) {
            if nrpnn.len() >= 2 {
                let msb = nrpnn[0] & 0x7F;
                let lsb = nrpnn[1] & 0x7F;
                let data_msb = (nrpv.clamp(0, 127)) as u8;
                let data_lsb = 0u8;
                for (cc_num, cc_val) in [(99u8, msb), (98u8, lsb), (6u8, data_msb), (38u8, data_lsb)]
                {
                    packets.push(ScheduledMidiPacket {
                        time_secs: onset,
                        priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                        bytes: vec![0xB0 | ch, cc_num, cc_val],
                    });
                }
            }
        }

        // 6. `sysexid` + `sysexdata` (`0xF0 .. 0xF7` — `MID-04`)
        if let (Some(ControlValue::Bytes(sid)), Some(ControlValue::Bytes(sdata))) =
            (ev.value.get("sysexid"), ev.value.get("sysexdata"))
        {
            let mut sysex = Vec::with_capacity(2 + sid.len() + sdata.len());
            sysex.push(0xF0);
            for &b in sid.iter().chain(sdata.iter()) {
                sysex.push(b & 0x7F);
            }
            sysex.push(0xF7);
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                bytes: sysex,
            });
        }

        // 7. `midibend` (`-1.0..=1.0` -> 14-bit `0..=16383`, center `0.0 -> 8192 [0xE0, 0x00, 0x40]` — `MID-05`)
        if let Some(bend) = ev.value.get("midibend").and_then(ControlValue::as_f64) {
            let clamped = bend.clamp(-1.0, 1.0);
            let val_u14 = if clamped.abs() < 1.0e-9 {
                8192u16
            } else if clamped <= -1.0 {
                0u16
            } else {
                ((clamped + 1.0) * 8191.5).round() as u16
            };
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                bytes: vec![
                    0xE0 | ch,
                    (val_u14 & 0x7F) as u8,
                    ((val_u14 >> 7) & 0x7F) as u8,
                ],
            });
        }

        // 8. `miditouch` (`0.0..=1.0` -> `[0xD0 | ch, val_u7]` — `MID-05`)
        if let Some(touch) = ev.value.get("miditouch").and_then(ControlValue::as_f64) {
            let val_u7 = (touch.clamp(0.0, 1.0) * 127.0).round() as u8;
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                bytes: vec![0xD0 | ch, val_u7],
            });
        }

        // 9. `note` (`NoteOn` + `NoteOff` + zero-hack retrigger `NoteOff` — `MID-01`, `PP-12`)
        if let Some(note_i64) = ev.value.get("note").and_then(ControlValue::as_i64) {
            let note_u7 = (note_i64.clamp(0, 127)) as u8;
            let vel = ev
                .value
                .get("velocity")
                .and_then(ControlValue::as_f64)
                .unwrap_or(0.9);
            let gain = ev
                .value
                .get("gain")
                .and_then(ControlValue::as_f64)
                .unwrap_or(1.0);
            let vel_u7 = ((vel * gain).clamp(0.0, 1.0) * 127.0).round() as u8;
            let key = ActiveNoteKey {
                port_id: self.port_id.clone(),
                channel: ch,
                note: note_u7,
            };
            // If already active, emit immediate RetriggerNoteOff at priority 0 (`PP-12`)
            if self.tracker.remove_if_active(&key) {
                packets.push(ScheduledMidiPacket {
                    time_secs: onset,
                    priority: MidiEventOrderingPriority::RetriggerNoteOff,
                    bytes: vec![0x80 | ch, note_u7, 0],
                });
            }
            self.tracker
                .track_note_on(key, onset + ev.duration_secs, &ev.stream_id);
            packets.push(ScheduledMidiPacket {
                time_secs: onset,
                priority: MidiEventOrderingPriority::NoteOn,
                bytes: vec![0x90 | ch, note_u7, vel_u7],
            });
            packets.push(ScheduledMidiPacket {
                time_secs: onset + ev.duration_secs,
                priority: MidiEventOrderingPriority::ScheduledNoteOff,
                bytes: vec![0x80 | ch, note_u7, 0],
            });
        }

        packets.sort_by(|a, b| {
            a.time_secs
                .partial_cmp(&b.time_secs)
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(a.priority.cmp(&b.priority))
        });
        packets
    }

    /// Returns all transmitted raw MIDI byte sequences in chronological + priority order.
    pub fn transmitted_bytes(&self) -> Vec<Vec<u8>> {
        self.transmitted
            .lock()
            .map(|g| g.iter().map(|p| p.bytes.clone()).collect())
            .unwrap_or_default()
    }
}

impl OutputSink for SilverWaveMidiEngine {
    fn name(&self) -> &str {
        &self.port_id
    }

    fn send_batch(&self, events: &[DispatchEvent]) -> Result<(), IoError> {
        let mut all = Vec::new();
        for ev in events {
            all.extend(self.encode_dispatch_event(ev));
        }
        all.sort_by(|a, b| {
            a.time_secs
                .partial_cmp(&b.time_secs)
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(a.priority.cmp(&b.priority))
        });
        if let Ok(mut guard) = self.transmitted.lock() {
            guard.extend(all);
        }
        Ok(())
    }

    fn panic(&self) -> Result<(), IoError> {
        let active = self.tracker.drain_all_note_offs();
        if let Ok(mut guard) = self.transmitted.lock() {
            for k in active {
                guard.push(ScheduledMidiPacket {
                    time_secs: 0.0,
                    priority: MidiEventOrderingPriority::RetriggerNoteOff,
                    bytes: vec![0x80 | (k.channel & 0x0F), k.note & 0x7F, 0],
                });
            }
            for ch in 0..16u8 {
                guard.push(ScheduledMidiPacket {
                    time_secs: 0.0,
                    priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                    bytes: vec![0xB0 | ch, 123, 0],
                });
                guard.push(ScheduledMidiPacket {
                    time_secs: 0.0,
                    priority: MidiEventOrderingPriority::ControlChangeAndPitchBend,
                    bytes: vec![0xB0 | ch, 120, 0],
                });
            }
        }
        Ok(())
    }

    fn close(&self) -> Result<(), IoError> {
        self.panic()
    }
}

impl Drop for SilverWaveMidiEngine {
    fn drop(&mut self) {
        let _ = self.panic();
    }
}

/// Helper building a `ControlMap` for MIDI tests.
pub fn midi_control_map(entries: Vec<(&str, ControlValue)>) -> ControlMap {
    let mut m = ControlMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), v);
    }
    m
}
