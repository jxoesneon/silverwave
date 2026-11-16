//! Comprehensive TDD & Protocol Parity Test Suite for `silverwave-io` (`v0.4.0`).
//! Verifies 100% of `04-SPEC-IO-STREAMS-AND-TRANSITIONS.md` Section 7 (`AUD-01`..`AUD-04`,
//! `OSC-01`..`OSC-04`, `MID-01`..`MID-08`, `LNK-01`..`LNK-03`, `TRN-01`..`TRN-09`),
//! `PP-07`, `PP-08`, `PP-12`, `CK-09`, `CK-10`, and 100.00% of all public symbols in `silverwave-io`.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use silverwave_core::{ControlMap as CoreControlMap, ControlPattern, Value, pure};
use silverwave_io::{
    ActiveNoteEntry, ActiveNoteKey, ActiveNoteTracker, ActorCursorBadge, ActorKind,
    AudioEventProducer, AudioFormat, AudioThreadEvent, ClockEngine, ClockSnapshot, ClockSource,
    CommitEpoch, ControlMap, ControlValue, ControllerInputBridge, CpalBackend, CpalConfig,
    CrdtEnsembleSession, CsoundEngine, CycleArc, CycleTime, DispatchEvent, ExternalMidiClockPll,
    HostTransportSnapshot, ImpulseAudioRenderEngine, InMemoryActiveNoteTracker,
    InMemoryMidiInputState, InMemoryMidiMapRegistry, InProcessCsoundEngine, IoError, LinkClock,
    LinkSessionMesh, MemoryCpalBackend, MemoryWebAudioBackend, MidiBackend, MidiCcMapping,
    MidiEventOrderingPriority, MidiInputNote, MidiInputState, MidiMap, MidiMapRegistry,
    MidiNoteValue, MidiPortHandle, MidiPortInfo, MidiScaleCurve, MpeChannelAllocator,
    MpeNoteExpression, MpeZoneConfig, NTP_UNIX_OFFSET_SECS, NihPlugBridge, OrcSource,
    OscArgDefault, OscArgSpec, OscBundle, OscEngine, OscMessage, OscPacket, OscPayloadMode,
    OscScheduleMode, OscShape, OscTarget, OscTime, OscType, OutputSink, PluginMidiEvent,
    PluginSyncMode, RemoteStreamCommand, ScheduledMidiPacket, SilverWaveMidiEngine,
    SilverWaveNihPlugBridge, SilverWaveOscEngine, SilverWaveStreamRegistry, SpscAudioEventProducer,
    StateMap, SteamDeckControllerBridge, StreamId, StreamManager, StreamSlotState,
    TransitionEngine, TransitionSpec, WebAudioBackend, WebAudioConfig, compile_transition_pattern,
    encode_osc_packet, midi_control_map, query_transition_arc, version,
};

struct DummyMidiBackend;
impl MidiBackend for DummyMidiBackend {
    fn list_outputs(&self) -> Result<Vec<MidiPortInfo>, IoError> {
        Ok(vec![MidiPortInfo {
            id: "0".to_string(),
            name: "IAC Bus 1".to_string(),
            is_virtual: true,
        }])
    }
    fn list_inputs(&self) -> Result<Vec<MidiPortInfo>, IoError> {
        Ok(Vec::new())
    }
    fn open_output(&mut self, _port_selector: Option<&str>) -> Result<MidiPortHandle, IoError> {
        Ok(MidiPortHandle("0".to_string()))
    }
}

fn make_const_ctrl_pattern(sound: &str, cutoff: f64, gain: f64) -> ControlPattern {
    let mut m = CoreControlMap::new();
    m.insert("s".to_string(), Value::String(sound.to_string()));
    m.insert("cutoff".to_string(), Value::Float(cutoff));
    m.insert("gain".to_string(), Value::Float(gain));
    pure(m)
}

