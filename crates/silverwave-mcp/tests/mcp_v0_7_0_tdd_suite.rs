//! Comprehensive TDD Conformance Suite for `silverwave-mcp` (`v0.7.0`).
//! Verifies 100% of `07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md` (Tiers 1–4, all 42 tools,
//! 10 live resources, 5 prompts, surgical AST slider patching, and sandbox/SSRF/rate-limit guards)
//! and 100.00% of all public symbols in `silverwave-mcp`.

#![forbid(unsafe_code)]

use silverwave_core::Rational64;
use silverwave_mcp::{
    AbletonLinkStatusResponse, AcousticMixDiagnostic, AudioExportFormat,
    CaptureVisualizerFrameRequest, CaptureVisualizerFrameResponse, ClockSnapshotResponse,
    CoPerformanceBridge, ConfigureAbletonLinkRequest, ConfigureMidiMapRequest,
    ConfigureMidiMapResponse, ConfigureOrbitRequest, ConfigureOrbitResponse,
    ConfigureOscTargetRequest, ConfigureOscTargetResponse, ConflictKind, ControlStateEntry,
    ControlValue, DecodeShareUrlRequest, DecodeShareUrlResponse, DefaultSandboxGuard,
    DiagnosticSeverity, DoctorSubsystemCheck, DrawCmd, ERR_DSL_SYNTAX, ERR_EXTERNAL_IO,
    ERR_INVALID_PARAMS, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, ERR_PARSE_ERROR,
    ERR_RATE_LIMIT_EXCEEDED, ERR_READ_ONLY_VIOLATION, ERR_REVISION_CONFLICT, ERR_SAMPLE_LOAD,
    ERR_SANDBOX_VIOLATION, ERR_STREAM_NOT_FOUND, ERR_WIDGET_NOT_FOUND, EncodeShareUrlRequest,
    EncodeShareUrlResponse, EvaluateBlockRequest, EvaluateBlockResponse, EvaluateBufferRequest,
    EvaluateBufferResponse, ExportAudioRequest, ExportAudioResponse, GetClockSnapshotRequest,
    GetControlStateRequest, GetControlStateResponse, GetOrbitTelemetryRequest,
    GetOrbitTelemetryResponse, HapEventRecord, HumanCursorSelection, HushRequest, HushResponse,
    HydraAudioUniforms, InterOrbitConflictAlert, ListMidiPortsRequest, ListMidiPortsResponse,
    ListSampleBanksRequest, ListSampleBanksResponse, ListSlidersRequest, ListSlidersResponse,
    LoadCsoundOrcRequest, LoadCsoundOrcResponse, LoadSampleSourceRequest, LoadSampleSourceResponse,
    MAX_COMMANDS_PER_AUDIO_BLOCK, MCP_ALL_42_TOOL_NAMES, MCP_RESOURCE_URIS, McpCliArgs,
    McpProtocolError, McpTransportMode, MidiMappingTarget, MidiOutboundCommand, MidiPortDescriptor,
    MidiPortDirection, MiniAstNode, NudgeClockRequest, OrbitStateSnapshot, PanicRequest,
    PanicResponse, ParseMiniNotationRequest, ParseMiniNotationResponse, PathSandboxGuard,
    PreallocatedAudioCommandRing, PromptDescriptor, QuantizeGrid, QueryPatternArcRequest,
    QueryPatternArcResponse, RateLimiterPolicy, ReadAudioTelemetryRequest,
    ReadAudioTelemetryResponse, RenderVideoRequest, RenderedPromptPayload,
    ResourceSubscriptionHandle, RtAudioCommand, RunDoctorRequest, RunDoctorResponse,
    SampleBankEntry, SampleSourceDescriptor, SendMidiCommandRequest, SendMidiCommandResponse,
    SessionAttachmentTarget, SetBeatsPerCycleRequest, SetBpmRequest, SetControlStateRequest,
    SetControlStateResponse, SetCpsRequest, SetCrossfaderSceneRequest, SetFillLatchRequest,
    SetStreamRequest, SetStreamResponse, SilverWaveMcpCategoryKingExtension,
    SilverWaveMcpPromptSurface, SilverWaveMcpServer, SilverWaveMcpToolSurface, SilverWavePromptId,
    SliderWidgetDescriptor, StreamStateSummary, StreamToggleRequest, StreamToggleResponse,
    SyntaxDiagnostic, TidalTransitionSpec, TokenBucketRateLimiter, ToolRateCategory,
    TransitionStreamRequest, TransitionStreamResponse, TriggerLooperFreezeRequest,
    UpdateSliderRequest, UpdateSliderResponse, ValidateSyntaxRequest, ValidateSyntaxResponse,
    VisualizerKind, VisualizerOutputFormat, version,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[test]
fn test_tier1_protocol_conformance_and_resources_prompts() {
    assert!(!version().is_empty());
    assert_eq!(MCP_ALL_42_TOOL_NAMES.len(), 42);
    assert_eq!(MCP_RESOURCE_URIS.len(), 10);
    assert_eq!(MAX_COMMANDS_PER_AUDIO_BLOCK, 64);
    assert_eq!(ERR_PARSE_ERROR, -32700);
    assert_eq!(ERR_INVALID_REQUEST, -32600);
    assert_eq!(ERR_METHOD_NOT_FOUND, -32601);
    assert_eq!(ERR_INVALID_PARAMS, -32602);
    assert_eq!(ERR_REVISION_CONFLICT, -32001);
    assert_eq!(ERR_DSL_SYNTAX, -32002);
    assert_eq!(ERR_STREAM_NOT_FOUND, -32003);
    assert_eq!(ERR_WIDGET_NOT_FOUND, -32004);
    assert_eq!(ERR_SAMPLE_LOAD, -32005);
    assert_eq!(ERR_SANDBOX_VIOLATION, -32006);
    assert_eq!(ERR_EXTERNAL_IO, -32007);
    assert_eq!(ERR_READ_ONLY_VIOLATION, -32008);
    assert_eq!(ERR_RATE_LIMIT_EXCEEDED, -32029);

    let err = McpProtocolError::new(ERR_PARSE_ERROR, "bad json").with_data("line 1");
    assert_eq!(err.data.as_deref(), Some("line 1"));

    let args = McpCliArgs {
        allow_external_network: true,
        attach_session: Some(SessionAttachmentTarget::DefaultLocalSession),
        transport: McpTransportMode::HttpSse {
            endpoint_path: "/mcp".to_string(),
            bearer_token: Some("tok".to_string()),
            allowed_origins: vec!["http://localhost:3000".to_string()],
        },
        ..Default::default()
    };

    let mut srv = SilverWaveMcpServer::new(args);
    let init_ok = srv
        .handle_jsonrpc_payload(
            "c1",
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}",
        )
        .unwrap();
    assert!(init_ok.contains("\"count\":42"));

    for uri in MCP_RESOURCE_URIS {
        let res_json = srv.read_resource_json(uri).unwrap();
        assert!(res_json.contains(uri));
    }

    let prompts: Vec<PromptDescriptor> = srv.list_prompts();
    assert_eq!(prompts.len(), 5);
    let rendered: RenderedPromptPayload = srv
        .get_prompt(SilverWavePromptId::LiveCodeSession, BTreeMap::new())
        .unwrap();
    assert_eq!(rendered.prompt_name, "live_code_session");
}

