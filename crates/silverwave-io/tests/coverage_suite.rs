//! Supplementary coverage suite for `silverwave-io` — exercises public API paths
//! not reached by `io_v0_4_0_tdd_suite.rs` (guard branches, error returns, default
//! constructors, every `TransitionSpec` variant, OSC defaults/interpolation, MIDI
//! bank/program/clock encoding, MPE upper-zone allocation, Link-mesh propagation).

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use silverwave_core::{
    ControlMap as CoreControlMap, ControlPattern, Pattern, PatternFunctor, Time, Value, pure,
};
use silverwave_io::{
    ActiveNoteKey, ActiveNoteTracker, ActorKind, AudioEventProducer, AudioFormat,
    AudioRenderEngine, AudioThreadEvent, ClockEngine, ClockSource, CommitEpoch, ControlMap,
    ControlValue, ControllerInputBridge, CpalBackend, CpalConfig, CrdtEnsembleSession,
    CsoundEngine, CycleArc, CycleTime, DispatchEvent, ExternalMidiClockPll, FadePrimitives,
    HostTransportSnapshot, ImpulseAudioRenderEngine, InMemoryActiveNoteTracker,
    InMemoryMidiInputState, InMemoryMidiMapRegistry, InProcessCsoundEngine, IoError, LinkClock,
    LinkSessionMesh, MemoryCpalBackend, MemoryWebAudioBackend, MidiCcMapping, MidiInputState,
    MidiMap, MidiMapRegistry, MidiNoteValue, MidiScaleCurve, MpeChannelAllocator,
    MpeNoteExpression, MpeZoneConfig, NihPlugBridge, OrcSource, OscArgDefault, OscArgSpec,
    OscBundle, OscEngine, OscMessage, OscPacket, OscPayloadMode, OscScheduleMode, OscShape,
    OscTarget, OscTime, OscType, OutputSink, PluginSyncMode, RemoteStreamCommand,
    SilverWaveMidiEngine, SilverWaveNihPlugBridge, SilverWaveOscEngine, SilverWaveStreamRegistry,
    SteamDeckControllerBridge, StreamId, StreamManager, TransitionEngine, TransitionSpec,
    WebAudioBackend, WebAudioConfig, compile_transition_pattern, encode_osc_packet,
    midi_control_map, query_transition_arc,
};

fn const_pat(sound: &str, cutoff: f64) -> ControlPattern {
    let mut m = CoreControlMap::new();
    m.insert("s".to_string(), Value::String(sound.to_string()));
    m.insert("cutoff".to_string(), Value::Float(cutoff));
    m.insert("gain".to_string(), Value::Float(1.0));
    pure(m)
}

fn wash_out_transform(p: ControlPattern) -> ControlPattern {
    p.fmap(|m| {
        let mut c = m.clone();
        c.insert("wash_out".to_string(), Value::Float(1.0));
        c
    })
}

fn wash_in_transform(p: ControlPattern) -> ControlPattern {
    p.fmap(|m| {
        let mut c = m.clone();
        c.insert("wash_in".to_string(), Value::Float(1.0));
        c
    })
}

fn handle_osc_msg(
    state_map: &mut BTreeMap<String, ControlValue>,
    cmds: &mut Vec<RemoteStreamCommand>,
    addr: &str,
    args: Vec<OscType>,
) {
    SilverWaveOscEngine::handle_incoming_message(
        &OscMessage {
            addr: addr.to_string(),
            args,
        },
        state_map,
        cmds,
    );
}

fn dispatch_event(value: ControlMap, onset: f64, dur: f64, cps: f64) -> DispatchEvent {
    DispatchEvent {
        whole: Some(CycleArc::new(CycleTime::zero(), CycleTime::one())),
        part: CycleArc::new(CycleTime::zero(), CycleTime::one()),
        value,
        onset_secs: onset,
        duration_secs: dur,
        cps,
        stream_id: StreamId::numbered(1),
    }
}

// ---------------------------------------------------------------------------
// types.rs — ControlValue coercion arms + IoError Display variants
// ---------------------------------------------------------------------------
#[test]
fn cov_types_control_value_and_error_display() {
    // as_f64: Int arm + catch-all None arm
    assert_eq!(ControlValue::Int(7).as_f64(), Some(7.0));
    assert_eq!(ControlValue::Str("x".to_string()).as_f64(), None);
    assert_eq!(ControlValue::Bool(true).as_f64(), None);
    // as_i64: Float arm + catch-all None arm
    assert_eq!(ControlValue::Float(3.9).as_i64(), Some(3));
    assert_eq!(ControlValue::Str("x".to_string()).as_i64(), None);
    // Note extraction through both numeric paths
    let note = ControlValue::Note(MidiNoteValue {
        midi_note: 72,
        detune_cents: 12.0,
    });
    assert_eq!(note.as_f64(), Some(72.0));
    assert_eq!(note.as_i64(), Some(72));
    // as_str: hit arm and miss arm
    assert_eq!(ControlValue::Str("abc".to_string()).as_str(), Some("abc"));
    assert_eq!(ControlValue::Int(1).as_str(), None);
    assert_eq!(ControlValue::Bytes(vec![1, 2]).as_str(), None);

    // IoError Display — every variant's message format
    assert_eq!(
        format!("{}", IoError::Audio("a".to_string())),
        "audio backend error: a"
    );
    assert_eq!(
        format!("{}", IoError::Plugin("p".to_string())),
        "plugin host error: p"
    );
    assert_eq!(
        format!("{}", IoError::Csound("c".to_string())),
        "csound engine error: c"
    );
    assert_eq!(
        format!("{}", IoError::Osc("o".to_string())),
        "osc transport error: o"
    );
    assert_eq!(
        format!("{}", IoError::Midi("m".to_string())),
        "midi port error: m"
    );
    assert_eq!(
        format!("{}", IoError::Link("l".to_string())),
        "ableton link error: l"
    );
    assert_eq!(
        format!("{}", IoError::StreamNotFound("s".to_string())),
        "invalid stream id: s"
    );
    assert_eq!(
        format!("{}", IoError::Config("cfg".to_string())),
        "configuration error: cfg"
    );

    // CycleArc round-trip through TimeSpan
    let arc = CycleArc::new(CycleTime::from_integer(2), CycleTime::from_integer(5));
    assert_eq!(CycleArc::from_timespan(arc.to_timespan()), arc);
}