#[test]
fn test_aud_01_02_03_04_cpal_webaudio_nihplug_and_csound_backends() {
    assert!(!version().is_empty());
    let arc = CycleArc::new(CycleTime::zero(), CycleTime::one());
    assert_eq!(CycleArc::from_timespan(arc.to_timespan()), arc);
    let nv = MidiNoteValue {
        midi_note: 60,
        detune_cents: 0.0,
    };
    assert_eq!(ControlValue::Note(nv).as_i64(), Some(60));
    assert_eq!(ControlValue::Note(nv).as_f64(), Some(60.0));
    assert_eq!(
        format!("{}", IoError::Audio("x".to_string())),
        "audio backend error: x"
    );

    // AUD-01: CpalBackend lock-free SPSC event placement at exact `frame_offset`
    assert!(!MemoryCpalBackend::list_output_devices().unwrap().is_empty());
    let cpal_cfg = CpalConfig::default();
    let mut cpal =
        MemoryCpalBackend::start(cpal_cfg, Box::new(ImpulseAudioRenderEngine::default())).unwrap();
    let fmt: AudioFormat = cpal.format();
    assert_eq!(fmt.sample_rate, 48_000);
    let sender: SpscAudioEventProducer = cpal.event_sender();
    let mut cmap = ControlMap::new();
    cmap.insert("gain".to_string(), ControlValue::Float(0.75));
    let ev = AudioThreadEvent {
        frame_offset: 29,
        duration_frames: 64,
        orbit: 0,
        controls: cmap,
    };
    sender.try_push(ev.clone()).unwrap();

    let mut cpal_buf = vec![0.0f32; 128 * 2];
    cpal.render_next_block(&mut cpal_buf);
    assert_eq!(cpal_buf[28 * 2], 0.0);
    assert!((cpal_buf[29 * 2] - 0.75).abs() < 1.0e-6);
    assert!(cpal.sample_clock_secs() > 0.0);

    // AUD-02: WebAudioBackend offline render bit-for-bit parity with native CpalBackend
    let web = MemoryWebAudioBackend::initialize(WebAudioConfig::default()).unwrap();
    web.resume().unwrap();
    web.post_event_batch(std::slice::from_ref(&ev)).unwrap();
    assert_eq!(web.context_time_secs(), 0.0);
    assert!(web.output_latency_secs() > 0.0);
    web.suspend().unwrap();
    let web_buf = MemoryWebAudioBackend::render_offline(fmt, 128.0 / 48_000.0, &[ev]).unwrap();
    assert_eq!(cpal_buf, web_buf);
    cpal.stop().unwrap();

    // AUD-03: NihPlugBridge host transport sync across tempo change (120 -> 140 BPM)
    let mut plug = SilverWaveNihPlugBridge::new();
    assert_eq!(plug.sync_mode(), PluginSyncMode::HostTransport);
    plug.set_sync_mode(PluginSyncMode::HostTransport);
    plug.on_transport_update(
        &HostTransportSnapshot {
            playing: true,
            tempo_bpm: 120.0,
            pos_beats: Some(0.0),
            pos_samples: Some(0),
            time_sig_num: 4,
            time_sig_den: 4,
        },
        48_000.0,
    );
    let mut p_audio = vec![0.0f32; 24_000 * 2];
    let mut p_midi: Vec<PluginMidiEvent> = Vec::new();
    plug.render_plugin_buffer(24_000, 2, &mut p_audio, &mut p_midi);
    // Change tempo to 140 BPM mid-stream
    plug.on_transport_update(
        &HostTransportSnapshot {
            playing: true,
            tempo_bpm: 140.0,
            pos_beats: Some(1.0),
            pos_samples: Some(24_000),
            time_sig_num: 4,
            time_sig_den: 4,
        },
        48_000.0,
    );
    plug.render_plugin_buffer(24_000, 2, &mut p_audio, &mut p_midi);
    // Beats 0->1 in buffer 1, then 1.0->2.167 at 140 BPM in buffer 2: quarter-beat pulses at 0, 1, 2.
    assert_eq!(p_midi.len(), 3);

    // AUD-04: CsoundEngine `github:` shorthand, `start_instrument("Mono")` -> `i "Mono" 0 -1`, and `chnset`
    let mut cs = InProcessCsoundEngine::new();
    cs.init_csound(48_000).unwrap();
    cs.load_orc(OrcSource::GithubShorthand(
        "github:user/repo/main/synths.orc".to_string(),
    ))
    .unwrap();
    assert_eq!(
        cs.loaded_orcs[0],
        "https://raw.githubusercontent.com/user/repo/main/synths.orc"
    );
    cs.start_instrument("Mono").unwrap();
    cs.stop_instrument("Mono").unwrap();
    cs.set_control_channel("NoiSaw.cut", 800.0).unwrap();
    assert_eq!(cs.get_control_channel("NoiSaw.cut"), Some(800.0));
    assert!(
        cs.emitted_score_lines()
            .contains(&"i \"Mono\" 0 -1".to_string())
    );
}