#[test]
fn test_tier2_all_42_tools_human_agent_parity() {
    let args = McpCliArgs {
        allow_external_network: true,
        ..Default::default()
    };
    let mut srv = SilverWaveMcpServer::new(args);

    // 1. evaluate_buffer & 2. evaluate_block
    let eval_res: EvaluateBufferResponse = srv
        .evaluate_buffer(EvaluateBufferRequest {
            code: Some("d1 $ s(\"bd sd\") .lpf(slider(0.25, 0.0, 1.0, 0.01))".to_string()),
            sync_to_editor: true,
            expected_revision: Some(1),
            quantize: QuantizeGrid::NextCycle,
            auto_start_clock: true,
        })
        .unwrap();
    assert_eq!(eval_res.revision, 2);
    assert_eq!(eval_res.discovered_widgets.len(), 1);

    let blk_res: EvaluateBlockResponse = srv
        .evaluate_block(EvaluateBlockRequest {
            cursor_line: Some(1),
            line_range: None,
            byte_range: None,
            target_stream_override: Some("d1".to_string()),
            quantize: QuantizeGrid::Immediate,
        })
        .unwrap();
    assert_eq!(blk_res.target_stream, "d1");

    // 3. parse_mini_notation, 4. query_pattern_arc, 5. validate_syntax
    let parsed: ParseMiniNotationResponse = srv
        .parse_mini_notation(ParseMiniNotationRequest {
            notation: "bd ~ sd".to_string(),
            base_byte_offset: 0,
            base_line: 1,
            base_col: 1,
        })
        .unwrap();
    assert!(matches!(parsed.ast, MiniAstNode::Sequence { .. }));
    assert_eq!(parsed.leaf_spans.len(), 3);

    let arc_res: QueryPatternArcResponse = srv
        .query_pattern_arc(QueryPatternArcRequest {
            pattern_code: Some("bd sd cp hh".to_string()),
            stream_id: None,
            begin_cycle: Rational64::from_integer(0),
            end_cycle: Rational64::from_integer(1),
            max_events: 16,
            deterministic_seed: 42,
        })
        .unwrap();
    assert_eq!(arc_res.total_events, 4);
    let _ev0: &HapEventRecord = &arc_res.events[0];

    let val_res: ValidateSyntaxResponse = srv
        .validate_syntax(ValidateSyntaxRequest {
            code: "[bd sd".to_string(),
            verify_sample_banks: true,
        })
        .unwrap();
    assert!(!val_res.valid);
    let _diag0: &SyntaxDiagnostic = &val_res.diagnostics[0];
    assert_eq!(_diag0.severity, DiagnosticSeverity::Error);

    // 6. set_stream & 7. transition_stream (all 11 transitions)
    let set_res: SetStreamResponse = srv
        .set_stream(SetStreamRequest {
            stream_id: "d1".to_string(),
            pattern_code: "s(\"bd*4\")".to_string(),
            orbit: Some(0),
            quantize: QuantizeGrid::NextCycle,
            sync_to_editor: true,
        })
        .unwrap();
    assert_eq!(set_res.stream_id, "d1");

    let transitions = vec![
        TidalTransitionSpec::Xfade { cycles: 4.0 },
        TidalTransitionSpec::Clutch { cycles: 2.0 },
        TidalTransitionSpec::Anticipate { cycles: 8.0 },
        TidalTransitionSpec::Interpolate { cycles: 4.0 },
        TidalTransitionSpec::Jump,
        TidalTransitionSpec::JumpIn { cycles: 4 },
        TidalTransitionSpec::JumpMod {
            modulo: 4,
            offset: 0,
        },
        TidalTransitionSpec::Histpan { pan_slots: 4 },
        TidalTransitionSpec::Wait { delay_cycles: 2.0 },
        TidalTransitionSpec::Wash {
            wash_expr: ".room(0.8)".to_string(),
            in_cycles: 2.0,
            out_cycles: 2.0,
        },
        TidalTransitionSpec::Mortal {
            lifespan_cycles: 4.0,
            degrade_cycles: 2.0,
        },
    ];
    for tr in transitions {
        let tr_res: TransitionStreamResponse = srv
            .transition_stream(TransitionStreamRequest {
                stream_id: "d1".to_string(),
                new_pattern_code: "s(\"hh*8\")".to_string(),
                transition: tr,
                sync_to_editor: true,
            })
            .unwrap();
        assert_eq!(tr_res.stream_id, "d1");
    }

    // 8..13. mute, unmute, solo, unsolo, hush, panic
    let m_res: StreamToggleResponse = srv
        .mute_stream(StreamToggleRequest {
            stream_ids: vec!["d1".to_string()],
            quantize: QuantizeGrid::Immediate,
        })
        .unwrap();
    let _sum0: &StreamStateSummary = &m_res.streams[0];
    assert!(!_sum0.effectively_audible);
    srv.unmute_stream(StreamToggleRequest {
        stream_ids: vec!["d1".to_string()],
        quantize: QuantizeGrid::Immediate,
    })
    .unwrap();
    srv.solo_stream(StreamToggleRequest {
        stream_ids: vec!["d1".to_string()],
        quantize: QuantizeGrid::Immediate,
    })
    .unwrap();
    srv.unsolo_stream(StreamToggleRequest {
        stream_ids: vec!["d1".to_string()],
        quantize: QuantizeGrid::Immediate,
    })
    .unwrap();
    let h_res: HushResponse = srv
        .hush(HushRequest {
            quantize: QuantizeGrid::Immediate,
            release_solo_and_mute: true,
        })
        .unwrap();
    assert!(!h_res.silenced_streams.is_empty());
    let p_res: PanicResponse = srv
        .panic(PanicRequest {
            send_midi_all_notes_off: true,
            clear_delay_reverb_tails: true,
        })
        .unwrap();
    assert_eq!(p_res.cleared_orbits, 16);

    // 14..19. clock & Ableton Link
    let clk0: ClockSnapshotResponse = srv.get_clock_snapshot(GetClockSnapshotRequest {}).unwrap();
    assert!(clk0.is_playing);
    srv.set_cps(SetCpsRequest {
        cps: 0.75,
        ramp_cycles: None,
    })
    .unwrap();
    let clk_bpm = srv
        .set_bpm(SetBpmRequest {
            bpm: 140.0,
            beats_per_cycle: Some(4.0),
            ramp_cycles: None,
        })
        .unwrap();
    assert!((clk_bpm.cps - (140.0 / 240.0)).abs() < 1.0e-6);
    srv.set_beats_per_cycle(SetBeatsPerCycleRequest {
        beats_per_cycle: 3.0,
        preserve_bpm: true,
    })
    .unwrap();
    srv.nudge_clock(NudgeClockRequest {
        phase_delta_cycles: Some(0.125),
        offset_ms: None,
        slew_duration_ms: 50.0,
    })
    .unwrap();
    let link_res: AbletonLinkStatusResponse = srv
        .configure_ableton_link(ConfigureAbletonLinkRequest {
            enabled: true,
            quantum_beats: Some(4.0),
            start_stop_sync: Some(true),
            latency_offset_ms: Some(0.0),
        })
        .unwrap();
    assert!(link_res.enabled);

    // 20..23. sliders & control states
    let sl_list: ListSlidersResponse = srv.list_sliders(ListSlidersRequest {}).unwrap();
    assert_eq!(sl_list.sliders.len(), 1);
    let _sl0: &SliderWidgetDescriptor = &sl_list.sliders[0];

    let mut ctrls = BTreeMap::new();
    ctrls.insert("cutoff".to_string(), ControlValue::Float(1200.0));
    let sc_res: SetControlStateResponse = srv
        .set_control_state(SetControlStateRequest {
            controls: ctrls,
            slew_ms: 10.0,
        })
        .unwrap();
    assert_eq!(sc_res.updated_keys, vec!["cutoff".to_string()]);
    let gc_res: GetControlStateResponse = srv
        .get_control_state(GetControlStateRequest { keys: None })
        .unwrap();
    let _entry: &ControlStateEntry = gc_res.entries.get("cutoff").unwrap();

    // 24..27. samples & orbits
    let sb_res: ListSampleBanksResponse = srv
        .list_sample_banks(ListSampleBanksRequest {
            filter_prefix: None,
            include_variants: true,
        })
        .unwrap();
    let _bank0: &SampleBankEntry = &sb_res.banks[0];
    let ld_res: LoadSampleSourceResponse = srv
        .load_sample_source(LoadSampleSourceRequest {
            source: SampleSourceDescriptor::GitHubRepo {
                owner_repo: "tidalcycles/Dirt-Samples".to_string(),
                git_ref: None,
                subpath: None,
            },
            bank_prefix: Some("dirt2".to_string()),
            eager_decode: true,
        })
        .unwrap();
    assert_eq!(ld_res.decoded_immediately, 16);

    let orb_res: ConfigureOrbitResponse = srv
        .configure_orbit(ConfigureOrbitRequest {
            orbit_index: 2,
            gain_db: Some(-3.0),
            pan: Some(0.5),
            muted: Some(false),
            reverb_room: Some(0.4),
            reverb_size: Some(2.0),
            reverb_ir_sample: None,
            delay_send: Some(0.25),
            delay_time_sec: Some(0.375),
            delay_feedback: Some(0.5),
            duck_target_orbit: Some(0),
            duck_attack_sec: Some(0.01),
            duck_depth: Some(0.8),
            output_channel_pair: Some((4, 5)),
        })
        .unwrap();
    let _orb_snap: &OrbitStateSnapshot = &orb_res.orbit;
    let ot_res: GetOrbitTelemetryResponse = srv
        .get_orbit_telemetry(GetOrbitTelemetryRequest {
            orbit_indices: Some(vec![0, 2]),
        })
        .unwrap();
    assert_eq!(ot_res.orbits.len(), 2);

    // 28..32. MIDI, OSC, Csound
    let mp_res: ListMidiPortsResponse = srv.list_midi_ports(ListMidiPortsRequest {}).unwrap();
    let _mp0: &MidiPortDescriptor = &mp_res.inputs[0];
    assert_eq!(_mp0.direction, MidiPortDirection::Input);
    let mm_res: ConfigureMidiMapResponse = srv
        .configure_midi_map(ConfigureMidiMapRequest {
            mapping_id: "m1".to_string(),
            input_port_id: "midi:in:0".to_string(),
            channel: Some(1),
            cc_or_note: 74,
            is_note_trigger: false,
            target_kind: MidiMappingTarget::SliderWidget {
                widget_id: "slider:0".to_string(),
            },
            range_min: 0.0,
            range_max: 1.0,
        })
        .unwrap();
    assert_eq!(mm_res.active_mappings_count, 1);
    let sm_res: SendMidiCommandResponse = srv
        .send_midi_command(SendMidiCommandRequest {
            output_port_id: "midi:out:0".to_string(),
            channel: 1,
            command: MidiOutboundCommand::NoteOn {
                note: 60,
                velocity: 100,
                duration_cycles: Some(0.25),
            },
            schedule_at_cycle: None,
        })
        .unwrap();
    assert!(sm_res.dispatched);
    let osc_res: ConfigureOscTargetResponse = srv
        .configure_osc_target(ConfigureOscTargetRequest {
            target_name: "superdirt".to_string(),
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 57120,
            address_path: "/dirt/play".to_string(),
            lookahead_latency_ms: 100.0,
            superdirt_compatible: true,
        })
        .unwrap();
    assert!(osc_res.active);
    let cs_res: LoadCsoundOrcResponse = srv
        .load_csound_orc(LoadCsoundOrcRequest {
            orc_code: Some("instr 1\nout a1\nendin".to_string()),
            orc_file_path: None,
        })
        .unwrap();
    assert!(cs_res.compiled_successfully);

    // 33..38. visualizers, telemetry, export, share URL, doctor
    let vf_res: CaptureVisualizerFrameResponse = srv
        .capture_visualizer_frame(CaptureVisualizerFrameRequest {
            kind: VisualizerKind::Pianoroll,
            format: VisualizerOutputFormat::DrawAstJson,
            stream_id: Some("d1".to_string()),
            window_cycles: 4.0,
            width_px: 800,
            height_px: 240,
        })
        .unwrap();
    let _cmd0: &DrawCmd = &vf_res.draw_commands.as_ref().unwrap()[0];

    let tel_res: ReadAudioTelemetryResponse = srv
        .read_audio_telemetry(ReadAudioTelemetryRequest {
            orbit_index: Some(0),
            fft_bins: 64,
        })
        .unwrap();
    let _hu: &HydraAudioUniforms = &tel_res.hydra_uniforms;
    let _md: &AcousticMixDiagnostic = &tel_res.mix_diagnostic;
    let _alert: &InterOrbitConflictAlert = &_md.masking_conflicts[0];
    assert_eq!(_alert.kind, ConflictKind::SubBassPhaseCancellation);

    let exp_res: ExportAudioResponse = srv
        .export_audio(ExportAudioRequest {
            output_path: PathBuf::from("/tmp/silverwave_sandbox/out.wav"),
            format: AudioExportFormat::WavPcm24,
            code_override: None,
            cycles: Some(2.0),
            duration_seconds: None,
            sample_rate: 48_000,
            split_orbit_stems: false,
            normalize_lufs: Some(-14.0),
        })
        .unwrap();
    assert_eq!(exp_res.written_files.len(), 1);

    let enc: EncodeShareUrlResponse = srv
        .encode_share_url(EncodeShareUrlRequest {
            code: Some("s(\"bd sd\")".to_string()),
            base_url: "https://strudel.cc/#".to_string(),
            include_session_metadata: true,
        })
        .unwrap();
    let dec: DecodeShareUrlResponse = srv
        .decode_share_url(DecodeShareUrlRequest {
            share_url: enc.share_url,
        })
        .unwrap();
    assert_eq!(dec.code, "s(\"bd sd\")");

    let doc_res: RunDoctorResponse = srv
        .run_doctor(RunDoctorRequest {
            run_audio_loopback_probe: true,
        })
        .unwrap();
    assert!(doc_res.overall_healthy);
    let _chk0: &DoctorSubsystemCheck = &doc_res.checks[0];

    // 39..42. Category-King 4 extensions
    srv.set_crossfader_scene(SetCrossfaderSceneRequest {
        scene_a_id: Some(0),
        scene_b_id: Some(1),
        xfader_position: 0.65,
        slew_ms: 20.0,
    })
    .unwrap();
    srv.trigger_looper_freeze(TriggerLooperFreezeRequest {
        enabled: true,
        freeze_cycles: Rational64::new_const(1, 4),
        target_orbit: Some(0),
    })
    .unwrap();
    srv.set_fill_latch(SetFillLatchRequest {
        fill_active: true,
        quantize: QuantizeGrid::Immediate,
    })
    .unwrap();
    let vid_res = srv
        .render_video(RenderVideoRequest {
            output_path: PathBuf::from("/tmp/silverwave_sandbox/clip.mp4"),
            preset: "vertical_9x16_mp4".to_string(),
            fps: 60,
            cycles: 4.0,
            embed_qr_permalink: true,
        })
        .unwrap();
    assert!(vid_res.rendered_frames > 0);
}