// ---------------------------------------------------------------------------
// backends.rs — producer contention/full ring, WebAudio posting, plugin edges
// ---------------------------------------------------------------------------
#[test]
fn cov_backends_audio_and_plugin_paths() {
    // SpscAudioEventProducer success + ring-full error path
    let cfg = CpalConfig {
        ring_capacity_events: 16,
        ..CpalConfig::default()
    };
    let mut cpal =
        MemoryCpalBackend::start(cfg, Box::new(ImpulseAudioRenderEngine::default())).unwrap();
    let sender = cpal.event_sender();
    let mut cmap = ControlMap::new();
    cmap.insert("gain".to_string(), ControlValue::Float(0.5));
    let mk_ev = |offset: u32| AudioThreadEvent {
        frame_offset: offset,
        duration_frames: 8,
        orbit: 1,
        controls: cmap.clone(),
    };
    // Overflow the 16-slot ring without rendering -> "ring buffer full" error
    let mut pushes = 0usize;
    let mut errs = 0usize;
    for i in 0..24u32 {
        match sender.try_push(mk_ev(i)) {
            Ok(()) => pushes += 1,
            Err(IoError::Audio(msg)) => {
                errs += 1;
                assert_eq!(msg, "audio SPSC ring buffer full");
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
    assert_eq!(pushes, 16);
    assert_eq!(errs, 8);

    // Drain through render_next_block; first queued event lands at frame 0
    let mut buf = vec![0.0f32; 128 * 2];
    cpal.render_next_block(&mut buf);
    assert!((buf[0] - 0.5).abs() < 1.0e-6);
    assert!((buf[1] - 0.5).abs() < 1.0e-6); // second channel also written
    let fmt = cpal.format();
    cpal.stop().unwrap();

    // WebAudio post_event_batch records events; render_offline matches impulse engine
    let web = MemoryWebAudioBackend::initialize(WebAudioConfig::default()).unwrap();
    web.post_event_batch(&[mk_ev(4), mk_ev(9)]).unwrap();
    assert_eq!(web.posted.lock().unwrap().len(), 2);
    web.suspend().unwrap();
    web.resume().unwrap();
    assert!(web.running.load(std::sync::atomic::Ordering::Relaxed));
    let offline = MemoryWebAudioBackend::render_offline(fmt, 0.01, &[mk_ev(2)]).unwrap();
    assert!((offline[2 * 2] - 0.5).abs() < 1.0e-6);

    // NihPlugBridge: Default impl, sync mode setter, guard branches, beat pulses
    let mut plug = SilverWaveNihPlugBridge::default();
    assert_eq!(plug.sync_mode(), PluginSyncMode::HostTransport);
    plug.set_sync_mode(PluginSyncMode::InternalOrLink);
    assert_eq!(plug.sync_mode(), PluginSyncMode::InternalOrLink);
    plug.set_sync_mode(PluginSyncMode::HostTransport);

    // playing == false -> early return, no MIDI emitted
    plug.on_transport_update(
        &HostTransportSnapshot {
            playing: false,
            tempo_bpm: 120.0,
            pos_beats: Some(0.0),
            pos_samples: Some(0),
            time_sig_num: 4,
            time_sig_den: 4,
        },
        48_000.0,
    );
    let mut audio = vec![0.0f32; 256];
    let mut midi_out = Vec::new();
    plug.render_plugin_buffer(128, 2, &mut audio, &mut midi_out);
    assert!(midi_out.is_empty());

    // buffer_frames == 0 -> early return via second guard arm
    plug.on_transport_update(
        &HostTransportSnapshot {
            playing: true,
            tempo_bpm: 120.0,
            pos_beats: Some(0.0),
            pos_samples: None,
            time_sig_num: 4,
            time_sig_den: 4,
        },
        48_000.0,
    );
    plug.render_plugin_buffer(0, 2, &mut audio, &mut midi_out);
    assert!(midi_out.is_empty());

    // Quarter-beat crossing writes 0.8 into channel 0 at the crossing frame
    plug.render_plugin_buffer(48_000, 1, &mut audio, &mut midi_out);
    assert!(!midi_out.is_empty());
    assert!((audio[0] - 0.8).abs() < 1.0e-6);

    // InternalOrLink mode ignores host pos_beats but still honors tempo/playing
    plug.set_sync_mode(PluginSyncMode::InternalOrLink);
    plug.on_transport_update(
        &HostTransportSnapshot {
            playing: true,
            tempo_bpm: 240.0,
            pos_beats: Some(999.0),
            pos_samples: Some(0),
            time_sig_num: 3,
            time_sig_den: 4,
        },
        44_100.0,
    );
    assert!(plug.current_beat < 999.0); // pos_beats ignored outside HostTransport
}

// ---------------------------------------------------------------------------
// clock.rs — defaults, mesh propagation, guard/error returns
// ---------------------------------------------------------------------------
#[test]
fn cov_clock_link_mesh_and_error_paths() {
    // Default impl -> new(0.5, 4.0)
    let clk = LinkClock::default();
    assert_eq!(clk.source(), ClockSource::Monotonic);
    assert!((clk.snapshot().cps - 0.5).abs() < 1.0e-9);

    // set_cps error return for non-positive / non-finite inputs
    assert!(matches!(
        clk.set_cps(-0.5),
        Err(IoError::Config(ref m)) if m == "cps must be positive and finite"
    ));
    assert!(clk.set_cps(f64::NAN).is_err());
    assert!(clk.set_cps(f64::INFINITY).is_err());

    // set_beats_per_cycle error + success paths
    assert!(matches!(
        clk.set_beats_per_cycle(0.0),
        Err(IoError::Config(ref m)) if m == "beats_per_cycle must be > 0"
    ));
    clk.set_beats_per_cycle(8.0).unwrap();
    assert!((clk.snapshot().beats_per_cycle - 8.0).abs() < 1.0e-9);

    // nudge shifts the origin (moves reported cycle position backwards)
    clk.set_wall_time_secs(10.0);
    let before = clk.snapshot().cycle;
    clk.nudge_secs(1.0);
    let after = clk.snapshot().cycle;
    assert!(after < before);

    // enable_link(false) restores Monotonic source; link-ified snapshot fallback
    clk.enable_link(true).unwrap();
    assert_eq!(clk.source(), ClockSource::AbletonLink);
    // Link enabled without a mesh -> per-clock fallback branch in snapshot()
    let snap = clk.snapshot();
    assert!(snap.cycle.is_finite());
    assert_eq!(snap.link_peers, 0);
    clk.enable_link(false).unwrap();
    assert_eq!(clk.source(), ClockSource::Monotonic);

    // Shared mesh: join, wall-time pull, set_cps propagation, start/stop sync
    let mesh = LinkSessionMesh::new();
    let peer_a = LinkClock::join_mesh(Arc::clone(&mesh), 0.5);
    let peer_b = LinkClock::join_mesh(Arc::clone(&mesh), 0.5);
    assert_eq!(peer_a.source(), ClockSource::AbletonLink);
    assert_eq!(peer_a.link_num_peers(), 1);
    assert_eq!(peer_b.link_num_peers(), 1);

    // set_cps on a mesh peer propagates into LinkMeshState
    peer_a.set_wall_time_secs(4.0);
    peer_a.set_cps(0.75).unwrap();
    // wall_secs_to_cycle uses the mesh branch (origin_cycle + dt * mesh_cps)
    let cyc_b = peer_b.wall_secs_to_cycle(4.0);
    peer_b.set_wall_time_secs(4.0);
    assert!((cyc_b - peer_b.snapshot().cycle).abs() < 1.0e-9);
    assert!((peer_b.snapshot().cps - 0.75).abs() < 1.0e-9);

    // start_stop_sync propagation through the mesh on set_playing
    peer_a.set_playing(false);
    peer_b.set_wall_time_secs(4.0);
    assert!(!peer_b.snapshot().is_playing);
    peer_a.set_playing(true);
    peer_b.set_wall_time_secs(4.0);
    assert!(peer_b.snapshot().is_playing);

    // start_stop_sync disabled -> peer keeps its own is_playing flag
    peer_b.enable_start_stop_sync(false).unwrap();
    peer_a.set_playing(false);
    peer_b.set_wall_time_secs(4.0);
    assert!(peer_b.snapshot().is_playing);
    peer_a.set_playing(true);

    // cycle_to_wall_secs & wall_secs_to_cycle round-trip on the standalone clock
    clk.set_cps(0.5).unwrap();
    let w = clk.cycle_to_wall_secs(CycleTime::from_integer(4));
    assert!(w.is_finite());
    assert!((clk.wall_secs_to_cycle(w) - 4.0).abs() < 1.0e-9);
    clk.set_bpm(120.0).unwrap();
    assert!((clk.snapshot().cps - 120.0 / 60.0 / 8.0).abs() < 1.0e-9);
}

// ---------------------------------------------------------------------------
// csound.rs — OrcSource resolution, OutputSink surface, instrument control
// ---------------------------------------------------------------------------
#[test]
fn cov_csound_engine_and_orc_sources() {
    // OrcSource::resolve_url arms: Url, GithubShorthand, Inline/FilePath -> None
    let url = OrcSource::Url("https://example.com/kit.orc".to_string());
    assert_eq!(
        url.resolve_url().as_deref(),
        Some("https://example.com/kit.orc")
    );
    let gh = OrcSource::GithubShorthand("github:u/r/b/s.orc".to_string());
    assert_eq!(
        gh.resolve_url().as_deref(),
        Some("https://raw.githubusercontent.com/u/r/b/s.orc")
    );
    assert_eq!(OrcSource::Inline("instr 1".to_string()).resolve_url(), None);
    assert_eq!(
        OrcSource::FilePath("/tmp/x.orc".to_string()).resolve_url(),
        None
    );

    let mut cs = InProcessCsoundEngine::new();
    assert_eq!(OutputSink::name(&cs), "csound");
    cs.init_csound(1).unwrap();
    assert_eq!(cs.sample_rate, 8_000); // clamped minimum

    // load_orc Inline + FilePath + Url + GithubShorthand arms
    cs.load_orc(OrcSource::Inline("instr 9\nendin".to_string()))
        .unwrap();
    cs.load_orc(OrcSource::FilePath("/tmp/drum.orc".to_string()))
        .unwrap();
    cs.load_orc(url.clone()).unwrap();
    cs.load_orc(gh.clone()).unwrap();
    assert_eq!(cs.loaded_orcs.len(), 4);
    assert_eq!(cs.loaded_orcs[0], "instr 9\nendin");
    assert_eq!(cs.loaded_orcs[1], "/tmp/drum.orc");
    assert_eq!(cs.loaded_orcs[2], "https://example.com/kit.orc");

    // send_batch: explicit `csound`/`s` instrument, freq, gain + defaults
    let mut v1 = ControlMap::new();
    v1.insert("csound".to_string(), ControlValue::Str("Bd".to_string()));
    v1.insert("freq".to_string(), ControlValue::Float(220.0));
    v1.insert("gain".to_string(), ControlValue::Float(0.5));
    let ev1 = dispatch_event(v1, 1.0, 0.5, 0.5);

    let mut v2 = ControlMap::new();
    v2.insert("s".to_string(), ControlValue::Str("Sn".to_string()));
    v2.insert("freq".to_string(), ControlValue::Int(110));
    let ev2 = dispatch_event(v2, 2.0, 0.25, 0.5);

    let ev3 = dispatch_event(ControlMap::new(), 3.0, 1.0, 0.5); // all defaults

    cs.send_batch(&[ev1, ev2, ev3]).unwrap();
    let lines = cs.emitted_score_lines();
    assert!(lines.contains(&"i \"Bd\" 1.000000 0.500000 220.00 0.500".to_string()));
    assert!(lines.contains(&"i \"Sn\" 2.000000 0.250000 110.00 0.800".to_string()));
    assert!(lines.contains(&"i \"Mono\" 3.000000 1.000000 440.00 0.800".to_string()));

    // start/stop instrument + send_score_event + chnset channel set/get
    cs.start_instrument("Pad").unwrap();
    cs.stop_instrument("Pad").unwrap();
    cs.send_score_event("f 1 0 1024 10 1").unwrap();
    cs.set_control_channel("Pad.cut", 0.33).unwrap();
    assert_eq!(cs.get_control_channel("Pad.cut"), Some(0.33));
    assert_eq!(cs.get_control_channel("missing"), None);
    let lines = cs.emitted_score_lines();
    assert!(lines.contains(&"i \"Pad\" 0 -1".to_string()));
    assert!(lines.contains(&"i -\"Pad\" 0 0".to_string()));
    assert!(lines.contains(&"f 1 0 1024 10 1".to_string()));

    // panic emits the Csound end-of-score marker; close is a no-op Ok
    cs.panic().unwrap();
    assert!(cs.emitted_score_lines().contains(&"e".to_string()));
    cs.close().unwrap();
}

// ---------------------------------------------------------------------------
// crdt.rs — stage_commit ordering comparator (cycle, then epoch_id)
// ---------------------------------------------------------------------------
#[test]
fn cov_crdt_commit_epoch_sorting() {
    let mut session = CrdtEnsembleSession::new("peer_local", 5.0);

    let mk = |id: u64, target: i64| CommitEpoch {
        epoch_id: id,
        author_peer_id: "peer_remote".to_string(),
        actor_kind: ActorKind::HumanPerformer,
        target_cycle: CycleTime::from_integer(target),
        dac_latency_offset_ms: 0.0,
        stream_id: format!("d{id}"),
        code_patch: "s(\"bd\")".to_string(),
    };
    // Insert out of order; two epochs share target_cycle to hit the epoch_id tie-break
    session.stage_commit(mk(3, 8));
    session.stage_commit(mk(1, 8));
    session.stage_commit(mk(2, 4));

    assert_eq!(session.staged_epochs[0].epoch_id, 2); // cycle 4 first
    assert_eq!(session.staged_epochs[1].epoch_id, 1); // tie broken by epoch_id
    assert_eq!(session.staged_epochs[2].epoch_id, 3);

    // poll_due_epochs activates cycles <= current and keeps the rest staged
    let due = session.poll_due_epochs(CycleTime::from_integer(8));
    assert_eq!(due.len(), 3);
    assert_eq!(session.active_code_by_stream.len(), 3);
    assert!(session.staged_epochs.is_empty());

    // acoustic_activation_wall_secs subtracts local DAC latency
    let ep = mk(9, 10);
    assert!((session.acoustic_activation_wall_secs(&ep, 0.5) - (20.0 - 0.005)).abs() < 1.0e-9);
}

// ---------------------------------------------------------------------------
// midi.rs — input ingest arms, tracker drains, MPE zones, encode paths
// ---------------------------------------------------------------------------
#[test]
fn cov_midi_input_tracker_mpe_and_encoding() {
    // scale_to_u7 degenerate range arm (min == max -> direct clamp)
    let degenerate = MidiCcMapping {
        cc: 20,
        min: 64.0,
        max: 64.0,
        curve: MidiScaleCurve::Linear,
    };
    assert_eq!(InMemoryMidiMapRegistry::scale_to_u7(&degenerate, 5.0), 64);
    // In-range normalized value on a wide-range mapping
    let wide = MidiCcMapping {
        cc: 21,
        min: 0.0,
        max: 127.0,
        curve: MidiScaleCurve::Exponential(1.0),
    };
    assert_eq!(InMemoryMidiMapRegistry::scale_to_u7(&wide, 0.5), 64);

    // MidiMap registry round-trip
    let mut reg = InMemoryMidiMapRegistry::new();
    reg.register_map(MidiMap {
        name: "deck".to_string(),
        default_channel: Some(3),
        controls: BTreeMap::from([("cut".to_string(), wide.clone())]),
    });
    assert_eq!(reg.get_map("deck").unwrap().default_channel, Some(3));
    assert!(reg.get_map("nope").is_none());

    // ingest_raw_midi: short message early-return, CC, note-on, vel-0 off,
    // explicit note-off, unhandled status
    let input = InMemoryMidiInputState::new();
    input.ingest_raw_midi(&[0x90]); // len < 2 -> ignored
    input.ingest_raw_midi(&[0xB1, 74, 64]); // ch 2 CC
    assert!((input.get_cc(2, 74) - 64.0 / 127.0).abs() < 1.0e-9);
    input.ingest_raw_midi(&[0x90, 60, 100]);
    input.ingest_raw_midi(&[0x90, 64, 90]);
    assert_eq!(input.active_keys(Some(1)).len(), 2);
    input.ingest_raw_midi(&[0x90, 60, 0]); // note-on velocity 0 = off
    assert_eq!(input.active_keys(Some(1)).len(), 1);
    input.ingest_raw_midi(&[0x80, 64, 0]); // explicit note-off
    assert!(input.active_keys(Some(1)).is_empty());
    input.ingest_raw_midi(&[0xC0, 12, 0]); // program change -> ignored
    input.ingest_raw_midi(&[0xF8]); // realtime -> ignored (also len<2 for others)

    // ActiveNoteTracker: due drain + stream-scoped drain + drain_all
    let tracker = InMemoryActiveNoteTracker::new();
    let k1 = ActiveNoteKey {
        port_id: "p".to_string(),
        channel: 0,
        note: 60,
    };
    let k2 = ActiveNoteKey {
        port_id: "p".to_string(),
        channel: 1,
        note: 64,
    };
    tracker.track_note_on(k1.clone(), 2.0, &StreamId::numbered(1));
    tracker.track_note_on(k2.clone(), 9.0, &StreamId::numbered(2));
    assert_eq!(tracker.active_count(), 2);
    // Due drain removes only expired notes
    assert_eq!(tracker.drain_due_note_offs(2.5), vec![k1.clone()]);
    // Stream-scoped drain removes only that stream's notes
    assert_eq!(
        tracker.drain_stream_note_offs(&StreamId::numbered(2)),
        vec![k2.clone()]
    );
    assert_eq!(tracker.active_count(), 0);
    tracker.track_note_on(k1, 5.0, &StreamId::numbered(1));
    assert_eq!(tracker.drain_all_note_offs().len(), 1);
    assert!(tracker.drain_all_note_offs().is_empty());

    // MPE: upper-zone (master channel 16) allocation descends from channel 15
    let mut mpe = MpeChannelAllocator::new(MpeZoneConfig {
        master_channel: 16,
        member_channels: 15,
        pitch_bend_range_semitones: 48,
    });
    let (ch, msgs) = mpe.allocate_and_encode(&MpeNoteExpression {
        pitch_bend_semitones: 12.0,
        timbre_cc74: 0.5,
        pressure: 0.8,
    });
    assert_eq!(ch, 15);
    assert_eq!(msgs[0], [0xEE, 127, 79]); // pitch bend u14 = 10239
    assert_eq!(msgs[1], [0xBE, 74, 64]);
    assert_eq!(msgs[2], [0xDE, 102, 0]);
    let (ch2, _) = mpe.allocate_and_encode(&MpeNoteExpression {
        pitch_bend_semitones: -12.0,
        timbre_cc74: 0.0,
        pressure: 0.0,
    });
    assert_eq!(ch2, 14);

    // ExternalMidiClockPll converges on the injected tick rate
    let mut pll = ExternalMidiClockPll::default();
    let tick_dt = 60.0 / (100.0 * 24.0);
    for i in 0..64 {
        pll.on_clock_tick(i as f64 * tick_dt);
    }
    assert!((pll.estimated_bpm - 100.0).abs() < 2.0);

    // Steam Deck / Gamepad / DeviceMotion bridge setters
    let deck = SteamDeckControllerBridge::new();
    deck.set_axis(1, "right_y", -0.4);
    deck.set_button(1, "b", 0.6);
    deck.set_motion("az", 0.9);
    assert_eq!(deck.gamepad_axis(1, "right_y"), -0.4);
    assert_eq!(deck.gamepad_button(1, "b"), 0.6);
    assert_eq!(deck.device_motion("az"), 0.9);
    assert_eq!(deck.gamepad_axis(2, "right_y"), 0.0); // missing -> 0.0
}

#[test]
fn cov_midi_engine_encode_all_commands() {
    // Default impl uses the default_midi_out port
    let default_engine = SilverWaveMidiEngine::default();
    assert_eq!(OutputSink::name(&default_engine), "default_midi_out");
    assert!(default_engine.keep_audio);

    let midi = SilverWaveMidiEngine::new("bus_a", false);
    assert_eq!(OutputSink::name(&midi), "bus_a");

    // midicmd "clock" emits 24 PPQN F8 ticks spread across the event duration
    let clock_ev = dispatch_event(
        midi_control_map(vec![("midicmd", ControlValue::Str("clock".to_string()))]),
        1.0,
        0.5,
        0.5,
    );
    let pkts = midi.encode_dispatch_event(&clock_ev);
    assert_eq!(pkts.len(), 24);
    assert!(pkts.iter().all(|p| p.bytes == vec![0xF8]));
    assert!((pkts[1].time_secs - pkts[0].time_secs - 1.0 / 48.0).abs() < 1.0e-9);
    assert!((pkts[23].time_secs - pkts[0].time_secs - 23.0 / 48.0).abs() < 1.0e-9);

    // midicmd start/stop/continue + unknown command ignored
    for (cmd, byte) in [("start", 0xFAu8), ("stop", 0xFC), ("continue", 0xFB)] {
        let ev = dispatch_event(
            midi_control_map(vec![("midicmd", ControlValue::Str(cmd.to_string()))]),
            0.0,
            0.1,
            0.5,
        );
        let p = midi.encode_dispatch_event(&ev);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].bytes, vec![byte]);
    }
    let ev_unknown = dispatch_event(
        midi_control_map(vec![("midicmd", ControlValue::Str("rewind".to_string()))]),
        0.0,
        0.1,
        0.5,
    );
    assert!(midi.encode_dispatch_event(&ev_unknown).is_empty());

    // bankMsb + bankLsb + progNum + ccn/ccv all on channel 16 (status 0xBF/0xCF)
    let ev = dispatch_event(
        midi_control_map(vec![
            ("midichan", ControlValue::Int(16)),
            ("bankMsb", ControlValue::Int(5)),
            ("bankLsb", ControlValue::Int(7)),
            ("progNum", ControlValue::Int(10)),
            ("ccn", ControlValue::Int(74)),
            ("ccv", ControlValue::Float(0.5)),
        ]),
        2.0,
        0.25,
        0.5,
    );
    let raw: Vec<Vec<u8>> = midi
        .encode_dispatch_event(&ev)
        .into_iter()
        .map(|p| p.bytes)
        .collect();
    assert!(raw.contains(&vec![0xBF, 0, 5])); // bank select MSB
    assert!(raw.contains(&vec![0xBF, 32, 7])); // bank select LSB
    assert!(raw.contains(&vec![0xCF, 10])); // program change
    assert!(raw.contains(&vec![0xBF, 74, 64])); // ccn/ccv

    // midibend extremes: <= -1 -> 0, positive -> linear map, center -> 8192
    for (bend, expect) in [
        (-1.0f64, vec![0xE0, 0, 0]),
        (0.0, vec![0xE0, 0, 64]),
        (0.5, vec![0xE0, 127, 95]),
        (2.0, vec![0xE0, 127, 127]), // clamped at +1.0 -> 16383
    ] {
        let ev = dispatch_event(
            midi_control_map(vec![("midibend", ControlValue::Float(bend))]),
            0.0,
            0.1,
            0.5,
        );
        let p = midi.encode_dispatch_event(&ev);
        assert_eq!(p.len(), 1, "bend {bend}");
        assert_eq!(p[0].bytes, expect, "bend {bend}");
    }

    // miditouch channel aftertouch
    let ev_touch = dispatch_event(
        midi_control_map(vec![("miditouch", ControlValue::Float(0.25))]),
        0.0,
        0.1,
        0.5,
    );
    let p = midi.encode_dispatch_event(&ev_touch);
    assert_eq!(p[0].bytes, vec![0xD0, 32]);

    // Note-on retrigger emits a priority-0 NoteOff before the new NoteOn
    let note_ev = dispatch_event(
        midi_control_map(vec![
            ("note", ControlValue::Int(60)),
            ("midichan", ControlValue::Int(1)),
        ]),
        0.0,
        0.25,
        0.5,
    );
    let p1 = midi.encode_dispatch_event(&note_ev);
    assert_eq!(p1.len(), 2); // NoteOn + ScheduledNoteOff
    let p2 = midi.encode_dispatch_event(&note_ev);
    assert_eq!(p2.len(), 3); // RetriggerNoteOff + NoteOn + ScheduledNoteOff
    assert_eq!(p2[0].bytes, vec![0x80, 60, 0]);

    // send_batch aggregates + sorts packets; close() drains into CC 123/120 sweep
    midi.send_batch(std::slice::from_ref(&note_ev)).unwrap();
    midi.close().unwrap();
    let tx = midi.transmitted_bytes();
    assert!(tx.contains(&vec![0x90, 60, 114])); // vel 0.9 * 127 = 114
    assert!(tx.contains(&vec![0xB0, 123, 0]));
    assert!(tx.contains(&vec![0xBF, 120, 0]));
    assert_eq!(midi.tracker.active_count(), 0);
}

// ---------------------------------------------------------------------------
// osc.rs — wire codec, /ctrl + stream commands, interpolation, shape defaults
// ---------------------------------------------------------------------------
#[test]
fn cov_osc_wire_codec_and_message_handling() {
    // OscTime round-trip + fractional precision
    let t = OscTime::from_unix_secs(100.25);
    assert!((t.to_unix_secs() - 100.25).abs() < 1.0e-6);
    // Negative unix secs clamp to the NTP/Unix epoch boundary
    assert_eq!(OscTime::from_unix_secs(-5.0).to_unix_secs(), 0.0);

    // encode_osc_packet: message with every arg type incl. Blob padding
    let msg = OscPacket::Message(OscMessage {
        addr: "/x".to_string(),
        args: vec![
            OscType::Int(3),
            OscType::Float(0.5),
            OscType::String("s".to_string()),
            OscType::Blob(vec![1, 2, 3]),
        ],
    });
    let bytes = encode_osc_packet(&msg);
    assert_eq!(bytes.len() % 4, 0);
    assert!(bytes.windows(6).any(|w| w == b",ifsb\0"));
    // Nested bundle encoding (bundle inside bundle)
    let outer = OscPacket::Bundle(OscBundle {
        timetag: t,
        content: vec![OscPacket::Bundle(OscBundle {
            timetag: t,
            content: vec![msg.clone()],
        })],
    });
    let outer_bytes = encode_osc_packet(&outer);
    assert!(outer_bytes.starts_with(b"#bundle\0"));
    assert_eq!(outer_bytes.len() % 4, 0);

    // /ctrl receiver: String/Int keys; Float/Int/Str/Blob values; bad key ignored
    let mut state_map = BTreeMap::new();
    let mut cmds: Vec<RemoteStreamCommand> = Vec::new();
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/ctrl",
        vec![OscType::String("cF".to_string()), OscType::Float(0.5)],
    );
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/ctrl",
        vec![OscType::Int(7), OscType::Int(42)],
    );
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/ctrl",
        vec![
            OscType::String("cS".to_string()),
            OscType::String("v".to_string()),
        ],
    );
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/ctrl",
        vec![OscType::String("cB".to_string()), OscType::Blob(vec![9, 9])],
    );
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/ctrl",
        vec![OscType::Blob(vec![1]), OscType::Float(1.0)], // non-scalar key -> ignored
    );
    assert_eq!(state_map.get("cF"), Some(&ControlValue::Float(0.5)));
    assert_eq!(state_map.get("7"), Some(&ControlValue::Int(42)));
    assert_eq!(
        state_map.get("cS"),
        Some(&ControlValue::Str("v".to_string()))
    );
    assert_eq!(state_map.get("cB"), Some(&ControlValue::Bytes(vec![9, 9])));
    assert_eq!(state_map.len(), 4);

    // Remote stream commands: mute/unmute/solo/unsolo by Int + String id,
    // muteAll/unmuteAll/hush, blob arg rejected, unknown addr ignored
    handle_osc_msg(&mut state_map, &mut cmds, "/mute", vec![OscType::Int(3)]);
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/unmute",
        vec![OscType::String("d2".to_string())],
    );
    handle_osc_msg(&mut state_map, &mut cmds, "/solo", vec![OscType::Int(5)]);
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/unsolo",
        vec![OscType::String("lead".to_string())],
    );
    handle_osc_msg(&mut state_map, &mut cmds, "/muteAll", vec![]);
    handle_osc_msg(&mut state_map, &mut cmds, "/unmuteAll", vec![]);
    handle_osc_msg(&mut state_map, &mut cmds, "/hush", vec![]);
    handle_osc_msg(
        &mut state_map,
        &mut cmds,
        "/mute",
        vec![OscType::Blob(vec![1])],
    ); // rejected arg type
    handle_osc_msg(&mut state_map, &mut cmds, "/mute", vec![]); // missing arg -> guard fails
    handle_osc_msg(&mut state_map, &mut cmds, "/bogus", vec![]); // unknown address
    assert_eq!(
        cmds,
        vec![
            RemoteStreamCommand::Mute(StreamId("3".to_string())),
            RemoteStreamCommand::Unmute(StreamId("d2".to_string())),
            RemoteStreamCommand::Solo(StreamId("5".to_string())),
            RemoteStreamCommand::Unsolo(StreamId("lead".to_string())),
            RemoteStreamCommand::MuteAll,
            RemoteStreamCommand::UnmuteAll,
            RemoteStreamCommand::Hush,
        ]
    );
}