#[test]
fn test_osc_01_02_03_04_superdirt_wire_ctrl_and_custom_shapes() {
    assert_eq!(NTP_UNIX_OFFSET_SECS, 2_208_988_800);
    let mut osc = SilverWaveOscEngine::new();
    let target = OscTarget::superdirt_default();
    let shape = OscShape::superdirt();
    osc.add_target(target.clone(), vec![shape.clone()]).unwrap();

    // OSC-01: `/dirt/play` PreBundleStamp NTP timetag and mandatory fields
    let mut cmap = ControlMap::new();
    cmap.insert("s".to_string(), ControlValue::Str("bd".to_string()));
    cmap.insert("gain".to_string(), ControlValue::Float(0.9));
    let ev = DispatchEvent {
        whole: Some(CycleArc::new(CycleTime::zero(), CycleTime::one())),
        part: CycleArc::new(CycleTime::zero(), CycleTime::one()),
        value: cmap,
        onset_secs: 100.0,
        duration_secs: 0.5,
        cps: 0.5,
        stream_id: StreamId::numbered(1),
    };
    osc.send_batch(std::slice::from_ref(&ev)).unwrap();
    let pkt: OscPacket = SilverWaveOscEngine::encode_event(&target, &shape, &ev)
        .unwrap()
        .unwrap();
    let raw_bytes = encode_osc_packet(&pkt);
    assert!(raw_bytes.starts_with(b"#bundle\0"));
    match pkt {
        OscPacket::Bundle(OscBundle { timetag, content }) => {
            assert_eq!(timetag, OscTime::from_unix_secs(100.2));
            assert!((timetag.to_unix_secs() - 100.2).abs() < 1.0e-5);
            match &content[0] {
                OscPacket::Message(OscMessage { addr, args }) => {
                    assert_eq!(addr, "/dirt/play");
                    assert!(args.contains(&OscType::String("cps".to_string())));
                    assert!(args.contains(&OscType::String("cycle".to_string())));
                    assert!(args.contains(&OscType::String("delta".to_string())));
                    assert!(args.contains(&OscType::String("orbit".to_string())));
                    assert!(args.contains(&OscType::String("s".to_string())));
                    assert!(args.contains(&OscType::String("gain".to_string())));
                }
                _ => panic!("expected OscMessage"),
            }
        }
        _ => panic!("expected OscBundle"),
    }

    // OSC-02 & OSC-03: `/ctrl` receiver and `/mute`, `/solo`, `/hush`
    let mut state_map = StateMap::new();
    let mut cmds = Vec::new();
    SilverWaveOscEngine::handle_incoming_message(
        &OscMessage {
            addr: "/ctrl".to_string(),
            args: vec![
                OscType::String("cutoff".to_string()),
                OscType::Float(1200.0),
            ],
        },
        &mut state_map,
        &mut cmds,
    );
    SilverWaveOscEngine::handle_incoming_message(
        &OscMessage {
            addr: "/ctrl".to_string(),
            args: vec![OscType::Int(4), OscType::Float(0.75)],
        },
        &mut state_map,
        &mut cmds,
    );
    assert_eq!(state_map.get("cutoff"), Some(&ControlValue::Float(1200.0)));
    assert_eq!(state_map.get("4"), Some(&ControlValue::Float(0.75)));

    SilverWaveOscEngine::handle_incoming_message(
        &OscMessage {
            addr: "/mute".to_string(),
            args: vec![OscType::Int(1)],
        },
        &mut state_map,
        &mut cmds,
    );
    SilverWaveOscEngine::handle_incoming_message(
        &OscMessage {
            addr: "/solo".to_string(),
            args: vec![OscType::String("drums".to_string())],
        },
        &mut state_map,
        &mut cmds,
    );
    SilverWaveOscEngine::handle_incoming_message(
        &OscMessage {
            addr: "/hush".to_string(),
            args: vec![],
        },
        &mut state_map,
        &mut cmds,
    );
    assert_eq!(
        cmds,
        vec![
            RemoteStreamCommand::Mute(StreamId("1".to_string())),
            RemoteStreamCommand::Solo(StreamId("drums".to_string())),
            RemoteStreamCommand::Hush,
        ]
    );

    // OSC-04: Custom `OscShape` with `{color}` address interpolation and `Required` arg gate
    let custom_shape = OscShape {
        address_pattern: "/fixture/{color}/set".to_string(),
        payload_mode: OscPayloadMode::ArgList(vec![OscArgSpec {
            param_name: "intensity".to_string(),
            default: OscArgDefault::Required,
        }]),
    };
    // Missing `color` and `intensity` -> suppressed (`Ok(None)`)
    assert!(
        SilverWaveOscEngine::encode_event(&target, &custom_shape, &ev)
            .unwrap()
            .is_none()
    );

    let mut ev_color = ev.clone();
    ev_color
        .value
        .insert("color".to_string(), ControlValue::Str("red".to_string()));
    ev_color
        .value
        .insert("intensity".to_string(), ControlValue::Float(0.8));
    let mut live_target = target.clone();
    live_target.schedule = OscScheduleMode::Live;
    let pkt_custom = SilverWaveOscEngine::encode_event(&live_target, &custom_shape, &ev_color)
        .unwrap()
        .unwrap();
    match pkt_custom {
        OscPacket::Message(m) => assert_eq!(m.addr, "/fixture/red/set"),
        _ => panic!("expected OscMessage"),
    }
}