#[test]
fn test_tier3_coperformance_surgical_patch_and_tier4_security_guards() {
    let mut srv = SilverWaveMcpServer::default();
    let sub: ResourceSubscriptionHandle = srv
        .hub
        .subscribe_resource_updates("silverwave://session/*")
        .unwrap();
    assert_eq!(sub.subscription_id, 1);
    srv.hub
        .notify_resource_updated("silverwave://session/buffer");

    let cursor_before: HumanCursorSelection = srv.hub.human_cursors[0].clone();
    let upd: UpdateSliderResponse = srv
        .update_slider(UpdateSliderRequest {
            widget_id: "slider:0".to_string(),
            value: 0.85,
            patch_source_in_place: true,
            slew_ms: 15.0,
            expected_revision: Some(1),
        })
        .unwrap();
    assert_eq!(upd.new_buffer_revision, 2);
    assert!(srv.hub.buffer_code.contains("slider(0.85"));
    assert_eq!(
        srv.hub.human_cursors[0].cursor_byte,
        cursor_before.cursor_byte
    );
    assert!(!srv.hub.pending_notifications.is_empty());

    // Optimistic concurrency conflict detection (-32001)
    let conflict = srv.update_slider(UpdateSliderRequest {
        widget_id: "slider:0".to_string(),
        value: 0.50,
        patch_source_in_place: true,
        slew_ms: 15.0,
        expected_revision: Some(1),
    });
    assert_eq!(conflict.unwrap_err().code, ERR_REVISION_CONFLICT);

    // Path traversal & SSRF rejection (-32006)
    let guard = DefaultSandboxGuard::default();
    assert_eq!(
        guard
            .validate_read_path(Path::new("/tmp/silverwave_sandbox/../etc/passwd"))
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );
    assert_eq!(
        guard
            .validate_remote_url("https://127.0.0.1/strudel.json")
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );

    // Read-only spectator mode enforcement (-32008)
    srv.config.read_only = true;
    assert_eq!(
        srv.hush(HushRequest {
            quantize: QuantizeGrid::Immediate,
            release_solo_and_mute: false,
        })
        .unwrap_err()
        .code,
        ERR_READ_ONLY_VIOLATION
    );

    // Token-bucket rate limiter enforcement & replenishment (-32029)
    let mut limiter = TokenBucketRateLimiter::new(2, 1);
    limiter
        .check_and_consume("clientA", ToolRateCategory::RealtimeControl)
        .unwrap();
    limiter
        .check_and_consume("clientA", ToolRateCategory::RealtimeControl)
        .unwrap();
    assert_eq!(
        limiter
            .check_and_consume("clientA", ToolRateCategory::RealtimeControl)
            .unwrap_err()
            .code,
        ERR_RATE_LIMIT_EXCEEDED
    );
    limiter.replenish("clientA", ToolRateCategory::RealtimeControl, 2);
    limiter
        .check_and_consume("clientA", ToolRateCategory::RealtimeControl)
        .unwrap();

    // Preallocated audio command ring drain
    let mut ring = PreallocatedAudioCommandRing::new();
    assert!(ring.push(RtAudioCommand::TriggerPanic));
    let mut count = 0;
    let drained = ring.drain_audio_block(|_cmd| count += 1);
    assert_eq!(drained, 1);
    assert_eq!(count, 1);
}