#[test]
fn cov_osc_engine_shapes_and_scheduling() {
    let mut engine = SilverWaveOscEngine::new();
    assert_eq!(OutputSink::name(&engine), "osc");
    let target = OscTarget::superdirt_default();
    engine
        .add_target(target.clone(), vec![OscShape::superdirt()])
        .unwrap();

    // NamedPairs: orbit/cps/cycle/delta keys in `value` are not duplicated
    let mut v = ControlMap::new();
    v.insert("s".to_string(), ControlValue::Str("bd".to_string()));
    v.insert("orbit".to_string(), ControlValue::Int(2));
    v.insert("n".to_string(), ControlValue::Int(4));
    v.insert("flag".to_string(), ControlValue::Bool(true));
    v.insert("raw".to_string(), ControlValue::Bytes(vec![0xAB]));
    v.insert(
        "note".to_string(),
        ControlValue::Note(MidiNoteValue {
            midi_note: 61,
            detune_cents: 0.0,
        }),
    );
    let ev = dispatch_event(v, 10.0, 0.5, 0.5);
    let pkt = SilverWaveOscEngine::encode_event(&target, &OscShape::superdirt(), &ev)
        .unwrap()
        .unwrap();
    let OscPacket::Bundle(bundle) = pkt else {
        panic!("expected bundle")
    };
    let OscPacket::Message(m) = &bundle.content[0] else {
        panic!("expected message")
    };
    let orbit_count = m
        .args
        .iter()
        .filter(|a| **a == OscType::String("orbit".to_string()))
        .count();
    assert_eq!(orbit_count, 1);
    assert!(m.args.contains(&OscType::Int(2))); // orbit value
    assert!(m.args.contains(&OscType::Int(1))); // Bool -> Int(1)
    assert!(m.args.contains(&OscType::Blob(vec![0xAB])));
    assert!(m.args.contains(&OscType::Float(61.0))); // Note -> midi float

    // send_batch records packets through the engine
    engine.send_batch(std::slice::from_ref(&ev)).unwrap();
    assert_eq!(engine.sent_packets.lock().unwrap().len(), 1);
    // panic emits a /hush message; close is Ok
    engine.panic().unwrap();
    let OscPacket::Message(hush) = &engine.sent_packets.lock().unwrap()[1] else {
        panic!("expected message")
    };
    assert_eq!(hush.addr, "/hush");
    engine.close().unwrap();

    // NamedPairs required_keys gate: missing key -> Ok(None)
    let req_shape = OscShape {
        address_pattern: "/dirt/play".to_string(),
        payload_mode: OscPayloadMode::NamedPairs {
            required_keys: vec!["s".to_string()],
        },
    };
    let mut bare = ControlMap::new();
    bare.insert("gain".to_string(), ControlValue::Float(1.0));
    let ev_bare = dispatch_event(bare, 0.0, 0.5, 0.5);
    assert!(
        SilverWaveOscEngine::encode_event(&target, &req_shape, &ev_bare)
            .unwrap()
            .is_none()
    );

    // ArgList defaults: every OscArgDefault arm incl. Required -> None
    let arglist_shape = OscShape {
        address_pattern: "/rig/{lamp}".to_string(),
        payload_mode: OscPayloadMode::ArgList(vec![
            OscArgSpec {
                param_name: "lamp".to_string(),
                default: OscArgDefault::Str("wash".to_string()),
            },
            OscArgSpec {
                param_name: "level".to_string(),
                default: OscArgDefault::Float(0.75),
            },
            OscArgSpec {
                param_name: "count".to_string(),
                default: OscArgDefault::Int(3),
            },
            OscArgSpec {
                param_name: "on".to_string(),
                default: OscArgDefault::Bool(true),
            },
            OscArgSpec {
                param_name: "payload".to_string(),
                default: OscArgDefault::Blob(vec![0xCA, 0xFE]),
            },
            OscArgSpec {
                param_name: "must".to_string(),
                default: OscArgDefault::Required,
            },
        ]),
    };
    let mut v_arg = ControlMap::new();
    v_arg.insert("must".to_string(), ControlValue::Float(9.0));
    let ev_arg = dispatch_event(v_arg, 0.0, 0.5, 0.5);
    // `lamp` missing -> interpolated default? No: defaults only fill payload
    // args; the address interpolation still needs `{lamp}`.
    assert!(
        SilverWaveOscEngine::encode_event(&target, &arglist_shape, &ev_arg)
            .unwrap()
            .is_none()
    );

    let mut v_arg2 = ev_arg.value.clone();
    v_arg2.insert("lamp".to_string(), ControlValue::Str("spot".to_string()));
    let ev_arg2 = dispatch_event(v_arg2, 0.0, 0.5, 0.5);
    let pkt = SilverWaveOscEngine::encode_event(&target, &arglist_shape, &ev_arg2)
        .unwrap()
        .unwrap();
    let OscPacket::Bundle(b) = pkt else {
        panic!("expected bundle")
    };
    let OscPacket::Message(m) = &b.content[0] else {
        panic!("expected message")
    };
    assert_eq!(m.addr, "/rig/spot");
    assert_eq!(
        m.args,
        vec![
            OscType::String("spot".to_string()), // `lamp` supplied -> beats Str default
            OscType::Float(0.75),                // Float default
            OscType::Int(3),                     // Int default
            OscType::Int(1),                     // Bool(true) default -> Int(1)
            OscType::Blob(vec![0xCA, 0xFE]),     // Blob default
            OscType::Float(9.0),                 // `must` supplied
        ]
    );

    // ArgList Required missing entirely -> suppressed packet
    let mut v_req = ControlMap::new();
    v_req.insert("lamp".to_string(), ControlValue::Str("spot".to_string()));
    let ev_req = dispatch_event(v_req, 0.0, 0.5, 0.5);
    assert!(
        SilverWaveOscEngine::encode_event(&target, &arglist_shape, &ev_req)
            .unwrap()
            .is_none()
    );

    // Address interpolation over Int + Float values; non-scalar -> None
    let interp_shape = OscShape {
        address_pattern: "/ch/{chan}/v/{val}".to_string(),
        payload_mode: OscPayloadMode::ArgList(vec![]),
    };
    let mut v_int = ControlMap::new();
    v_int.insert("chan".to_string(), ControlValue::Int(4));
    v_int.insert("val".to_string(), ControlValue::Float(1.5));
    let ev_i = dispatch_event(v_int, 0.0, 0.5, 0.5);
    let mut live = target.clone();
    live.schedule = OscScheduleMode::Live;
    let pkt = SilverWaveOscEngine::encode_event(&live, &interp_shape, &ev_i)
        .unwrap()
        .unwrap();
    let OscPacket::Message(m) = pkt else {
        panic!("expected message")
    };
    assert_eq!(m.addr, "/ch/4/v/1.5");

    let mut v_bad = ControlMap::new();
    v_bad.insert("chan".to_string(), ControlValue::Bytes(vec![1]));
    v_bad.insert("val".to_string(), ControlValue::Float(1.5));
    let ev_bad = dispatch_event(v_bad, 0.0, 0.5, 0.5);
    assert!(
        SilverWaveOscEngine::encode_event(&live, &interp_shape, &ev_bad)
            .unwrap()
            .is_none()
    );

    // PreMessageStamp prepends (sec, usec) integer args and returns a bare message
    let mut stamp = target.clone();
    stamp.schedule = OscScheduleMode::PreMessageStamp;
    let pkt = SilverWaveOscEngine::encode_event(&stamp, &OscShape::superdirt(), &ev)
        .unwrap()
        .unwrap();
    let OscPacket::Message(m) = pkt else {
        panic!("expected message")
    };
    assert_eq!(m.args[0], OscType::Int(10)); // onset 10.0 + latency 0.2 -> sec 10
    assert_eq!(m.args[1], OscType::Int(200_000));
    assert_eq!(m.args[2], OscType::String("cps".to_string()));

    // PreBundleStamp produces an NTP-stamped bundle
    let pkt = SilverWaveOscEngine::encode_event(&target, &OscShape::superdirt(), &ev)
        .unwrap()
        .unwrap();
    let OscPacket::Bundle(b) = pkt else {
        panic!("expected bundle")
    };
    assert!((b.timetag.to_unix_secs() - 10.2).abs() < 1.0e-5);
}