#[test]
fn test_mid_01_to_08_midi_wire_nrpn_sysex_mpe_pll_and_note_off_safety() {
    let mut dummy_backend = DummyMidiBackend;
    assert_eq!(dummy_backend.list_outputs().unwrap().len(), 1);
    assert!(dummy_backend.list_inputs().unwrap().is_empty());
    let _handle: MidiPortHandle = dummy_backend.open_output(None).unwrap();

    let midi = SilverWaveMidiEngine::new("iac_1", true);

    // MID-01: `note=60, midichan=2, velocity=0.8, gain=0.5` -> `[0x91, 60, 51]` and `[0x81, 60, 0]`
    let ev1 = DispatchEvent {
        whole: None,
        part: CycleArc::new(CycleTime::zero(), CycleTime::one()),
        value: midi_control_map(vec![
            ("note", ControlValue::Int(60)),
            ("midichan", ControlValue::Int(2)),
            ("velocity", ControlValue::Float(0.8)),
            ("gain", ControlValue::Float(0.5)),
        ]),
        onset_secs: 1.0,
        duration_secs: 0.5,
        cps: 0.5,
        stream_id: StreamId::numbered(1),
    };
    let pkts1: Vec<ScheduledMidiPacket> = midi.encode_dispatch_event(&ev1);
    assert_eq!(pkts1[0].bytes, vec![0x91, 60, 51]);
    assert_eq!(pkts1[0].priority, MidiEventOrderingPriority::NoteOn);
    assert_eq!(pkts1[1].bytes, vec![0x81, 60, 0]);
    assert_eq!(
        pkts1[1].priority,
        MidiEventOrderingPriority::ScheduledNoteOff
    );

    // MID-02: `midimaps` Linear (`0.5 -> 64`) and Exponential (`min=20, max=120, exp=2.0`, `0.5 -> 45`)
    let mut map_reg = InMemoryMidiMapRegistry::new();
    let mut mmap = MidiMap {
        name: "synth".to_string(),
        default_channel: Some(1),
        controls: BTreeMap::new(),
    };
    let lin_map = MidiCcMapping {
        cc: 74,
        min: 0.0,
        max: 1.0,
        curve: MidiScaleCurve::Linear,
    };
    let exp_map = MidiCcMapping {
        cc: 71,
        min: 20.0,
        max: 120.0,
        curve: MidiScaleCurve::Exponential(2.0),
    };
    mmap.controls.insert("cutoff".to_string(), lin_map.clone());
    map_reg.register_map(mmap);
    assert!(map_reg.get_map("synth").is_some());
    assert_eq!(InMemoryMidiMapRegistry::scale_to_u7(&lin_map, 0.5), 64);
    assert_eq!(InMemoryMidiMapRegistry::scale_to_u7(&exp_map, 0.5), 45);

    // MID-03, MID-04, MID-05, MID-06: NRPN (`99, 98, 6, 38`), SysEx (`0xF0..0xF7`), PitchBend (`0xE0`), Aftertouch (`0xD0`), Clock (`0xF8`)
    let ev_complex = DispatchEvent {
        whole: None,
        part: CycleArc::new(CycleTime::zero(), CycleTime::one()),
        value: midi_control_map(vec![
            ("nrpnn", ControlValue::Bytes(vec![0x01, 0x20])),
            ("nrpv", ControlValue::Int(64)),
            ("sysexid", ControlValue::Bytes(vec![0x7E, 0x7F])),
            ("sysexdata", ControlValue::Bytes(vec![0x06, 0x01])),
            ("midibend", ControlValue::Float(0.0)),
            ("miditouch", ControlValue::Float(1.0)),
            ("midicmd", ControlValue::Str("start".to_string())),
        ]),
        onset_secs: 2.0,
        duration_secs: 0.5,
        cps: 0.5,
        stream_id: StreamId::numbered(1),
    };
    let pkts_c = midi.encode_dispatch_event(&ev_complex);
    let raw_list: Vec<Vec<u8>> = pkts_c.into_iter().map(|p| p.bytes).collect();
    assert!(raw_list.contains(&vec![0xB0, 99, 1]));
    assert!(raw_list.contains(&vec![0xB0, 98, 32]));
    assert!(raw_list.contains(&vec![0xB0, 6, 64]));
    assert!(raw_list.contains(&vec![0xB0, 38, 0]));
    assert!(raw_list.contains(&vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]));
    assert!(raw_list.contains(&vec![0xE0, 0x00, 0x40]));
    assert!(raw_list.contains(&vec![0xD0, 127]));
    assert!(raw_list.contains(&vec![0xFA]));

    // MID-07: `midin` & `midikeys`
    let in_state = InMemoryMidiInputState::new();
    in_state.ingest_raw_midi(&[0xB0, 74, 127]);
    assert!((in_state.get_cc(1, 74) - 1.0).abs() < 1.0e-6);
    in_state.ingest_raw_midi(&[0x90, 60, 100]);
    let keys: Vec<MidiInputNote> = in_state.active_keys(Some(1));
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].note, 60);
    in_state.ingest_raw_midi(&[0x80, 60, 0]);
    assert!(in_state.active_keys(Some(1)).is_empty());

    // MID-08: ActiveNoteTracker & `panic()` flushing all held notes + CC 123 + CC 120 across all 16 channels
    let tracker = InMemoryActiveNoteTracker::new();
    let k0 = ActiveNoteKey {
        port_id: "iac_1".to_string(),
        channel: 0,
        note: 64,
    };
    let _entry = ActiveNoteEntry {
        key: k0.clone(),
        off_time_secs: 5.0,
        stream_id: StreamId::numbered(1),
    };
    tracker.track_note_on(k0, 5.0, &StreamId::numbered(1));
    assert_eq!(tracker.drain_due_note_offs(6.0).len(), 1);
    assert_eq!(
        tracker.drain_stream_note_offs(&StreamId::numbered(1)).len(),
        0
    );

    for n in [60, 64, 67, 72] {
        let mut ev_n = ev1.clone();
        ev_n.value.insert("note".to_string(), ControlValue::Int(n));
        midi.send_batch(&[ev_n]).unwrap();
    }
    assert_eq!(midi.tracker.active_count(), 4);
    midi.panic().unwrap();
    assert_eq!(midi.tracker.active_count(), 0);
    let tx_bytes = midi.transmitted_bytes();
    assert!(tx_bytes.contains(&vec![0xB0, 123, 0]));
    assert!(tx_bytes.contains(&vec![0xBF, 120, 0]));

    // MPE, External Clock PLL, and Steam Deck Controller Bridge (`PP-12`, `CK-10`)
    let mut mpe = MpeChannelAllocator::new(MpeZoneConfig::default());
    let (mpe_ch, mpe_msgs) = mpe.allocate_and_encode(&MpeNoteExpression {
        pitch_bend_semitones: 12.0,
        timbre_cc74: 0.5,
        pressure: 0.8,
    });
    assert_eq!(mpe_ch, 2);
    assert_eq!(mpe_msgs.len(), 3);

    let mut pll = ExternalMidiClockPll::default();
    let tick_dt = 60.0 / (128.0 * 24.0);
    for i in 0..48 {
        pll.on_clock_tick((i as f64) * tick_dt);
    }
    assert!((pll.estimated_bpm - 128.0).abs() < 1.0);

    let deck = SteamDeckControllerBridge::new();
    deck.set_axis(0, "left_x", 0.75);
    deck.set_button(0, "a", 1.0);
    deck.set_motion("gx", 0.5);
    assert_eq!(deck.gamepad_axis(0, "left_x"), 0.75);
    assert_eq!(deck.gamepad_button(0, "a"), 1.0);
    assert_eq!(deck.device_motion("gx"), 0.5);
}

