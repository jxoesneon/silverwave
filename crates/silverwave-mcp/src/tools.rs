//! Complete 42-Tool Surface (`SilverWaveMcpToolSurface` 38 Core Tools +
//! `SilverWaveMcpCategoryKingExtension` 4 Category-King Tools & `AcousticMixDiagnostic`)
//! (`07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md` §2 & §7).

#![forbid(unsafe_code)]

use crate::protocol::{ActorOrigin, McpProtocolError, QuantizeGrid, SourceSpan};
use silverwave_core::Rational64;
use std::collections::BTreeMap;
use std::path::PathBuf;

// ============================================================================
// 2.1 Domain 1: Pattern & DSL Tool Contracts
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBufferRequest {
    pub code: Option<String>,
    pub sync_to_editor: bool,
    pub expected_revision: Option<u64>,
    pub quantize: QuantizeGrid,
    pub auto_start_clock: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBufferResponse {
    pub revision: u64,
    pub scheduled_at_cycle: Rational64,
    pub active_streams: Vec<String>,
    pub discovered_widgets: Vec<SliderWidgetDescriptor>,
    pub diagnostics: Vec<SyntaxDiagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBlockRequest {
    pub cursor_line: Option<u32>,
    pub line_range: Option<(u32, u32)>,
    pub byte_range: Option<(usize, usize)>,
    pub target_stream_override: Option<String>,
    pub quantize: QuantizeGrid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBlockResponse {
    pub buffer_revision: u64,
    pub evaluated_span: SourceSpan,
    pub target_stream: String,
    pub scheduled_at_cycle: Rational64,
    pub highlight_spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseMiniNotationRequest {
    pub notation: String,
    pub base_byte_offset: usize,
    pub base_line: u32,
    pub base_col: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MiniAstNode {
    Atom {
        value: String,
        span: SourceSpan,
    },
    Rest {
        span: SourceSpan,
    },
    Sequence {
        children: Vec<MiniAstNode>,
        span: SourceSpan,
    },
    Stack {
        layers: Vec<MiniAstNode>,
        span: SourceSpan,
    },
    Slowcat {
        alternatives: Vec<MiniAstNode>,
        span: SourceSpan,
    },
    Polymeter {
        children: Vec<MiniAstNode>,
        steps_per_cycle: Option<u32>,
        span: SourceSpan,
    },
    Bjorklund {
        child: Box<MiniAstNode>,
        pulses: u32,
        steps: u32,
        rotation: i32,
        span: SourceSpan,
    },
    Fast {
        child: Box<MiniAstNode>,
        factor: Rational64,
        span: SourceSpan,
    },
    Slow {
        child: Box<MiniAstNode>,
        factor: Rational64,
        span: SourceSpan,
    },
    Weight {
        child: Box<MiniAstNode>,
        weight: f64,
        span: SourceSpan,
    },
    Replicate {
        child: Box<MiniAstNode>,
        count: u32,
        span: SourceSpan,
    },
    Degrade {
        child: Box<MiniAstNode>,
        probability: f64,
        span: SourceSpan,
    },
    RandomChoice {
        options: Vec<(MiniAstNode, f64)>,
        span: SourceSpan,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseMiniNotationResponse {
    pub ast: MiniAstNode,
    pub leaf_spans: Vec<SourceSpan>,
    pub implied_steps_per_cycle: Rational64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryPatternArcRequest {
    pub pattern_code: Option<String>,
    pub stream_id: Option<String>,
    pub begin_cycle: Rational64,
    pub end_cycle: Rational64,
    pub max_events: u32,
    pub deterministic_seed: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlValue {
    Float(f64),
    Int(i64),
    Rational(Rational64),
    Text(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HapEventRecord {
    pub whole: Option<(Rational64, Rational64)>,
    pub part: (Rational64, Rational64),
    pub controls: BTreeMap<String, ControlValue>,
    pub source_spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryPatternArcResponse {
    pub query_arc: (Rational64, Rational64),
    pub total_events: usize,
    pub truncated: bool,
    pub events: Vec<HapEventRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidateSyntaxRequest {
    pub code: String,
    pub verify_sample_banks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Hint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub span: SourceSpan,
    pub suggested_replacement: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidateSyntaxResponse {
    pub valid: bool,
    pub diagnostics: Vec<SyntaxDiagnostic>,
    pub referenced_streams: Vec<String>,
    pub referenced_samples: Vec<String>,
    pub sliders: Vec<SliderWidgetDescriptor>,
}

// ============================================================================
// 2.2 Domain 2: Streams & Tidal Transitions Tool Contracts
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct SetStreamRequest {
    pub stream_id: String,
    pub pattern_code: String,
    pub orbit: Option<u8>,
    pub quantize: QuantizeGrid,
    pub sync_to_editor: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetStreamResponse {
    pub stream_id: String,
    pub orbit: u8,
    pub active_from_cycle: Rational64,
    pub buffer_revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TidalTransitionSpec {
    Xfade {
        cycles: f64,
    },
    Clutch {
        cycles: f64,
    },
    Anticipate {
        cycles: f64,
    },
    Interpolate {
        cycles: f64,
    },
    Jump,
    JumpIn {
        cycles: u32,
    },
    JumpMod {
        modulo: u32,
        offset: u32,
    },
    Histpan {
        pan_slots: u8,
    },
    Wait {
        delay_cycles: f64,
    },
    Wash {
        wash_expr: String,
        in_cycles: f64,
        out_cycles: f64,
    },
    Mortal {
        lifespan_cycles: f64,
        degrade_cycles: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionStreamRequest {
    pub stream_id: String,
    pub new_pattern_code: String,
    pub transition: TidalTransitionSpec,
    pub sync_to_editor: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionStreamResponse {
    pub stream_id: String,
    pub transition_kind: String,
    pub start_cycle: Rational64,
    pub completion_cycle: Rational64,
    pub buffer_revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamToggleRequest {
    pub stream_ids: Vec<String>,
    pub quantize: QuantizeGrid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamStateSummary {
    pub stream_id: String,
    pub muted: bool,
    pub soloed: bool,
    pub effectively_audible: bool,
    pub orbit: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamToggleResponse {
    pub applied_at_cycle: Rational64,
    pub streams: Vec<StreamStateSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HushRequest {
    pub quantize: QuantizeGrid,
    pub release_solo_and_mute: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HushResponse {
    pub silenced_streams: Vec<String>,
    pub effective_cycle: Rational64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanicRequest {
    pub send_midi_all_notes_off: bool,
    pub clear_delay_reverb_tails: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanicResponse {
    pub killed_voices: u32,
    pub cleared_orbits: u8,
    pub midi_ports_flushed: usize,
    pub sample_frame: u64,
}

// ============================================================================
// 2.3 Domain 3: Clock & Ableton Link Tool Contracts
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetClockSnapshotRequest {}

#[derive(Debug, Clone, PartialEq)]
pub struct AbletonLinkStatusResponse {
    pub enabled: bool,
    pub num_peers: usize,
    pub quantum_beats: f64,
    pub start_stop_sync: bool,
    pub link_beat: f64,
    pub link_phase: f64,
    pub latency_offset_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClockSnapshotResponse {
    pub is_playing: bool,
    pub cycle_rational: Rational64,
    pub cycle_f64: f64,
    pub cps: f64,
    pub bpm: f64,
    pub beats_per_cycle: f64,
    pub bar_index: u64,
    pub beat_in_bar: f64,
    pub sample_frame: u64,
    pub sample_rate: u32,
    pub audio_buffer_frames: u32,
    pub xrun_count: u64,
    pub link: AbletonLinkStatusResponse,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetCpsRequest {
    pub cps: f64,
    pub ramp_cycles: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetBpmRequest {
    pub bpm: f64,
    pub beats_per_cycle: Option<f64>,
    pub ramp_cycles: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetBeatsPerCycleRequest {
    pub beats_per_cycle: f64,
    pub preserve_bpm: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NudgeClockRequest {
    pub phase_delta_cycles: Option<f64>,
    pub offset_ms: Option<f64>,
    pub slew_duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureAbletonLinkRequest {
    pub enabled: bool,
    pub quantum_beats: Option<f64>,
    pub start_stop_sync: Option<bool>,
    pub latency_offset_ms: Option<f64>,
}

// ============================================================================
// 2.4 Domain 4: Interactive Widgets & Controls Tool Contracts
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSlidersRequest {}

#[derive(Debug, Clone, PartialEq)]
pub struct SliderWidgetDescriptor {
    pub widget_id: String,
    pub index_in_buffer: usize,
    pub label: Option<String>,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
    pub call_span: SourceSpan,
    pub value_literal_span: SourceSpan,
    pub owning_stream_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListSlidersResponse {
    pub buffer_revision: u64,
    pub sliders: Vec<SliderWidgetDescriptor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateSliderRequest {
    pub widget_id: String,
    pub value: f64,
    pub patch_source_in_place: bool,
    pub slew_ms: f64,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateSliderResponse {
    pub widget_id: String,
    pub clamped_value: f64,
    pub new_buffer_revision: u64,
    pub patched_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetControlStateRequest {
    pub controls: BTreeMap<String, ControlValue>,
    pub slew_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetControlStateResponse {
    pub updated_keys: Vec<String>,
    pub applied_at_cycle: Rational64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetControlStateRequest {
    pub keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlStateEntry {
    pub key: String,
    pub value: ControlValue,
    pub updated_at_cycle: Rational64,
    pub origin: ActorOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetControlStateResponse {
    pub entries: BTreeMap<String, ControlStateEntry>,
}

// ============================================================================
// 2.5 Domain 5: DSP, Orbits & Samples Tool Contracts
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct ListSampleBanksRequest {
    pub filter_prefix: Option<String>,
    pub include_variants: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleBankEntry {
    pub bank_name: String,
    pub variant_count: usize,
    pub loaded_variants: usize,
    pub source_kind: String,
    pub memory_bytes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListSampleBanksResponse {
    pub total_banks: usize,
    pub total_memory_bytes: usize,
    pub banks: Vec<SampleBankEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SampleSourceDescriptor {
    StrudelJsonManifest {
        url_or_path: String,
    },
    GitHubRepo {
        owner_repo: String,
        git_ref: Option<String>,
        subpath: Option<String>,
    },
    LocalDirectory {
        path: PathBuf,
    },
    SoundFont2 {
        path: PathBuf,
        bank_prefix: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadSampleSourceRequest {
    pub source: SampleSourceDescriptor,
    pub bank_prefix: Option<String>,
    pub eager_decode: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadSampleSourceResponse {
    pub registered_banks: Vec<String>,
    pub total_samples_registered: usize,
    pub decoded_immediately: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOrbitRequest {
    pub orbit_index: u8,
    pub gain_db: Option<f64>,
    pub pan: Option<f64>,
    pub muted: Option<bool>,
    pub reverb_room: Option<f64>,
    pub reverb_size: Option<f64>,
    pub reverb_ir_sample: Option<String>,
    pub delay_send: Option<f64>,
    pub delay_time_sec: Option<f64>,
    pub delay_feedback: Option<f64>,
    pub duck_target_orbit: Option<u8>,
    pub duck_attack_sec: Option<f64>,
    pub duck_depth: Option<f64>,
    pub output_channel_pair: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrbitStateSnapshot {
    pub orbit_index: u8,
    pub gain_db: f64,
    pub pan: f64,
    pub muted: bool,
    pub active_voices: u32,
    pub rms_dbfs_l: f32,
    pub rms_dbfs_r: f32,
    pub peak_dbfs_l: f32,
    pub peak_dbfs_r: f32,
    pub compressor_gr_db: f32,
    pub reverb_room: f64,
    pub delay_send: f64,
    pub delay_feedback: f64,
    pub duck_target_orbit: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOrbitResponse {
    pub orbit: OrbitStateSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetOrbitTelemetryRequest {
    pub orbit_indices: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetOrbitTelemetryResponse {
    pub dsp_cpu_load_percent: f32,
    pub total_active_voices: u32,
    pub orbits: Vec<OrbitStateSnapshot>,
}

// ============================================================================
// 2.6 Domain 6: External I/O Tool Contracts (MIDI, OSC & Csound)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMidiPortsRequest {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiPortDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortDescriptor {
    pub port_id: String,
    pub display_name: String,
    pub direction: MidiPortDirection,
    pub connected: bool,
    pub clock_sync_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMidiPortsResponse {
    pub inputs: Vec<MidiPortDescriptor>,
    pub outputs: Vec<MidiPortDescriptor>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MidiMappingTarget {
    SliderWidget { widget_id: String },
    ControlState { key: String },
    StreamMuteToggle { stream_id: String },
    StreamSoloToggle { stream_id: String },
    HushTrigger,
    TapTempo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureMidiMapRequest {
    pub mapping_id: String,
    pub input_port_id: String,
    pub channel: Option<u8>,
    pub cc_or_note: u8,
    pub is_note_trigger: bool,
    pub target_kind: MidiMappingTarget,
    pub range_min: f64,
    pub range_max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureMidiMapResponse {
    pub mapping_id: String,
    pub active_mappings_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MidiOutboundCommand {
    NoteOn {
        note: u8,
        velocity: u8,
        duration_cycles: Option<f64>,
    },
    NoteOff {
        note: u8,
        velocity: u8,
    },
    ControlChange {
        cc: u8,
        value: u8,
    },
    ProgramChange {
        program: u8,
    },
    PitchBend {
        value_14bit: u16,
    },
    ClockStart,
    ClockStop,
    ClockContinue,
    SysEx {
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendMidiCommandRequest {
    pub output_port_id: String,
    pub channel: u8,
    pub command: MidiOutboundCommand,
    pub schedule_at_cycle: Option<Rational64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendMidiCommandResponse {
    pub dispatched: bool,
    pub scheduled_cycle: Rational64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOscTargetRequest {
    pub target_name: String,
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub address_path: String,
    pub lookahead_latency_ms: f64,
    pub superdirt_compatible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOscTargetResponse {
    pub target_name: String,
    pub active: bool,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadCsoundOrcRequest {
    pub orc_code: Option<String>,
    pub orc_file_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadCsoundOrcResponse {
    pub compiled_successfully: bool,
    pub registered_instruments: Vec<String>,
    pub compiler_messages: Vec<String>,
}

// ============================================================================
// 2.7 Domain 7: Visualizers, Telemetry & Export Tool Contracts
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerKind {
    Pianoroll,
    Punchcard,
    Spiral,
    Scope,
    Pitchwheel,
    Spectrum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerOutputFormat {
    DrawAstJson,
    Svg,
    PngBase64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    Clear {
        rgba: [f32; 4],
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        corner_radius: f32,
        fill_rgba: [f32; 4],
        stroke_rgba: Option<([f32; 4], f32)>,
        active_hap: bool,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        rgba: [f32; 4],
    },
    Polyline {
        points: Vec<(f32, f32)>,
        width: f32,
        rgba: [f32; 4],
        closed: bool,
    },
    ArcSegment {
        cx: f32,
        cy: f32,
        r_inner: f32,
        r_outer: f32,
        start_rad: f32,
        end_rad: f32,
        fill_rgba: [f32; 4],
    },
    Text {
        x: f32,
        y: f32,
        size_px: f32,
        rgba: [f32; 4],
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureVisualizerFrameRequest {
    pub kind: VisualizerKind,
    pub format: VisualizerOutputFormat,
    pub stream_id: Option<String>,
    pub window_cycles: f64,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureVisualizerFrameResponse {
    pub kind: VisualizerKind,
    pub playhead_cycle: Rational64,
    pub draw_commands: Option<Vec<DrawCmd>>,
    pub svg_markup: Option<String>,
    pub png_base64: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadAudioTelemetryRequest {
    pub orbit_index: Option<u8>,
    pub fft_bins: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HydraAudioUniforms {
    pub fft_bands: [f32; 4],
    pub rms_normalized: f32,
    pub peak_normalized: f32,
    pub bpm: f32,
    pub cycle_time: f64,
}

/// Closed-loop acoustic mix diagnostic (`CK-06` — `07-SPEC` §7).
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictKind {
    SubBassPhaseCancellation,
    LowMidSpectralMasking,
    TruePeakClippingRisk,
}

/// Inter-orbit spectral masking or sub-bass phase conflict alert (`CK-06` — `07-SPEC` §7).
#[derive(Debug, Clone, PartialEq)]
pub struct InterOrbitConflictAlert {
    pub orbit_a: u8,
    pub orbit_b: u8,
    pub center_freq_hz: f32,
    pub severity_db: f32,
    pub kind: ConflictKind,
}

/// Closed-loop LUFS/dBTP/phase/1/3-octave mix diagnostic (`CK-06` — `07-SPEC` §7).
#[derive(Debug, Clone, PartialEq)]
pub struct AcousticMixDiagnostic {
    pub integrated_lufs: f32,
    pub short_term_lufs: f32,
    pub true_peak_dbtp: f32,
    pub crest_factor_db: f32,
    pub stereo_phase_correlation: f32,
    pub third_octave_spectrum_db: [f32; 31],
    pub masking_conflicts: Vec<InterOrbitConflictAlert>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadAudioTelemetryResponse {
    pub sample_frame: u64,
    pub rms_dbfs_l: f32,
    pub rms_dbfs_r: f32,
    pub peak_dbfs_l: f32,
    pub peak_dbfs_r: f32,
    pub momentary_lufs: f32,
    pub spectral_centroid_hz: f32,
    pub zero_crossing_rate: f32,
    pub fft_magnitude_db: Vec<f32>,
    pub hydra_uniforms: HydraAudioUniforms,
    pub mix_diagnostic: AcousticMixDiagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioExportFormat {
    WavPcm16,
    WavPcm24,
    WavFloat32,
    Flac16,
    Flac24,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportAudioRequest {
    pub output_path: PathBuf,
    pub format: AudioExportFormat,
    pub code_override: Option<String>,
    pub cycles: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub sample_rate: u32,
    pub split_orbit_stems: bool,
    pub normalize_lufs: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportAudioResponse {
    pub written_files: Vec<PathBuf>,
    pub rendered_frames: u64,
    pub duration_seconds: f64,
    pub peak_dbfs: f32,
    pub integrated_lufs: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncodeShareUrlRequest {
    pub code: Option<String>,
    pub base_url: String,
    pub include_session_metadata: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncodeShareUrlResponse {
    pub share_url: String,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeShareUrlRequest {
    pub share_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeShareUrlResponse {
    pub code: String,
    pub cps: Option<f64>,
    pub referenced_sample_sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDoctorRequest {
    pub run_audio_loopback_probe: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoctorSubsystemCheck {
    pub subsystem: String,
    pub healthy: bool,
    pub latency_ms: Option<f64>,
    pub summary: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunDoctorResponse {
    pub overall_healthy: bool,
    pub checks: Vec<DoctorSubsystemCheck>,
}

// ============================================================================
// Category-King 4 Extension Tool Requests (`07-SPEC` §7)
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct SetCrossfaderSceneRequest {
    pub scene_a_id: Option<u8>,
    pub scene_b_id: Option<u8>,
    pub xfader_position: f32,
    pub slew_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerLooperFreezeRequest {
    pub enabled: bool,
    pub freeze_cycles: Rational64,
    pub target_orbit: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetFillLatchRequest {
    pub fill_active: bool,
    pub quantize: QuantizeGrid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderVideoRequest {
    pub output_path: PathBuf,
    pub preset: String,
    pub fps: u16,
    pub cycles: f64,
    pub embed_qr_permalink: bool,
}

/// Complete 38-method core MCP tool surface (`07-SPEC` §2.0).
pub trait SilverWaveMcpToolSurface: Send + Sync {
    fn evaluate_buffer(
        &mut self,
        req: EvaluateBufferRequest,
    ) -> Result<EvaluateBufferResponse, McpProtocolError>;
    fn evaluate_block(
        &mut self,
        req: EvaluateBlockRequest,
    ) -> Result<EvaluateBlockResponse, McpProtocolError>;
    fn parse_mini_notation(
        &self,
        req: ParseMiniNotationRequest,
    ) -> Result<ParseMiniNotationResponse, McpProtocolError>;
    fn query_pattern_arc(
        &self,
        req: QueryPatternArcRequest,
    ) -> Result<QueryPatternArcResponse, McpProtocolError>;
    fn validate_syntax(
        &self,
        req: ValidateSyntaxRequest,
    ) -> Result<ValidateSyntaxResponse, McpProtocolError>;

    fn set_stream(&mut self, req: SetStreamRequest) -> Result<SetStreamResponse, McpProtocolError>;
    fn transition_stream(
        &mut self,
        req: TransitionStreamRequest,
    ) -> Result<TransitionStreamResponse, McpProtocolError>;
    fn mute_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError>;
    fn unmute_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError>;
    fn solo_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError>;
    fn unsolo_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError>;
    fn hush(&mut self, req: HushRequest) -> Result<HushResponse, McpProtocolError>;
    fn panic(&mut self, req: PanicRequest) -> Result<PanicResponse, McpProtocolError>;

    fn get_clock_snapshot(
        &self,
        req: GetClockSnapshotRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_cps(&mut self, req: SetCpsRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_bpm(&mut self, req: SetBpmRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_beats_per_cycle(
        &mut self,
        req: SetBeatsPerCycleRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn nudge_clock(
        &mut self,
        req: NudgeClockRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn configure_ableton_link(
        &mut self,
        req: ConfigureAbletonLinkRequest,
    ) -> Result<AbletonLinkStatusResponse, McpProtocolError>;

    fn list_sliders(
        &self,
        req: ListSlidersRequest,
    ) -> Result<ListSlidersResponse, McpProtocolError>;
    fn update_slider(
        &mut self,
        req: UpdateSliderRequest,
    ) -> Result<UpdateSliderResponse, McpProtocolError>;
    fn set_control_state(
        &mut self,
        req: SetControlStateRequest,
    ) -> Result<SetControlStateResponse, McpProtocolError>;
    fn get_control_state(
        &self,
        req: GetControlStateRequest,
    ) -> Result<GetControlStateResponse, McpProtocolError>;

    fn list_sample_banks(
        &self,
        req: ListSampleBanksRequest,
    ) -> Result<ListSampleBanksResponse, McpProtocolError>;
    fn load_sample_source(
        &mut self,
        req: LoadSampleSourceRequest,
    ) -> Result<LoadSampleSourceResponse, McpProtocolError>;
    fn configure_orbit(
        &mut self,
        req: ConfigureOrbitRequest,
    ) -> Result<ConfigureOrbitResponse, McpProtocolError>;
    fn get_orbit_telemetry(
        &self,
        req: GetOrbitTelemetryRequest,
    ) -> Result<GetOrbitTelemetryResponse, McpProtocolError>;

    fn list_midi_ports(
        &self,
        req: ListMidiPortsRequest,
    ) -> Result<ListMidiPortsResponse, McpProtocolError>;
    fn configure_midi_map(
        &mut self,
        req: ConfigureMidiMapRequest,
    ) -> Result<ConfigureMidiMapResponse, McpProtocolError>;
    fn send_midi_command(
        &mut self,
        req: SendMidiCommandRequest,
    ) -> Result<SendMidiCommandResponse, McpProtocolError>;
    fn configure_osc_target(
        &mut self,
        req: ConfigureOscTargetRequest,
    ) -> Result<ConfigureOscTargetResponse, McpProtocolError>;
    fn load_csound_orc(
        &mut self,
        req: LoadCsoundOrcRequest,
    ) -> Result<LoadCsoundOrcResponse, McpProtocolError>;

    fn capture_visualizer_frame(
        &self,
        req: CaptureVisualizerFrameRequest,
    ) -> Result<CaptureVisualizerFrameResponse, McpProtocolError>;
    fn read_audio_telemetry(
        &self,
        req: ReadAudioTelemetryRequest,
    ) -> Result<ReadAudioTelemetryResponse, McpProtocolError>;
    fn export_audio(
        &mut self,
        req: ExportAudioRequest,
    ) -> Result<ExportAudioResponse, McpProtocolError>;
    fn encode_share_url(
        &self,
        req: EncodeShareUrlRequest,
    ) -> Result<EncodeShareUrlResponse, McpProtocolError>;
    fn decode_share_url(
        &self,
        req: DecodeShareUrlRequest,
    ) -> Result<DecodeShareUrlResponse, McpProtocolError>;
    fn run_doctor(&self, req: RunDoctorRequest) -> Result<RunDoctorResponse, McpProtocolError>;
}

/// 4 Category-King MCP tools bringing the total surface to 42 tools (`07-SPEC` §7).
pub trait SilverWaveMcpCategoryKingExtension: SilverWaveMcpToolSurface {
    fn set_crossfader_scene(
        &mut self,
        req: SetCrossfaderSceneRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn trigger_looper_freeze(
        &mut self,
        req: TriggerLooperFreezeRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_fill_latch(
        &mut self,
        req: SetFillLatchRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn render_video(
        &mut self,
        req: RenderVideoRequest,
    ) -> Result<ExportAudioResponse, McpProtocolError>;
}