// ---------------------------------------------------------------------------
// streams.rs — registry lifecycle, filters, history truncation, transitions
// ---------------------------------------------------------------------------
#[test]
fn cov_streams_registry_lifecycle() {
    let reg = SilverWaveStreamRegistry::new();

    // combined_audible_pattern over an empty registry -> silence
    let empty = reg.combined_audible_pattern();
    assert!(query_transition_arc(&empty, CycleTime::zero(), CycleTime::one()).is_empty());

    // Sink registration + panic fan-out into the Csound score log
    let cs = Arc::new(InProcessCsoundEngine::new());
    reg.add_sink(cs.clone());

    let d1 = StreamId::numbered(1);
    let d2 = StreamId::numbered(2);
    reg.set_pattern(&d1, const_pat("bd", 100.0));
    reg.set_pattern(&d2, const_pat("sd", 16000.0));

    // mute -> skipped in query_audible_events; unmute restores
    reg.mute(&d1);
    let evs = reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5);
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].stream_id, d2);
    reg.unmute(&d1);
    assert_eq!(
        reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5)
            .len(),
        2
    );

    // solo filter: only the soloed stream emits
    reg.solo(&d1);
    let evs = reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5);
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].stream_id, d1);
    reg.unsolo(&d1);
    assert_eq!(
        reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5)
            .len(),
        2
    );

    // mute_all / unmute_all
    reg.mute_all();
    assert!(
        reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5)
            .is_empty()
    );
    reg.unmute_all();
    assert_eq!(
        reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5)
            .len(),
        2
    );

    // Fractional query window: haps whose `part` doesn't contain `whole.begin`
    // are filtered out by the `has_onset` guard
    let frac =
        reg.query_audible_events(CycleTime::new_const(1, 2), CycleTime::from_integer(2), 0.5);
    // Only the cycle-1 haps have true onsets; the partial cycle-0 haps are dropped
    assert_eq!(frac.len(), 2);

    // Value::Int / Value::Rational / non-mappable (_ arm) conversion into ControlMap
    let mut m = CoreControlMap::new();
    m.insert("s".to_string(), Value::String("hh".to_string()));
    m.insert("intv".to_string(), Value::Int(5));
    m.insert("ratv".to_string(), Value::Rational(Time::new_const(1, 2)));
    m.insert("boolv".to_string(), Value::Bool(true));
    let d3 = StreamId::numbered(3);
    reg.set_pattern(&d3, pure(m));
    let evs = reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5);
    let ev3 = evs.iter().find(|e| e.stream_id == d3).unwrap();
    assert_eq!(ev3.value.get("intv"), Some(&ControlValue::Int(5)));
    assert_eq!(ev3.value.get("ratv"), Some(&ControlValue::Float(0.5)));
    assert!(!ev3.value.contains_key("boolv"));

    // silence() removes the slot entirely
    reg.silence(&d3);
    assert!(!reg.active_streams().contains(&d3));

    // history truncation: > 16 patterns keep only the newest 16
    for _ in 0..20 {
        reg.set_pattern(&d1, const_pat("bd", 100.0));
    }
    assert!(reg.active_streams().contains(&d1));

    // apply_transition inserts a slot when the stream doesn't exist yet
    let d9 = StreamId::numbered(9);
    reg.apply_transition(
        &d9,
        TransitionSpec::Jump,
        const_pat("cp", 200.0),
        CycleTime::zero(),
    )
    .unwrap();
    assert!(reg.active_streams().contains(&d9));
    // Repeated transitions exercise the history truncation path
    for _ in 0..20 {
        reg.apply_transition(
            &d9,
            TransitionSpec::Jump,
            const_pat("cp", 200.0),
            CycleTime::zero(),
        )
        .unwrap();
    }

    // hush clears all slots AND panics registered sinks
    reg.hush().unwrap();
    assert!(reg.active_streams().is_empty());
    assert!(cs.emitted_score_lines().contains(&"e".to_string()));

    // registry-level panic still works on an empty registry
    reg.panic().unwrap();
    assert_eq!(
        cs.emitted_score_lines()
            .iter()
            .filter(|l| *l == "e")
            .count(),
        2
    );
}