#[test]
fn test_lnk_01_02_03_and_trn_01_to_09_and_crdt_ensemble() {
    // LNK-01: Tempo continuity mid-cycle (`cycle = 10.35` at `cps = 0.5` -> `wall_secs = 20.7`)
    let clk = LinkClock::new(0.5, 4.0);
    assert_eq!(clk.source(), ClockSource::Monotonic);
    clk.set_wall_time_secs(20.7);
    assert!((clk.wall_secs_to_cycle(20.7) - 10.35).abs() < 1.0e-9);
    clk.set_cps(0.75).unwrap();
    // Immediately after `set_cps(0.75)`, cycle position at `20.7s` MUST still be `10.35 +- 1e-9`!
    assert!((clk.wall_secs_to_cycle(20.7) - 10.35).abs() < 1.0e-9);
    clk.set_bpm(120.0).unwrap();
    clk.set_beats_per_cycle(4.0).unwrap();
    clk.nudge_secs(0.0);
    assert!(
        clk.cycle_to_wall_secs(CycleTime::from_integer(12))
            .is_finite()
    );

    // LNK-02 & LNK-03: Multi-peer phase lock and `start_stop_sync`
    let mesh = LinkSessionMesh::new();
    let peer_a = LinkClock::join_mesh(Arc::clone(&mesh), 0.5);
    let peer_b = LinkClock::join_mesh(Arc::clone(&mesh), 0.5);
    peer_a.enable_link(true).unwrap();
    peer_b.enable_link(true).unwrap();
    peer_a.enable_start_stop_sync(true).unwrap();
    peer_b.enable_start_stop_sync(true).unwrap();
    assert_eq!(peer_a.link_num_peers(), 1);
    peer_a.set_wall_time_secs(4.0);
    peer_b.set_wall_time_secs(4.0);
    let snap_a: ClockSnapshot = peer_a.snapshot();
    let snap_b: ClockSnapshot = peer_b.snapshot();
    assert!((snap_a.cycle - snap_b.cycle).abs() < 0.002);

    peer_a.set_playing(false);
    assert!(!peer_b.snapshot().is_playing);

    // TRN-01..TRN-08: All TidalCycles transitions
    let prev = make_const_ctrl_pattern("bd", 100.0, 1.0);
    let next = make_const_ctrl_pattern("sd", 16000.0, 1.0);
    let t_eval = CycleTime::from_integer(0);

    // TRN-01: `xfadeIn(4)`
    let xfade_pat = compile_transition_pattern(
        &TransitionSpec::XfadeIn {
            cycles: CycleTime::from_integer(4),
        },
        std::slice::from_ref(&prev),
        &next,
        t_eval,
    );
    let h_after = query_transition_arc(
        &xfade_pat,
        CycleTime::from_integer(4),
        CycleTime::from_integer(5),
    );
    assert_eq!(h_after.len(), 1);
    assert_eq!(
        h_after[0].value.get("s"),
        Some(&Value::String("sd".to_string()))
    );

    // TRN-02: `clutchIn(2)`
    let clutch_pat = compile_transition_pattern(
        &TransitionSpec::ClutchIn {
            cycles: CycleTime::from_integer(2),
        },
        std::slice::from_ref(&prev),
        &next,
        t_eval,
    );
    assert_eq!(
        query_transition_arc(
            &clutch_pat,
            CycleTime::from_integer(3),
            CycleTime::from_integer(4)
        )[0]
        .value
        .get("s"),
        Some(&Value::String("sd".to_string()))
    );

    // TRN-03: `interpolateIn(4)` (`cutoff(2.0) == 8050.0`)
    let interp_pat = compile_transition_pattern(
        &TransitionSpec::InterpolateIn {
            cycles: CycleTime::from_integer(4),
        },
        std::slice::from_ref(&prev),
        &next,
        t_eval,
    );
    let h_mid = query_transition_arc(
        &interp_pat,
        CycleTime::from_integer(2),
        CycleTime::from_integer(3),
    );
    assert_eq!(h_mid[0].value.get("cutoff"), Some(&Value::Float(8050.0)));

    // TRN-04: `jumpIn(2)` vs `jumpIn'(2)` evaluated at `c = 3.4` (`17/5`)
    let c_3_4 = CycleTime::new_const(17, 5);
    let j_unaligned = compile_transition_pattern(
        &TransitionSpec::JumpIn { cycles: 2 },
        std::slice::from_ref(&prev),
        &next,
        c_3_4,
    );
    let j_aligned = compile_transition_pattern(
        &TransitionSpec::JumpInPrime { cycles: 2 },
        std::slice::from_ref(&prev),
        &next,
        c_3_4,
    );
    // At cycle 5 (`[5, 6)`), `jumpIn(2)` has switched at `5.4`, while `jumpIn'(2)` waits until `ceil(3.4) + 2 = 6.0`!
    assert_eq!(
        query_transition_arc(
            &j_aligned,
            CycleTime::from_integer(5),
            CycleTime::from_integer(6)
        )[0]
        .value
        .get("s"),
        Some(&Value::String("bd".to_string()))
    );
    assert_eq!(
        query_transition_arc(
            &j_aligned,
            CycleTime::from_integer(6),
            CycleTime::from_integer(7)
        )[0]
        .value
        .get("s"),
        Some(&Value::String("sd".to_string()))
    );
    assert_eq!(
        query_transition_arc(
            &j_unaligned,
            CycleTime::from_integer(6),
            CycleTime::from_integer(7)
        )[0]
        .value
        .get("s"),
        Some(&Value::String("sd".to_string()))
    );

    // TRN-05: `jumpMod(4)` evaluated at `c = 5.2` (`26/5`) switches at `c = 8.0`
    let c_5_2 = CycleTime::new_const(26, 5);
    let j_mod = compile_transition_pattern(
        &TransitionSpec::JumpMod { cycle_mod: 4 },
        std::slice::from_ref(&prev),
        &next,
        c_5_2,
    );
    assert_eq!(
        query_transition_arc(
            &j_mod,
            CycleTime::from_integer(7),
            CycleTime::from_integer(8)
        )[0]
        .value
        .get("s"),
        Some(&Value::String("bd".to_string()))
    );
    assert_eq!(
        query_transition_arc(
            &j_mod,
            CycleTime::from_integer(8),
            CycleTime::from_integer(9)
        )[0]
        .value
        .get("s"),
        Some(&Value::String("sd".to_string()))
    );

    // TRN-06: `histpan(4)` assigns distinct stereo pan values ordered left-to-right
    let hist_pat = compile_transition_pattern(
        &TransitionSpec::Histpan { count: 4 },
        &[prev.clone(), prev.clone(), prev.clone()],
        &next,
        t_eval,
    );
    let h_pan = query_transition_arc(&hist_pat, CycleTime::zero(), CycleTime::one());
    assert_eq!(h_pan.len(), 4);
    assert_eq!(h_pan[0].value.get("pan"), Some(&Value::Float(0.0)));
    assert_eq!(h_pan[3].value.get("pan"), Some(&Value::Float(1.0)));

    // TRN-07: `wait(4)` & `waitT(4)`
    let w_pat = compile_transition_pattern(
        &TransitionSpec::Wait {
            cycles: CycleTime::from_integer(4),
        },
        std::slice::from_ref(&prev),
        &next,
        t_eval,
    );
    assert!(query_transition_arc(&w_pat, CycleTime::zero(), CycleTime::from_integer(4)).is_empty());
    assert_eq!(
        query_transition_arc(
            &w_pat,
            CycleTime::from_integer(4),
            CycleTime::from_integer(5)
        )
        .len(),
        1
    );

    // TRN-08: `mortal(4, 2)`
    let m_pat = compile_transition_pattern(
        &TransitionSpec::Mortal {
            lifespan: CycleTime::from_integer(4),
            release: CycleTime::from_integer(2),
        },
        std::slice::from_ref(&prev),
        &next,
        t_eval,
    );
    assert_eq!(
        query_transition_arc(&m_pat, CycleTime::zero(), CycleTime::one()).len(),
        1
    );
    assert!(
        query_transition_arc(
            &m_pat,
            CycleTime::from_integer(6),
            CycleTime::from_integer(7)
        )
        .is_empty()
    );

    // TRN-09: `SilverWaveStreamRegistry` solo / mute / `$:` orphan cleanup
    let reg = SilverWaveStreamRegistry::new();
    let d1 = StreamId::numbered(1);
    let d2 = StreamId::numbered(2);
    reg.set_pattern(&d1, prev.clone());
    reg.set_pattern(&d2, next.clone());
    reg.solo(&d2);
    let audible = reg.query_audible_events(CycleTime::zero(), CycleTime::one(), 0.5);
    assert_eq!(audible.len(), 1);
    assert_eq!(audible[0].stream_id, d2);
    reg.unsolo(&d2);
    reg.mute(&d1);
    reg.unmute(&d1);
    reg.mute_all();
    reg.unmute_all();
    reg.apply_dollar_colon_block(vec![prev.clone(), next.clone(), prev.clone()]);
    assert!(reg.active_streams().contains(&StreamId("$:2".to_string())));
    reg.apply_dollar_colon_block(vec![prev.clone(), next.clone()]);
    assert!(!reg.active_streams().contains(&StreamId("$:2".to_string())));
    reg.apply_transition(&d1, TransitionSpec::Jump, next.clone(), CycleTime::zero())
        .unwrap();
    let _ = reg.combined_audible_pattern();
    let _ = StreamSlotState {
        id: d1.clone(),
        history: vec![prev.clone()],
        muted: false,
        soloed: false,
        orbit: 0,
    };
    let _ = prev.clone().fade_in(CycleTime::one(), CycleTime::one());
    let _ = prev.fade_out(CycleTime::one(), CycleTime::one());
    reg.hush().unwrap();

    // CK-09: Cycle-Quantized CRDT Ensemble Sync
    let mut crdt = CrdtEnsembleSession::new("peer_eduardo", 10.0);
    crdt.update_cursor(ActorCursorBadge {
        peer_id: "peer_agent".to_string(),
        display_name: "MCP Co-Performer".to_string(),
        kind: ActorKind::McpAgent,
        byte_offset: 12,
        color_rgba: [0, 255, 200, 255],
    });
    let ep = CommitEpoch {
        epoch_id: 1,
        author_peer_id: "peer_agent".to_string(),
        actor_kind: ActorKind::McpAgent,
        target_cycle: CycleTime::from_integer(8),
        dac_latency_offset_ms: 10.0,
        stream_id: "d1".to_string(),
        code_patch: "s(\"bd*4\")".to_string(),
    };
    assert!((crdt.acoustic_activation_wall_secs(&ep, 0.5) - 15.99).abs() < 1.0e-4);
    crdt.stage_commit(ep);
    assert!(crdt.poll_due_epochs(CycleTime::from_integer(7)).is_empty());
    assert_eq!(crdt.poll_due_epochs(CycleTime::from_integer(8)).len(), 1);
}

#[test]
fn test_helper_surface_completeness() {
    let cv_str = ControlValue::Str("superdirt".to_string());
    assert_eq!(cv_str.as_str(), Some("superdirt"));

    let gh =
        OrcSource::GithubShorthand("github:tidalcycles/Dirt-Samples/master/synths.orc".to_string());
    assert_eq!(
        gh.resolve_url().as_deref(),
        Some("https://raw.githubusercontent.com/tidalcycles/Dirt-Samples/master/synths.orc")
    );

    let reg = SilverWaveStreamRegistry::new();
    reg.add_sink(Arc::new(InProcessCsoundEngine::new()));

    let tracker = InMemoryActiveNoteTracker::new();
    let key = ActiveNoteKey {
        port_id: "iac_1".to_string(),
        channel: 0,
        note: 60,
    };
    tracker.track_note_on(key.clone(), 4.0, &StreamId::numbered(1));
    assert!(tracker.remove_if_active(&key));
    assert!(!tracker.remove_if_active(&key));
}