// ---------------------------------------------------------------------------
// transitions.rs — fade primitives
// ---------------------------------------------------------------------------
#[test]
fn cov_transitions_fade_primitives() {
    let pat = const_pat("bd", 100.0);

    // fade_in delegates to fade_in_from(start = 0)
    let fin = <ControlPattern as FadePrimitives<ControlPattern>>::fade_in(
        CycleTime::from_integer(4),
        pat.clone(),
    );
    let haps = query_transition_arc(&fin, CycleTime::zero(), CycleTime::from_integer(5));
    // t = 0 -> gain 0 (filtered), t = 4 -> gain 1.0
    assert_eq!(haps.len(), 4);
    let g = |h: &silverwave_core::Hap<CoreControlMap>| {
        h.value.get("gain").and_then(Value::as_f64).unwrap()
    };
    // haps in the window are t = 1, 2, 3 (lerped) and t = 4 (full)
    let at2 = haps
        .iter()
        .find(|h| h.whole_or_part().begin == CycleTime::from_integer(2))
        .unwrap();
    assert!((g(at2) - 0.5).abs() < 1.0e-9);
    let at4 = haps
        .iter()
        .find(|h| h.whole_or_part().begin == CycleTime::from_integer(4))
        .unwrap();
    assert!((g(at4) - 1.0).abs() < 1.0e-9);

    // fade_in_from with non-zero start + zero cycles -> pattern returned as-is
    let same = <ControlPattern as FadePrimitives<ControlPattern>>::fade_in_from(
        CycleTime::from_integer(2),
        CycleTime::zero(),
        pat.clone(),
    );
    assert_eq!(
        query_transition_arc(&same, CycleTime::zero(), CycleTime::one()).len(),
        1
    );
    // Non-zero start: haps before `start` are filtered out entirely
    let fin2 = <ControlPattern as FadePrimitives<ControlPattern>>::fade_in_from(
        CycleTime::from_integer(1),
        CycleTime::from_integer(2),
        pat.clone(),
    );
    let haps = query_transition_arc(&fin2, CycleTime::zero(), CycleTime::from_integer(4));
    assert!(
        haps.iter()
            .all(|h| h.whole_or_part().begin >= CycleTime::from_integer(2))
    );

    // fade_out delegates to fade_out_from(start = 0): haps ramp to silence
    let fout = <ControlPattern as FadePrimitives<ControlPattern>>::fade_out(
        CycleTime::from_integer(4),
        pat.clone(),
    );
    let haps = query_transition_arc(&fout, CycleTime::zero(), CycleTime::from_integer(6));
    assert_eq!(haps.len(), 4); // t = 0..3 kept, t >= 4 dropped
    let at0 = haps
        .iter()
        .find(|h| h.whole_or_part().begin == CycleTime::zero())
        .unwrap();
    assert!((g(at0) - 1.0).abs() < 1.0e-9);
    let at3 = haps
        .iter()
        .find(|h| h.whole_or_part().begin == CycleTime::from_integer(3))
        .unwrap();
    assert!((g(at3) - 0.25).abs() < 1.0e-9);

    // fade_out_from with zero cycles -> silence()
    let sil = <ControlPattern as FadePrimitives<ControlPattern>>::fade_out_from(
        CycleTime::zero(),
        CycleTime::zero(),
        pat.clone(),
    );
    assert!(query_transition_arc(&sil, CycleTime::zero(), CycleTime::one()).is_empty());
}

// ---------------------------------------------------------------------------
// transitions.rs — every TransitionSpec variant through compile + query
// ---------------------------------------------------------------------------
#[test]
fn cov_transitions_all_specs() {
    let prev = const_pat("bd", 100.0);
    let next = const_pat("sd", 16000.0);
    let hist = std::slice::from_ref(&prev);
    let s_of = |h: &silverwave_core::Hap<CoreControlMap>| h.value.get("s").cloned();
    let sd = || Some(Value::String("sd".to_string()));
    let bd = || Some(Value::String("bd".to_string()));

    // Jump -> next immediately
    let p = compile_transition_pattern(&TransitionSpec::Jump, hist, &next, CycleTime::zero());
    assert_eq!(
        s_of(&query_transition_arc(&p, CycleTime::zero(), CycleTime::one())[0]),
        sd()
    );

    // XfadeIn: prev fades out across the window, next fades in
    let p = compile_transition_pattern(
        &TransitionSpec::XfadeIn {
            cycles: CycleTime::from_integer(4),
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    let mid = query_transition_arc(&p, CycleTime::one(), CycleTime::from_integer(2));
    assert_eq!(mid.len(), 2); // both layers audible mid-fade
    let after = query_transition_arc(&p, CycleTime::from_integer(4), CycleTime::from_integer(5));
    assert_eq!(after.len(), 1);
    assert_eq!(s_of(&after[0]), sd());

    // ClutchIn: prev persists before eval, probabilistic takeover inside window
    let p = compile_transition_pattern(
        &TransitionSpec::ClutchIn {
            cycles: CycleTime::from_integer(8),
        },
        hist,
        &next,
        CycleTime::from_integer(4),
    );
    let before = query_transition_arc(&p, CycleTime::from_integer(2), CycleTime::from_integer(4));
    assert!(before.iter().all(|h| s_of(h) == bd()));
    let after = query_transition_arc(&p, CycleTime::from_integer(12), CycleTime::from_integer(13));
    assert!(after.iter().all(|h| s_of(h) == sd()));
    let window = query_transition_arc(&p, CycleTime::from_integer(4), CycleTime::from_integer(12));
    assert!(!window.is_empty()); // transition window emits haps

    // AnticipateIn: prev haps get a rising `hpf` sweep, next starts at switch_at
    let p = compile_transition_pattern(
        &TransitionSpec::AnticipateIn {
            cycles: CycleTime::from_integer(4),
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    let haps = query_transition_arc(&p, CycleTime::from_integer(2), CycleTime::from_integer(3));
    assert_eq!(haps.len(), 1);
    let hpf = haps[0].value.get("hpf").and_then(Value::as_f64).unwrap();
    assert!((hpf - 1100.0).abs() < 1.0e-6); // 100 + 2000 * 0.5
    let after = query_transition_arc(&p, CycleTime::from_integer(4), CycleTime::from_integer(5));
    assert_eq!(s_of(&after[0]), sd());

    // InterpolateIn: numeric params morph across the window
    let p = compile_transition_pattern(
        &TransitionSpec::InterpolateIn {
            cycles: CycleTime::from_integer(4),
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    let at0 = query_transition_arc(&p, CycleTime::zero(), CycleTime::one());
    assert_eq!(
        at0[0].value.get("cutoff"),
        Some(&Value::Float(100.0)) // alpha = 0 -> pure prev value
    );
    let at2 = query_transition_arc(&p, CycleTime::from_integer(2), CycleTime::from_integer(3));
    assert_eq!(at2[0].value.get("cutoff"), Some(&Value::Float(8050.0)));
    let at5 = query_transition_arc(&p, CycleTime::from_integer(5), CycleTime::from_integer(6));
    assert_eq!(at5[0].value.get("cutoff"), Some(&Value::Float(16000.0)));
    // Empty history -> prev_haps.first() is None, values pass through unmerged
    let p_no_prev = compile_transition_pattern(
        &TransitionSpec::InterpolateIn {
            cycles: CycleTime::from_integer(4),
        },
        &[],
        &next,
        CycleTime::zero(),
    );
    let h = query_transition_arc(&p_no_prev, CycleTime::one(), CycleTime::from_integer(2));
    assert_eq!(h[0].value.get("cutoff"), Some(&Value::Float(16000.0)));

    // JumpIn / JumpInPrime: unaligned eval vs ceil-aligned switch point
    let eval = CycleTime::new_const(17, 5); // 3.4
    let p_jump =
        compile_transition_pattern(&TransitionSpec::JumpIn { cycles: 2 }, hist, &next, eval);
    let p_prime = compile_transition_pattern(
        &TransitionSpec::JumpInPrime { cycles: 2 },
        hist,
        &next,
        eval,
    );
    // jumpIn switches at 5.4; jumpIn' waits until 6.0
    assert_eq!(
        s_of(
            &query_transition_arc(
                &p_prime,
                CycleTime::from_integer(5),
                CycleTime::from_integer(6)
            )[0]
        ),
        bd()
    );
    let late = query_transition_arc(
        &p_jump,
        CycleTime::from_integer(6),
        CycleTime::from_integer(7),
    );
    assert_eq!(s_of(&late[0]), sd());
    // Negative cycles clamp through from_integer fine as well
    let p_jn = compile_transition_pattern(
        &TransitionSpec::JumpIn { cycles: -1 },
        hist,
        &next,
        CycleTime::from_integer(3),
    );
    assert_eq!(
        s_of(
            &query_transition_arc(
                &p_jn,
                CycleTime::from_integer(3),
                CycleTime::from_integer(4)
            )[0]
        ),
        sd()
    );

    // JumpMod: eval exactly on an integer boundary exercises `cand += 1`
    let p_mod = compile_transition_pattern(
        &TransitionSpec::JumpMod { cycle_mod: 4 },
        hist,
        &next,
        CycleTime::from_integer(5),
    );
    assert_eq!(
        s_of(
            &query_transition_arc(
                &p_mod,
                CycleTime::from_integer(7),
                CycleTime::from_integer(8)
            )[0]
        ),
        bd()
    );
    assert_eq!(
        s_of(
            &query_transition_arc(
                &p_mod,
                CycleTime::from_integer(8),
                CycleTime::from_integer(9)
            )[0]
        ),
        sd()
    );

    // Histpan: stereo-spread stack of history + next; count=1 -> center pan
    let p_pan = compile_transition_pattern(
        &TransitionSpec::Histpan { count: 4 },
        &[prev.clone(), prev.clone(), prev.clone()],
        &next,
        CycleTime::zero(),
    );
    let haps = query_transition_arc(&p_pan, CycleTime::zero(), CycleTime::one());
    assert_eq!(haps.len(), 4);
    assert_eq!(haps[0].value.get("pan"), Some(&Value::Float(0.0)));
    assert_eq!(haps[3].value.get("pan"), Some(&Value::Float(1.0)));
    let p_pan1 = compile_transition_pattern(
        &TransitionSpec::Histpan { count: 1 },
        hist,
        &next,
        CycleTime::zero(),
    );
    let haps = query_transition_arc(&p_pan1, CycleTime::zero(), CycleTime::one());
    assert_eq!(haps.len(), 1);
    assert_eq!(haps[0].value.get("pan"), Some(&Value::Float(0.5)));

    // Wait: silence until resume cycle, then next plays
    let p_wait = compile_transition_pattern(
        &TransitionSpec::Wait {
            cycles: CycleTime::from_integer(4),
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    assert!(
        query_transition_arc(&p_wait, CycleTime::zero(), CycleTime::from_integer(4)).is_empty()
    );
    assert_eq!(
        s_of(
            &query_transition_arc(
                &p_wait,
                CycleTime::from_integer(4),
                CycleTime::from_integer(5)
            )[0]
        ),
        sd()
    );

    // WaitT: silence until resume, then the INNER transition engages at resume
    let p_waitt = compile_transition_pattern(
        &TransitionSpec::WaitT {
            cycles: CycleTime::from_integer(2),
            inner: Box::new(TransitionSpec::JumpIn { cycles: 2 }),
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    assert!(
        query_transition_arc(&p_waitt, CycleTime::zero(), CycleTime::from_integer(2)).is_empty()
    );
    // Inner jumpIn(2) evaluated at resume (cycle 2) switches at cycle 4
    assert_eq!(
        s_of(
            &query_transition_arc(
                &p_waitt,
                CycleTime::from_integer(3),
                CycleTime::from_integer(4)
            )[0]
        ),
        bd()
    );
    assert_eq!(
        s_of(
            &query_transition_arc(
                &p_waitt,
                CycleTime::from_integer(4),
                CycleTime::from_integer(5)
            )[0]
        ),
        sd()
    );

    // Wash: prev plays until t1, washed prev+next inside [t1, t2), clean next after
    let p_wash = compile_transition_pattern(
        &TransitionSpec::Wash {
            delay: CycleTime::from_integer(1),
            duration: CycleTime::from_integer(2),
            out_transform: wash_out_transform,
            in_transform: wash_in_transform,
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    let h0 = query_transition_arc(&p_wash, CycleTime::zero(), CycleTime::one());
    assert_eq!(h0.len(), 1);
    assert!(!h0[0].value.contains_key("wash_out")); // unmodified prev before t1
    let h_mid = query_transition_arc(&p_wash, CycleTime::one(), CycleTime::from_integer(3));
    assert!(
        h_mid
            .iter()
            .any(|h| h.value.contains_key("wash_out") && s_of(h) == bd())
    );
    assert!(
        h_mid
            .iter()
            .any(|h| h.value.contains_key("wash_in") && s_of(h) == sd())
    );
    let h_late = query_transition_arc(
        &p_wash,
        CycleTime::from_integer(3),
        CycleTime::from_integer(4),
    );
    assert_eq!(h_late.len(), 1);
    assert_eq!(s_of(&h_late[0]), sd());
    assert!(!h_late[0].value.contains_key("wash_in")); // clean next after t2

    // WashIn: next plays clean outside [t1, t2), washed inside
    let p_washin = compile_transition_pattern(
        &TransitionSpec::WashIn {
            delay: CycleTime::from_integer(1),
            duration: CycleTime::from_integer(2),
            transform: wash_in_transform,
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    let h0 = query_transition_arc(&p_washin, CycleTime::zero(), CycleTime::one());
    assert_eq!(h0.len(), 1);
    assert!(!h0[0].value.contains_key("wash_in"));
    let h_mid = query_transition_arc(&p_washin, CycleTime::one(), CycleTime::from_integer(2));
    assert_eq!(h_mid.len(), 1);
    assert!(h_mid[0].value.contains_key("wash_in"));
    let h_late = query_transition_arc(
        &p_washin,
        CycleTime::from_integer(3),
        CycleTime::from_integer(4),
    );
    assert!(!h_late[0].value.contains_key("wash_in"));

    // Mortal: full volume until lifespan, gain ramp-down across release, then gone
    let p_mortal = compile_transition_pattern(
        &TransitionSpec::Mortal {
            lifespan: CycleTime::from_integer(4),
            release: CycleTime::from_integer(2),
        },
        hist,
        &next,
        CycleTime::zero(),
    );
    let early = query_transition_arc(&p_mortal, CycleTime::zero(), CycleTime::one());
    assert_eq!(early.len(), 1);
    let releasing = query_transition_arc(
        &p_mortal,
        CycleTime::from_integer(5),
        CycleTime::from_integer(6),
    );
    assert_eq!(releasing.len(), 1);
    let gain = releasing[0]
        .value
        .get("gain")
        .and_then(Value::as_f64)
        .unwrap();
    assert!((gain - 0.5).abs() < 1.0e-9); // 1 - (5 - 4) / 2
    assert!(
        query_transition_arc(
            &p_mortal,
            CycleTime::from_integer(6),
            CycleTime::from_integer(7)
        )
        .is_empty()
    );
}

// ---------------------------------------------------------------------------
// backends.rs — SPSC `try_lock` contention, `process_block` index clamp, and
// the WebAudio `posted` lock-fail arm (public mutex field, poisonable)
// ---------------------------------------------------------------------------
#[test]
fn cov_backends_contention_clamp_and_poisoned_posted() {
    // `process_block` called directly with an event whose clamped frame index
    // still lands past `output.len()` hits the `idx * ch + c < output.len()`
    // guard's false arm (unreachable through `render_next_block`, which caps
    // `frames` at `output.len() / channels`).
    let mut engine = ImpulseAudioRenderEngine::default();
    engine.prepare(AudioFormat::default());
    engine.push_event(AudioThreadEvent {
        frame_offset: 7,
        duration_frames: 1,
        orbit: 0,
        controls: ControlMap::new(),
    });
    let mut tiny = [0.0f32; 2];
    AudioRenderEngine::process_block(&mut engine, &mut tiny, 4, 2);
    assert!(tiny.iter().all(|s| *s == 0.0));

    // `try_lock` contention: clones of the same `SpscAudioEventProducer` racing
    // `try_push` hit the "audio SPSC producer contended" error arm.
    let cfg = CpalConfig {
        ring_capacity_events: 64,
        ..CpalConfig::default()
    };
    let cpal =
        MemoryCpalBackend::start(cfg, Box::new(ImpulseAudioRenderEngine::default())).unwrap();
    let sender = cpal.event_sender();
    let contended = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(std::sync::Barrier::new(5));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let s = sender.clone();
        let n = Arc::clone(&contended);
        let d = Arc::clone(&done);
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            b.wait();
            for _ in 0..2_000_000 {
                if d.load(Ordering::Relaxed) {
                    break;
                }
                let res = s.try_push(AudioThreadEvent {
                    frame_offset: 0,
                    duration_frames: 1,
                    orbit: 0,
                    controls: ControlMap::new(),
                });
                if let Err(IoError::Audio(ref msg)) = res
                    && msg == "audio SPSC producer contended"
                {
                    n.fetch_add(1, Ordering::Relaxed);
                    d.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }));
    }
    barrier.wait();
    for h in handles {
        h.join().unwrap();
    }
    assert!(
        contended.load(Ordering::Relaxed) > 0,
        "expected at least one try_lock contention across 4 racing producers"
    );
    cpal.stop().unwrap();

    // `posted` is a public `Mutex` field — poison it to exercise the lock-fail
    // arm of `post_event_batch` (events silently dropped, still `Ok`).
    let web = MemoryWebAudioBackend::initialize(WebAudioConfig::default()).unwrap();
    let res = catch_unwind(AssertUnwindSafe(|| {
        let _guard = web.posted.lock().unwrap();
        panic!("intentional panic to poison the posted log");
    }));
    assert!(res.is_err());
    assert!(web.posted.is_poisoned());
    web.post_event_batch(&[AudioThreadEvent {
        frame_offset: 0,
        duration_frames: 1,
        orbit: 0,
        controls: ControlMap::new(),
    }])
    .unwrap();
}

// ---------------------------------------------------------------------------
// csound.rs — `score_log`/`channels` are public mutexes: poison both to reach
// the `if let Ok` lock-fail arms in `send_score_event`/`set_control_channel`
// ---------------------------------------------------------------------------
#[test]
fn cov_csound_poisoned_log_and_channel_locks() {
    let cs = InProcessCsoundEngine::new();

    let res = catch_unwind(AssertUnwindSafe(|| {
        let _g = cs.score_log.lock().unwrap();
        panic!("intentional panic to poison score_log");
    }));
    assert!(res.is_err());
    assert!(cs.score_log.is_poisoned());
    cs.send_score_event("i \"x\" 0 1").unwrap(); // lock-fail: line dropped
    assert!(cs.emitted_score_lines().is_empty()); // unwrap_or_default fallback

    let res = catch_unwind(AssertUnwindSafe(|| {
        let _g = cs.channels.lock().unwrap();
        panic!("intentional panic to poison channels");
    }));
    assert!(res.is_err());
    assert!(cs.channels.is_poisoned());
    cs.set_control_channel("Pad.cut", 0.5).unwrap(); // lock-fail: insert dropped
    assert_eq!(cs.get_control_channel("Pad.cut"), None);
}

// ---------------------------------------------------------------------------
// midi.rs — `transmitted` is a public mutex: poison it to reach the `if let Ok`
// lock-fail arms in `send_batch` (silent drop) and `panic` (no CC 123/120 sweep)
// ---------------------------------------------------------------------------
#[test]
fn cov_midi_poisoned_transmitted_log() {
    let midi = SilverWaveMidiEngine::new("bus_poison", true);
    let res = catch_unwind(AssertUnwindSafe(|| {
        let _g = midi.transmitted.lock().unwrap();
        panic!("intentional panic to poison transmitted");
    }));
    assert!(res.is_err());
    assert!(midi.transmitted.is_poisoned());

    let ev = dispatch_event(
        midi_control_map(vec![("note", ControlValue::Int(60))]),
        0.0,
        0.25,
        0.5,
    );
    midi.send_batch(std::slice::from_ref(&ev)).unwrap(); // lock-fail arm
    midi.panic().unwrap(); // lock-fail arm: no note-off sweep appended
    assert!(midi.transmitted_bytes().is_empty()); // unwrap_or_default fallback
    // Active notes were still drained by panic() before the lock attempt.
    assert_eq!(midi.tracker.active_count(), 0);
}

// ---------------------------------------------------------------------------
// osc.rs — rejected remote-command arg types, required-keys satisfied arm,
// suppressed-packet skip inside `send_batch`, `OscArgDefault::Str` fallback,
// and the `sent_packets` lock-fail/poisoned-log arms
// ---------------------------------------------------------------------------
#[test]
fn cov_osc_rejected_args_suppression_and_poisoned_log() {
    // /unmute, /solo, /unsolo with non-scalar args -> `osc_arg_to_stream_id`
    // returns None -> `if let Some(id)` false arms.
    let mut state_map = BTreeMap::new();
    let mut cmds: Vec<RemoteStreamCommand> = Vec::new();
    for addr in ["/unmute", "/solo", "/unsolo"] {
        handle_osc_msg(
            &mut state_map,
            &mut cmds,
            addr,
            vec![OscType::Blob(vec![1])],
        );
        handle_osc_msg(&mut state_map, &mut cmds, addr, vec![]); // empty-args guard
        handle_osc_msg(&mut state_map, &mut cmds, addr, vec![OscType::Float(0.5)]); // scalar-but-wrong-type arg
    }
    assert!(cmds.is_empty());

    // ArgList `OscArgDefault::Str` fallback arm (param absent from the event).
    let target = OscTarget::superdirt_default();
    let str_default = OscShape {
        address_pattern: "/rig".to_string(),
        payload_mode: OscPayloadMode::ArgList(vec![OscArgSpec {
            param_name: "lamp".to_string(),
            default: OscArgDefault::Str("wash".to_string()),
        }]),
    };
    let bare = dispatch_event(ControlMap::new(), 0.0, 0.5, 0.5);
    let pkt = SilverWaveOscEngine::encode_event(&target, &str_default, &bare)
        .unwrap()
        .unwrap();
    let OscPacket::Bundle(b) = pkt else {
        panic!("expected bundle")
    };
    let OscPacket::Message(m) = &b.content[0] else {
        panic!("expected message")
    };
    assert_eq!(m.args, vec![OscType::String("wash".to_string())]);

    // NamedPairs `required_keys` present in the event -> `if !contains_key`
    // false arm proceeds into mandatory-field emission.
    let mut engine = SilverWaveOscEngine::new();
    let req_shape = OscShape {
        address_pattern: "/dirt/play".to_string(),
        payload_mode: OscPayloadMode::NamedPairs {
            required_keys: vec!["s".to_string()],
        },
    };
    engine.add_target(target.clone(), vec![req_shape]).unwrap();
    let mut v = ControlMap::new();
    v.insert("s".to_string(), ControlValue::Str("bd".to_string()));
    let ev = dispatch_event(v, 0.0, 0.5, 0.5);
    engine.send_batch(std::slice::from_ref(&ev)).unwrap();
    assert_eq!(engine.sent_packets.lock().unwrap().len(), 1);

    // Event missing the required key -> `encode_event` Ok(None) -> the
    // `if let Some(pkt)` skip arm inside `send_batch`'s loop.
    engine.send_batch(std::slice::from_ref(&bare)).unwrap();
    assert_eq!(engine.sent_packets.lock().unwrap().len(), 1);

    // `sent_packets` is public — poison it to reach `send_batch`'s map_err and
    // the `panic()` lock-fail arm.
    let res = catch_unwind(AssertUnwindSafe(|| {
        let _g = engine.sent_packets.lock().unwrap();
        panic!("intentional panic to poison sent_packets");
    }));
    assert!(res.is_err());
    assert!(matches!(
        engine.send_batch(std::slice::from_ref(&ev)),
        Err(IoError::Osc(ref m)) if m == "osc packet log poisoned"
    ));
    engine.panic().unwrap(); // lock-fail: /hush silently dropped
}

// ---------------------------------------------------------------------------
// streams.rs — poisoned private `slots`/`sinks` mutexes. `slots` IS poisonable
// via the public API: `query_audible_events` holds the guard across user
// pattern code, and `panic()` holds the `sinks` guard across `sink.panic()`.
// ---------------------------------------------------------------------------
struct PanickySink;

impl OutputSink for PanickySink {
    fn name(&self) -> &str {
        "panicky"
    }

    fn send_batch(&self, _events: &[DispatchEvent]) -> Result<(), IoError> {
        Ok(())
    }

    fn panic(&self) -> Result<(), IoError> {
        panic!("intentional sink panic to poison the sinks mutex")
    }

    fn close(&self) -> Result<(), IoError> {
        Ok(())
    }
}

#[test]
fn cov_streams_poisoned_slot_and_sink_locks() {
    let reg = SilverWaveStreamRegistry::new();
    let d1 = StreamId::numbered(1);
    reg.set_pattern(&d1, const_pat("bd", 100.0));

    // `get_mut` None arms on a healthy registry (stream id absent).
    let missing = StreamId::numbered(15);
    reg.mute(&missing);
    reg.unmute(&missing);
    reg.solo(&missing);
    reg.unsolo(&missing);

    // A pattern whose query panics poisons `slots`: `query_audible_events`
    // holds the `slots` guard across `pat.query(&state)`, so the unwind runs
    // while the guard is held.
    let panicking: ControlPattern = Pattern::new(|_| panic!("intentional query panic"));
    reg.set_pattern(&StreamId::numbered(2), panicking);
    let res = catch_unwind(AssertUnwindSafe(|| {
        reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5)
    }));
    assert!(res.is_err());

    // Every subsequent lock-guarded op now hits its poisoned-lock arm.
    reg.set_pattern(&d1, const_pat("sd", 200.0));
    reg.mute(&d1);
    reg.unmute(&d1);
    reg.solo(&d1);
    reg.unsolo(&d1);
    reg.mute_all();
    reg.unmute_all();
    reg.silence(&d1);
    // hush skips the poisoned slots clear but still fans out panic() -> Ok.
    reg.hush().unwrap();
    // apply_transition surfaces the poison as an IoError::Config.
    assert!(matches!(
        reg.apply_transition(
            &d1,
            TransitionSpec::Jump,
            const_pat("cp", 300.0),
            CycleTime::zero()
        ),
        Err(IoError::Config(ref m)) if m == "stream registry lock poisoned"
    ));

    // `sinks` poisoning: a sink whose `panic()` itself panics unwinds while
    // `SilverWaveStreamRegistry::panic()` holds the `sinks` guard.
    let reg2 = SilverWaveStreamRegistry::new();
    reg2.add_sink(Arc::new(PanickySink));
    let res = catch_unwind(AssertUnwindSafe(|| reg2.panic()));
    assert!(res.is_err());
    reg2.add_sink(Arc::new(InProcessCsoundEngine::new())); // add_sink lock-fail
    reg2.panic().unwrap(); // sinks lock-fail -> Ok without fan-out
    reg2.hush().unwrap(); // panic() inside hush hits the same arm
}

// ---------------------------------------------------------------------------
// clock.rs — `set_playing` let-chain false edges (line 310 region): each of
// the three `&&` operands fails independently.
// ---------------------------------------------------------------------------
#[test]
fn cov_clock_set_playing_guard_edges() {
    // `link_enabled == false` short-circuits the first operand: only the
    // clock's own `is_playing` flag is updated.
    let clk = LinkClock::new(0.5, 4.0);
    clk.set_playing(true);
    assert!(clk.snapshot().is_playing);
    clk.set_playing(false);
    assert!(!clk.snapshot().is_playing);

    // `link_enabled && start_stop_sync` but `mesh == None` -> the
    // `let Some(ref mesh)` operand fails (link enabled without joining a mesh).
    clk.enable_link(true).unwrap();
    clk.enable_start_stop_sync(true).unwrap();
    clk.set_playing(true);
    assert!(clk.snapshot().is_playing);

    // `link_enabled` true but `start_stop_sync` false on a mesh peer -> the
    // middle operand fails; the mesh's own `is_playing` flag is untouched.
    let mesh = LinkSessionMesh::new();
    let peer = LinkClock::join_mesh(Arc::clone(&mesh), 0.5);
    peer.enable_start_stop_sync(false).unwrap();
    peer.set_playing(false);
    assert!(!peer.snapshot().is_playing); // local flag, sync disabled
    let other = LinkClock::join_mesh(Arc::clone(&mesh), 0.5);
    assert!(other.snapshot().is_playing); // mesh `is_playing` stayed `true`
}

// ---------------------------------------------------------------------------
// streams.rs — `combined_audible_pattern` filter arms (line 118): muted,
// solo-filtered, and pass-through slots across `any_solo` states.
// ---------------------------------------------------------------------------
#[test]
fn cov_streams_combined_audible_pattern_filter_arms() {
    let reg = SilverWaveStreamRegistry::new();
    let d1 = StreamId::numbered(1);
    let d2 = StreamId::numbered(2);
    let d3 = StreamId::numbered(3);
    reg.set_pattern(&d1, const_pat("bd", 100.0));
    reg.set_pattern(&d2, const_pat("sd", 200.0));
    reg.set_pattern(&d3, const_pat("hh", 300.0));
    let span = |p: &ControlPattern| query_transition_arc(p, CycleTime::zero(), CycleTime::one());

    // No mute/solo state: `!any_solo` edge passes all three patterns.
    assert_eq!(span(&reg.combined_audible_pattern()).len(), 3);

    // `s.muted == true` edge: d3 drops out of the combined stack.
    reg.mute(&d3);
    assert_eq!(span(&reg.combined_audible_pattern()).len(), 2);

    // `any_solo && !s.soloed` edge drops d2 (and muted d3), while
    // `any_solo && s.soloed` keeps only d1.
    reg.solo(&d1);
    assert_eq!(span(&reg.combined_audible_pattern()).len(), 1);
}

// ---------------------------------------------------------------------------
// osc.rs — `ControlValue::Bool(false)` -> `OscType::Int(0)` else arm
// (line 328) and `OscArgDefault::Bool(false)` fallback else arm (line 407).
// ---------------------------------------------------------------------------
#[test]
fn cov_osc_bool_false_osc_arms() {
    let target = OscTarget::superdirt_default();

    // `if *b { 1 } else { 0 }` -> `0` arm inside `control_value_to_osc`.
    let mut v = ControlMap::new();
    v.insert("flag".to_string(), ControlValue::Bool(false));
    let ev = dispatch_event(v, 0.0, 0.1, 0.5);
    let pkt = SilverWaveOscEngine::encode_event(&target, &OscShape::superdirt(), &ev)
        .unwrap()
        .unwrap();
    let OscPacket::Bundle(bundle) = pkt else {
        panic!("expected bundle")
    };
    let OscPacket::Message(msg) = &bundle.content[0] else {
        panic!("expected message")
    };
    assert!(msg.args.contains(&OscType::Int(0))); // Bool(false) -> Int(0)

    // `OscArgDefault::Bool(false)` on a missing param -> `Int(0)` fallback.
    let bool_default_shape = OscShape {
        address_pattern: "/cov/bool".to_string(),
        payload_mode: OscPayloadMode::ArgList(vec![OscArgSpec {
            param_name: "missing".to_string(),
            default: OscArgDefault::Bool(false),
        }]),
    };
    let empty_ev = dispatch_event(ControlMap::new(), 0.0, 0.1, 0.5);
    let pkt2 = SilverWaveOscEngine::encode_event(&target, &bool_default_shape, &empty_ev)
        .unwrap()
        .unwrap();
    let OscPacket::Bundle(bundle2) = pkt2 else {
        panic!("expected bundle")
    };
    let OscPacket::Message(msg2) = &bundle2.content[0] else {
        panic!("expected message")
    };
    assert_eq!(msg2.args, vec![OscType::Int(0)]);
}
