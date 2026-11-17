//! Supplementary coverage suite for `silverwave-mcp` (`v0.7.0`).
//! Exercises error branches and fallback paths across `protocol.rs`,
//! `resources.rs`, and `security.rs` that the primary TDD suite does not reach.

#![forbid(unsafe_code)]

use silverwave_core::Rational64;
use silverwave_mcp::{
    ActorOrigin, CaptureVisualizerFrameRequest, CoPerformanceBridge, CoPerformanceSessionHub,
    ConfigureOrbitRequest, ControlValue, DecodeShareUrlRequest, DefaultSandboxGuard,
    ERR_DSL_SYNTAX, ERR_INVALID_PARAMS, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, ERR_PARSE_ERROR,
    ERR_SANDBOX_VIOLATION, ERR_WIDGET_NOT_FOUND, EncodeShareUrlRequest, EvaluateBlockRequest,
    EvaluateBufferRequest, GetControlStateRequest, HushRequest, ListSampleBanksRequest,
    LoadCsoundOrcRequest, LoadSampleSourceRequest, MAX_COMMANDS_PER_AUDIO_BLOCK, MCP_RESOURCE_URIS,
    McpCliArgs, NudgeClockRequest, ParseMiniNotationRequest, PathSandboxGuard,
    PreallocatedAudioCommandRing, QuantizeGrid, QueryPatternArcRequest, RateLimiterPolicy,
    RtAudioCommand, SampleSourceDescriptor, SetBeatsPerCycleRequest, SetBpmRequest,
    SetControlStateRequest, SetStreamRequest, SilverWaveMcpPromptSurface, SilverWaveMcpServer,
    SilverWaveMcpToolSurface, SilverWavePromptId, StreamToggleRequest, SurgicalTextEdit,
    TokenBucketRateLimiter, ToolRateCategory, UpdateSliderRequest, ValidateSyntaxRequest,
    VisualizerKind, VisualizerOutputFormat,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn agent_origin() -> ActorOrigin {
    ActorOrigin::McpAgent {
        session_id: "mcp:cov".to_string(),
        client_name: "cov-agent".to_string(),
        request_id: "cov-1".to_string(),
    }
}

// ----------------------------------------------------------------------------
// protocol.rs — snapshot_buffer (268-270), out-of-bounds edit error (293-296),
// cursor shifting past the edit span (305-307).
// ----------------------------------------------------------------------------
#[test]
fn protocol_snapshot_and_surgical_edit_edges() {
    let mut hub = CoPerformanceSessionHub::new("d1 $ s(\"bd sd\")");

    // snapshot_buffer (268-270)
    let (rev, code) = hub.snapshot_buffer();
    assert_eq!(rev, 1);
    assert_eq!(code, "d1 $ s(\"bd sd\")");

    // byte_range.end beyond buffer length -> -32004 (293-296)
    let too_long = SurgicalTextEdit {
        base_revision: rev,
        new_revision: rev + 1,
        byte_range: 0..(code.len() + 10),
        line_col_start: (1, 1),
        line_col_end: (1, 1),
        replacement_text: "x".to_string(),
        origin: agent_origin(),
    };
    let err = hub
        .apply_buffer_mutation(vec![too_long], None, agent_origin())
        .unwrap_err();
    assert_eq!(err.code, ERR_WIDGET_NOT_FOUND);
    assert!(err.message.contains("out of bounds"));

    // start > end -> also -32004 (293-296 second condition)
    let inverted = SurgicalTextEdit {
        base_revision: rev,
        new_revision: rev + 1,
        byte_range: 10..5,
        line_col_start: (1, 1),
        line_col_end: (1, 1),
        replacement_text: "y".to_string(),
        origin: agent_origin(),
    };
    assert_eq!(
        hub.apply_buffer_mutation(vec![inverted], None, agent_origin())
            .unwrap_err()
            .code,
        ERR_WIDGET_NOT_FOUND
    );

    // A positive-delta edit strictly before the human cursor shifts it (305-307).
    // Default cursor sits at byte 4; editing bytes 0..2 (+3 chars) moves it to 7.
    let grow = SurgicalTextEdit {
        base_revision: rev,
        new_revision: rev + 1,
        byte_range: 0..2,
        line_col_start: (1, 1),
        line_col_end: (1, 3),
        replacement_text: "xxxxx".to_string(),
        origin: agent_origin(),
    };
    let new_rev = hub
        .apply_buffer_mutation(vec![grow], None, agent_origin())
        .unwrap();
    assert_eq!(new_rev, 2);
    assert_eq!(hub.human_cursors[0].cursor_byte, 7);
    assert_eq!(hub.human_cursors[0].selection_end_byte, 7);
    assert!(hub.buffer_code.starts_with("xxxxx"));
}

// ----------------------------------------------------------------------------
// security.rs — NUL/device prefix check (54-57), root containment success (74)
// and failure (76-82), read-path delegation (89), dotfile refusal (97-100),
// export extension refusal (108-111), network disabled (118-121),
// non-HTTPS refusal (124-127).
// ----------------------------------------------------------------------------
#[test]
fn sandbox_guard_malicious_and_edge_inputs() {
    let guard = DefaultSandboxGuard::default();

    // NUL byte injection -> -32006 (54-57)
    assert_eq!(
        guard
            .validate_read_path(Path::new("evil\0path"))
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );
    // Windows device namespace prefix -> -32006 (54-57)
    assert_eq!(
        guard
            .validate_read_path(Path::new(r"\\.\pipe\silverwave"))
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );

    // Happy path under an allowed root (71-74, 89)
    let ok = guard
        .validate_read_path(Path::new("/tmp/silverwave_sandbox/audio/kick.wav"))
        .unwrap();
    assert_eq!(ok, PathBuf::from("/tmp/silverwave_sandbox/audio/kick.wav"));

    // Outside every configured root -> -32006 (76-82)
    assert_eq!(
        guard
            .validate_read_path(Path::new("/etc/passwd"))
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );

    // Hidden dotfile export refused (96-100)
    assert_eq!(
        guard
            .validate_export_path(Path::new("/tmp/silverwave_sandbox/.secret.wav"))
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );

    // Non-allowlisted export extension refused (107-111)
    assert_eq!(
        guard
            .validate_export_path(Path::new("/tmp/silverwave_sandbox/render.exe"))
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );
    // Missing extension also refused
    assert_eq!(
        guard
            .validate_export_path(Path::new("/tmp/silverwave_sandbox/render"))
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );
    // Allowed extensions pass
    for ext in ["wav", "flac", "mp4", "webm", "WAV"] {
        let p = PathBuf::from(format!("/tmp/silverwave_sandbox/out.{ext}"));
        assert!(guard.validate_export_path(&p).is_ok());
    }

    // External network disabled -> -32006 (118-121)
    let guard_off = DefaultSandboxGuard::new(vec![PathBuf::from("/tmp/silverwave_sandbox")], false);
    assert_eq!(
        guard_off
            .validate_remote_url("https://example.com/manifest.json")
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );

    // Non-HTTPS scheme refused (123-127)
    assert_eq!(
        guard
            .validate_remote_url("http://example.com/manifest.json")
            .unwrap_err()
            .code,
        ERR_SANDBOX_VIOLATION
    );

    // SSRF: loopback / link-local / RFC-1918 hosts refused
    for url in [
        "https://localhost/x",
        "https://127.0.0.1:8443/x",
        "https://10.1.2.3/x",
        "https://192.168.0.9/x",
        "https://169.254.169.254/latest/meta-data",
        "https://172.16.0.1/x",
    ] {
        assert_eq!(
            guard.validate_remote_url(url).unwrap_err().code,
            ERR_SANDBOX_VIOLATION,
            "expected SSRF rejection for {url}"
        );
    }

    // Bracketed IPv6 loopback and IPv4-mapped IPv6 forms must also be rejected.
    for url in [
        "https://[::1]/x",
        "https://[::1]:8443/x",
        "https://[::ffff:127.0.0.1]/x",
        "https://[::ffff:10.0.0.5]/x",
    ] {
        assert_eq!(
            guard.validate_remote_url(url).unwrap_err().code,
            ERR_SANDBOX_VIOLATION,
            "expected SSRF rejection for {url}"
        );
    }

    // Public HTTPS host passes
    assert_eq!(
        guard
            .validate_remote_url("https://example.com/m.json")
            .unwrap(),
        "https://example.com/m.json"
    );
}

// ----------------------------------------------------------------------------
// security.rs — TokenBucketRateLimiter::default (171-173),
// PreallocatedAudioCommandRing::default (239-241), full ring push -> false (258),
// drain callback invocation (272).
// ----------------------------------------------------------------------------
#[test]
fn rate_limiter_default_and_ring_saturation() {
    let mut limiter = TokenBucketRateLimiter::default();
    assert_eq!(limiter.capacity, 240);
    limiter
        .check_and_consume("cov-client", ToolRateCategory::ReadOnlyQuery)
        .unwrap();

    let mut ring = PreallocatedAudioCommandRing::default();
    for _ in 0..MAX_COMMANDS_PER_AUDIO_BLOCK {
        assert!(ring.push(RtAudioCommand::TriggerPanic));
    }
    // 65th push must fail without allocation (257-258)
    assert!(!ring.push(RtAudioCommand::SetCps(2.0)));

    // Drain invokes the callback for each queued command (269-275)
    let mut seen = 0usize;
    let drained = ring.drain_audio_block(|cmd| {
        seen += 1;
        assert_eq!(cmd, RtAudioCommand::TriggerPanic);
    });
    assert_eq!(drained, MAX_COMMANDS_PER_AUDIO_BLOCK);
    assert_eq!(seen, drained);
    // Ring is now empty; draining again yields zero.
    assert_eq!(ring.drain_audio_block(|_| {}), 0);
}

// ----------------------------------------------------------------------------
// resources.rs — read_resource_json unsupported scheme (219-222),
// handle_jsonrpc_payload parse error (238-241), missing version (244-247),
// spaced initialize (252), unknown method (259-263).
// ----------------------------------------------------------------------------
#[test]
fn resource_read_and_jsonrpc_error_paths() {
    let mut srv = SilverWaveMcpServer::default();

    // Unsupported URI scheme -> -32602 (218-222)
    let err = srv.read_resource_json("file:///etc/passwd").unwrap_err();
    assert_eq!(err.code, ERR_INVALID_PARAMS);
    assert!(err.message.contains("Unsupported resource URI"));

    // Every canonical silverwave:// URI reads back a JSON envelope.
    for uri in MCP_RESOURCE_URIS {
        let json = srv.read_resource_json(uri).unwrap();
        assert!(json.contains(&format!("\"uri\":\"{uri}\"")));
        assert!(json.contains("\"revision\":1"));
    }

    // Non-object framing -> -32700 (237-241)
    assert_eq!(
        srv.handle_jsonrpc_payload("c1", "[1,2,3]")
            .unwrap_err()
            .code,
        ERR_PARSE_ERROR
    );
    // Object without jsonrpc version -> -32600 (243-247)
    assert_eq!(
        srv.handle_jsonrpc_payload("c1", "{\"method\":\"initialize\"}")
            .unwrap_err()
            .code,
        ERR_INVALID_REQUEST
    );

    // JSON-RPC with whitespace around colons still initializes (251-258)
    let ok = srv
        .handle_jsonrpc_payload(
            "c1",
            "{ \"jsonrpc\": \"2.0\", \"id\": 7, \"method\": \"initialize\" }",
        )
        .unwrap();
    assert!(ok.contains("\"count\":42"));
    assert!(ok.contains("\"count\":10"));

    // Well-formed envelope, unknown method -> -32601 (260-263)
    assert_eq!(
        srv.handle_jsonrpc_payload("c1", "{\"jsonrpc\":\"2.0\",\"method\":\"tools/call\"}")
            .unwrap_err()
            .code,
        ERR_METHOD_NOT_FOUND
    );
}

// ----------------------------------------------------------------------------
// resources.rs — get_prompt dispatch line (373) for all five prompt IDs, plus
// the `style` argument defaulting branch (377).
// ----------------------------------------------------------------------------
#[test]
fn prompt_surface_renders_all_five_prompts() {
    let srv = SilverWaveMcpServer::default();
    assert_eq!(srv.list_prompts().len(), 5);

    let ids = [
        (SilverWavePromptId::LiveCodeSession, "live_code_session"),
        (SilverWavePromptId::ComposePattern, "compose_pattern"),
        (
            SilverWavePromptId::TransitionArrangement,
            "transition_arrangement",
        ),
        (SilverWavePromptId::SoundDesignSynth, "sound_design_synth"),
        (
            SilverWavePromptId::DiagnoseAudioSetup,
            "diagnose_audio_setup",
        ),
    ];
    for (id, expected_name) in ids {
        let mut args = BTreeMap::new();
        args.insert("style".to_string(), "ambient".to_string());
        let payload = srv.get_prompt(id, args).unwrap();
        assert_eq!(payload.prompt_name, expected_name);
        assert!(payload.system_instructions.contains("style=ambient"));
        assert!(!payload.embedded_resource_uris.is_empty());
        for uri in &payload.embedded_resource_uris {
            assert!(uri.starts_with("silverwave://"));
        }
    }

    // Missing `style` argument falls back to "algorave" (377).
    let payload = srv
        .get_prompt(SilverWavePromptId::ComposePattern, BTreeMap::new())
        .unwrap();
    assert!(payload.system_instructions.contains("style=algorave"));
}

// ----------------------------------------------------------------------------
// resources.rs — evaluate_buffer DSL error (394-397) and sync_to_editor block
// (399-419), evaluate_block default stream (441), parse_mini_notation error
// (466-469), validate_syntax diagnostic block (568-583), and the unterminated
// slider() break in discover_sliders_in_code (309).
// ----------------------------------------------------------------------------
#[test]
fn evaluation_and_notation_error_paths() {
    let mut srv = SilverWaveMcpServer::default();

    // Unclosed "[[" mini-notation bracket -> -32002 (393-397)
    let err = srv
        .evaluate_buffer(EvaluateBufferRequest {
            code: Some("d1 $ s(\"[[bd sd\")".to_string()),
            sync_to_editor: false,
            expected_revision: None,
            quantize: QuantizeGrid::Immediate,
            auto_start_clock: false,
        })
        .unwrap_err();
    assert_eq!(err.code, ERR_DSL_SYNTAX);
    assert!(err.message.contains("Unclosed"));

    // sync_to_editor=true applies the surgical buffer mutation (399-419)
    let res = srv
        .evaluate_buffer(EvaluateBufferRequest {
            code: Some("d1 $ s(\"cp hh\")".to_string()),
            sync_to_editor: true,
            expected_revision: None,
            quantize: QuantizeGrid::NextBar,
            auto_start_clock: true,
        })
        .unwrap();
    assert_eq!(res.revision, 2);
    assert_eq!(srv.hub.buffer_code, "d1 $ s(\"cp hh\")");

    // evaluate_block with no target_stream_override defaults to "d1" (439-441)
    let blk = srv
        .evaluate_block(EvaluateBlockRequest {
            cursor_line: None,
            line_range: None,
            byte_range: None,
            target_stream_override: None,
            quantize: QuantizeGrid::CycleMultiple(2.0),
        })
        .unwrap();
    assert_eq!(blk.target_stream, "d1");
    assert_eq!(blk.highlight_spans.len(), 1);

    // Unclosed '[' in notation -> -32002 (465-469)
    assert_eq!(
        srv.parse_mini_notation(ParseMiniNotationRequest {
            notation: "[bd sd".to_string(),
            base_byte_offset: 0,
            base_line: 1,
            base_col: 1,
        })
        .unwrap_err()
        .code,
        ERR_DSL_SYNTAX
    );

    // Invalid code produces an E0001 diagnostic with a suggested fix (568-583)
    let v = srv
        .validate_syntax(ValidateSyntaxRequest {
            code: "s(\"[bd\"".to_string(),
            verify_sample_banks: false,
        })
        .unwrap();
    assert!(!v.valid);
    assert_eq!(v.diagnostics[0].code, "E0001");
    assert_eq!(
        v.diagnostics[0].suggested_replacement.as_deref(),
        Some("s(\"[bd\"]")
    );

    // Unterminated `slider(` call hits the `else { break }` arm (308-309)
    let v2 = srv
        .validate_syntax(ValidateSyntaxRequest {
            code: "d1 $ lpf(slider(0.7".to_string(),
            verify_sample_banks: false,
        })
        .unwrap();
    assert!(v2.valid);
    assert!(v2.sliders.is_empty());
}

// ----------------------------------------------------------------------------
// resources.rs — query_pattern_arc stream-expression lookup (519-522), the
// "bd sd" fallback (524), and the max_events break (533-534).
// ----------------------------------------------------------------------------
#[test]
fn pattern_arc_fallbacks_and_max_events_break() {
    let mut srv = SilverWaveMcpServer::default();
    srv.set_stream(SetStreamRequest {
        stream_id: "d7".to_string(),
        pattern_code: "hh cp".to_string(),
        orbit: None,
        quantize: QuantizeGrid::Immediate,
        sync_to_editor: false,
    })
    .unwrap();

    // pattern_code=None + registered stream_id -> stream_expressions lookup (519-522)
    let from_stream = srv
        .query_pattern_arc(QueryPatternArcRequest {
            pattern_code: None,
            stream_id: Some("d7".to_string()),
            begin_cycle: Rational64::from_integer(0),
            end_cycle: Rational64::from_integer(1),
            max_events: 16,
            deterministic_seed: 7,
        })
        .unwrap();
    assert_eq!(from_stream.total_events, 2);

    // Unknown stream_id -> falls through to the "bd sd" default (523-524)
    let unknown_stream = srv
        .query_pattern_arc(QueryPatternArcRequest {
            pattern_code: None,
            stream_id: Some("d99".to_string()),
            begin_cycle: Rational64::from_integer(0),
            end_cycle: Rational64::from_integer(1),
            max_events: 16,
            deterministic_seed: 7,
        })
        .unwrap();
    assert_eq!(unknown_stream.total_events, 2);

    // No stream_id at all -> same default (524)
    let no_stream = srv
        .query_pattern_arc(QueryPatternArcRequest {
            pattern_code: None,
            stream_id: None,
            begin_cycle: Rational64::from_integer(2),
            end_cycle: Rational64::from_integer(3),
            max_events: 16,
            deterministic_seed: 7,
        })
        .unwrap();
    assert_eq!(no_stream.total_events, 2);

    // max_events smaller than the token count triggers the early break (533-534)
    let truncated = srv
        .query_pattern_arc(QueryPatternArcRequest {
            pattern_code: Some("bd sd cp hh".to_string()),
            stream_id: None,
            begin_cycle: Rational64::from_integer(0),
            end_cycle: Rational64::from_integer(1),
            max_events: 2,
            deterministic_seed: 7,
        })
        .unwrap();
    assert_eq!(truncated.total_events, 2);
}

// ----------------------------------------------------------------------------
// resources.rs — hush release_solo_and_mute loops (700-707) and the empty
// mute-map fallback to ["d1"] (712-713).
// ----------------------------------------------------------------------------
#[test]
fn hush_release_loops_and_empty_fallback() {
    let mut srv = SilverWaveMcpServer::default();

    // Populate mute/solo maps, then release them (701-706)
    srv.mute_stream(StreamToggleRequest {
        stream_ids: vec!["d1".to_string()],
        quantize: QuantizeGrid::Immediate,
    })
    .unwrap();
    srv.solo_stream(StreamToggleRequest {
        stream_ids: vec!["d1".to_string()],
        quantize: QuantizeGrid::Immediate,
    })
    .unwrap();
    let h = srv
        .hush(HushRequest {
            quantize: QuantizeGrid::Immediate,
            release_solo_and_mute: true,
        })
        .unwrap();
    assert!(srv.stream_muted.values().all(|v| !*v));
    assert!(srv.stream_soloed.values().all(|v| !*v));
    assert!(h.silenced_streams.contains(&"d1".to_string()));

    // With an empty mute map, hush falls back to silencing "d1" (712-713)
    srv.stream_muted.clear();
    let h2 = srv
        .hush(HushRequest {
            quantize: QuantizeGrid::Immediate,
            release_solo_and_mute: false,
        })
        .unwrap();
    assert_eq!(h2.silenced_streams, vec!["d1".to_string()]);
}

// ----------------------------------------------------------------------------
// resources.rs — set_bpm beats_per_cycle branch (776-778),
// set_beats_per_cycle preserve_bpm branch (793-795),
// nudge_clock phase_delta branch (806-810).
// ----------------------------------------------------------------------------
#[test]
fn clock_mutation_optional_branches() {
    let mut srv = SilverWaveMcpServer::default();

    // beats_per_cycle supplied -> clamped and applied (776-778)
    let snap = srv
        .set_bpm(SetBpmRequest {
            bpm: 120.0,
            beats_per_cycle: Some(8.0),
            ramp_cycles: None,
        })
        .unwrap();
    assert!((snap.beats_per_cycle - 8.0).abs() < f64::EPSILON);
    assert!((snap.cps - (120.0 / 60.0 / 8.0)).abs() < 1.0e-9);

    // preserve_bpm=true recomputes cps (793-795)
    let before_bpm = snap.cps * 60.0 * snap.beats_per_cycle;
    let snap2 = srv
        .set_beats_per_cycle(SetBeatsPerCycleRequest {
            beats_per_cycle: 3.0,
            preserve_bpm: true,
        })
        .unwrap();
    let after_bpm = snap2.cps * 60.0 * snap2.beats_per_cycle;
    assert!((before_bpm - after_bpm).abs() < 1.0e-6);

    // phase_delta_cycles nudges the rational active cycle (806-810)
    let nudged = srv
        .nudge_clock(NudgeClockRequest {
            phase_delta_cycles: Some(0.5),
            offset_ms: Some(5.0),
            slew_duration_ms: 25.0,
        })
        .unwrap();
    assert!(nudged.cycle_f64 >= 0.0);
    assert!((srv.hub.active_cycle.to_f64() - 0.5).abs() < 1.0e-6);
}

// ----------------------------------------------------------------------------
// resources.rs — update_slider widget-not-found error (854-856) and the
// patch_source_in_place span bookkeeping (863-893).
// ----------------------------------------------------------------------------
#[test]
fn update_slider_not_found_and_in_place_patch() {
    let mut srv = SilverWaveMcpServer::default();

    // Default buffer has exactly one slider() -> "slider:99" is unknown (851-856)
    let err = srv
        .update_slider(UpdateSliderRequest {
            widget_id: "slider:99".to_string(),
            value: 0.1,
            patch_source_in_place: false,
            slew_ms: 0.0,
            expected_revision: None,
        })
        .unwrap_err();
    assert_eq!(err.code, ERR_WIDGET_NOT_FOUND);
    assert!(err.message.contains("Slider widget not found"));

    // patch_source_in_place=true rewrites the literal and returns a span (863-893)
    let res = srv
        .update_slider(UpdateSliderRequest {
            widget_id: "slider:0".to_string(),
            value: 0.9,
            patch_source_in_place: true,
            slew_ms: 5.0,
            expected_revision: None,
        })
        .unwrap();
    assert!(res.patched_span.is_some());
    assert!(srv.hub.buffer_code.contains("slider(0.90"));
    assert_eq!(res.new_buffer_revision, 2);
}

// ----------------------------------------------------------------------------
// resources.rs — get_control_state key-filter closure (936-943).
// ----------------------------------------------------------------------------
#[test]
fn control_state_filtered_query() {
    let mut srv = SilverWaveMcpServer::default();
    let mut controls = BTreeMap::new();
    controls.insert("cutoff".to_string(), ControlValue::Float(0.5));
    controls.insert("resonance".to_string(), ControlValue::Float(0.8));
    srv.set_control_state(SetControlStateRequest {
        controls,
        slew_ms: 0.0,
    })
    .unwrap();

    // keys filter present -> per-entry containment check (936-942)
    let filtered = srv
        .get_control_state(GetControlStateRequest {
            keys: Some(vec!["cutoff".to_string()]),
        })
        .unwrap();
    assert_eq!(filtered.entries.len(), 1);
    assert!(filtered.entries.contains_key("cutoff"));

    // No filter -> all entries returned (943)
    let all = srv
        .get_control_state(GetControlStateRequest { keys: None })
        .unwrap();
    assert_eq!(all.entries.len(), 2);
}

// ----------------------------------------------------------------------------
// resources.rs — list_sample_banks prefix filter (955-958) and every
// load_sample_source source-kind arm (976-1002).
// ----------------------------------------------------------------------------
#[test]
fn sample_bank_filter_and_all_load_source_arms() {
    let args = McpCliArgs {
        allow_external_network: true,
        ..Default::default()
    };
    let mut srv = SilverWaveMcpServer::new(args);

    // filter_prefix Some -> starts_with predicate (955-959)
    let banks = srv
        .list_sample_banks(ListSampleBanksRequest {
            filter_prefix: Some("dirt".to_string()),
            include_variants: true,
        })
        .unwrap();
    assert_eq!(banks.total_banks, 1);
    let miss = srv
        .list_sample_banks(ListSampleBanksRequest {
            filter_prefix: Some("no-such".to_string()),
            include_variants: false,
        })
        .unwrap();
    assert_eq!(miss.total_banks, 0);
    assert_eq!(miss.total_memory_bytes, 0);

    // StrudelJsonManifest over HTTPS -> validate_remote_url arm (977-979),
    // default bank prefix (984-985)
    let remote = srv
        .load_sample_source(LoadSampleSourceRequest {
            source: SampleSourceDescriptor::StrudelJsonManifest {
                url_or_path: "https://example.com/strudel.json".to_string(),
            },
            bank_prefix: None,
            eager_decode: false,
        })
        .unwrap();
    assert_eq!(
        remote.registered_banks,
        vec!["strudel_manifest".to_string()]
    );
    assert_eq!(remote.decoded_immediately, 0);

    // StrudelJsonManifest local path -> validate_read_path arm (980-982)
    let local_manifest = srv
        .load_sample_source(LoadSampleSourceRequest {
            source: SampleSourceDescriptor::StrudelJsonManifest {
                url_or_path: "/tmp/silverwave_sandbox/manifest.json".to_string(),
            },
            bank_prefix: Some("manifest_bank".to_string()),
            eager_decode: true,
        })
        .unwrap();
    assert_eq!(local_manifest.decoded_immediately, 16);

    // GitHubRepo with no bank_prefix -> owner_repo-derived name (987-991)
    let gh = srv
        .load_sample_source(LoadSampleSourceRequest {
            source: SampleSourceDescriptor::GitHubRepo {
                owner_repo: "tidalcycles/Dirt-Samples".to_string(),
                git_ref: Some("main".to_string()),
                subpath: Some("samples".to_string()),
            },
            bank_prefix: None,
            eager_decode: false,
        })
        .unwrap();
    assert_eq!(
        gh.registered_banks,
        vec!["tidalcycles_Dirt-Samples".to_string()]
    );

    // LocalDirectory arm (993-995)
    let local_dir = srv
        .load_sample_source(LoadSampleSourceRequest {
            source: SampleSourceDescriptor::LocalDirectory {
                path: PathBuf::from("/tmp/silverwave_sandbox/samples"),
            },
            bank_prefix: None,
            eager_decode: false,
        })
        .unwrap();
    assert_eq!(local_dir.registered_banks, vec!["local_bank".to_string()]);

    // SoundFont2 arm with all prefixes unset -> "gm_sf2" (997-1002)
    let sf2 = srv
        .load_sample_source(LoadSampleSourceRequest {
            source: SampleSourceDescriptor::SoundFont2 {
                path: PathBuf::from("/tmp/silverwave_sandbox/gm.sf2"),
                bank_prefix: None,
            },
            bank_prefix: None,
            eager_decode: false,
        })
        .unwrap();
    assert_eq!(sf2.registered_banks, vec!["gm_sf2".to_string()]);
}

// ----------------------------------------------------------------------------
// resources.rs — configure_orbit per-field branches (1027-1052).
// ----------------------------------------------------------------------------
#[test]
fn configure_orbit_all_field_branches() {
    let mut srv = SilverWaveMcpServer::default();
    let res = srv
        .configure_orbit(ConfigureOrbitRequest {
            orbit_index: 3,
            gain_db: Some(-6.0),
            pan: Some(0.25),
            muted: Some(true),
            reverb_room: Some(0.9),
            reverb_size: None,
            reverb_ir_sample: None,
            delay_send: Some(0.4),
            delay_time_sec: None,
            delay_feedback: Some(0.6),
            duck_target_orbit: Some(5),
            duck_attack_sec: None,
            duck_depth: None,
            output_channel_pair: None,
        })
        .unwrap();
    let o = res.orbit;
    assert_eq!(o.gain_db, -6.0);
    assert_eq!(o.pan, 0.25);
    assert!(o.muted);
    assert_eq!(o.reverb_room, 0.9);
    assert_eq!(o.delay_send, 0.4);
    assert_eq!(o.delay_feedback, 0.6);
    assert_eq!(o.duck_target_orbit, Some(5));

    // Fields left as None keep the seeded defaults.
    let res2 = srv
        .configure_orbit(ConfigureOrbitRequest {
            orbit_index: 4,
            gain_db: None,
            pan: None,
            muted: None,
            reverb_room: None,
            reverb_size: None,
            reverb_ir_sample: None,
            delay_send: None,
            delay_time_sec: None,
            delay_feedback: None,
            duck_target_orbit: None,
            duck_attack_sec: None,
            duck_depth: None,
            output_channel_pair: None,
        })
        .unwrap();
    assert_eq!(res2.orbit.gain_db, 0.0);
    assert_eq!(res2.orbit.pan, 0.5);
}

// ----------------------------------------------------------------------------
// resources.rs — load_csound_orc file-path validation (1142-1143).
// ----------------------------------------------------------------------------
#[test]
fn csound_orc_file_path_validation() {
    let mut srv = SilverWaveMcpServer::default();

    // Path inside the sandbox compiles (1142-1143 success)
    let ok = srv
        .load_csound_orc(LoadCsoundOrcRequest {
            orc_code: None,
            orc_file_path: Some(PathBuf::from("/tmp/silverwave_sandbox/kit.orc")),
        })
        .unwrap();
    assert!(ok.compiled_successfully);
    assert_eq!(ok.registered_instruments, vec!["instr_1".to_string()]);

    // Path outside the sandbox is refused by the guard (1143 error propagation)
    let err = srv
        .load_csound_orc(LoadCsoundOrcRequest {
            orc_code: None,
            orc_file_path: Some(PathBuf::from("/etc/passwd")),
        })
        .unwrap_err();
    assert_eq!(err.code, ERR_SANDBOX_VIOLATION);
}

// ----------------------------------------------------------------------------
// resources.rs — capture_visualizer_frame Svg (1174-1186) and PngBase64
// (1187-1191) output arms.
// ----------------------------------------------------------------------------
#[test]
fn visualizer_all_output_formats() {
    let srv = SilverWaveMcpServer::default();

    let svg = srv
        .capture_visualizer_frame(CaptureVisualizerFrameRequest {
            kind: VisualizerKind::Scope,
            format: VisualizerOutputFormat::Svg,
            stream_id: None,
            window_cycles: 1.0,
            width_px: 640,
            height_px: 360,
        })
        .unwrap();
    assert!(svg.draw_commands.is_none());
    let markup = svg.svg_markup.unwrap();
    assert!(markup.contains("width=\"640\""));
    assert!(markup.contains("height=\"360\""));
    assert!(svg.png_base64.is_none());

    let png = srv
        .capture_visualizer_frame(CaptureVisualizerFrameRequest {
            kind: VisualizerKind::Spectrum,
            format: VisualizerOutputFormat::PngBase64,
            stream_id: Some("d1".to_string()),
            window_cycles: 2.0,
            width_px: 320,
            height_px: 200,
        })
        .unwrap();
    assert!(png.draw_commands.is_none());
    assert!(png.svg_markup.is_none());
    let b64 = png.png_base64.unwrap();
    assert!(b64.starts_with("iVBORw0KGgo"));

    let ast = srv
        .capture_visualizer_frame(CaptureVisualizerFrameRequest {
            kind: VisualizerKind::Punchcard,
            format: VisualizerOutputFormat::DrawAstJson,
            stream_id: None,
            window_cycles: 1.0,
            width_px: 100,
            height_px: 100,
        })
        .unwrap();
    assert_eq!(ast.draw_commands.as_ref().unwrap().len(), 2);
    assert!(ast.svg_markup.is_none());
    assert!(ast.png_base64.is_none());
}

// ----------------------------------------------------------------------------
// resources.rs — decode_share_url hex-decode loop, including the
// bytes.push success path (1274-1278).
// ----------------------------------------------------------------------------
#[test]
fn share_url_roundtrip_and_malformed_hex() {
    let srv = SilverWaveMcpServer::default();

    let enc = srv
        .encode_share_url(EncodeShareUrlRequest {
            code: Some("d1 $ s(\"bd\")".to_string()),
            base_url: "https://strudel.cc/#".to_string(),
            include_session_metadata: false,
        })
        .unwrap();
    assert!(enc.share_url.contains("#sw="));
    assert_eq!(enc.payload_bytes, "d1 $ s(\"bd\")".len());

    // Valid hex payload decodes byte-for-byte (1274-1278)
    let dec = srv
        .decode_share_url(DecodeShareUrlRequest {
            share_url: enc.share_url.clone(),
        })
        .unwrap();
    assert_eq!(dec.code, "d1 $ s(\"bd\")");
    assert!(dec.cps.is_some());

    // Non-hex pairs are skipped by the `if let Ok(b)` guard
    let dec2 = srv
        .decode_share_url(DecodeShareUrlRequest {
            share_url: "https://x/#sw=zz41".to_string(),
        })
        .unwrap();
    assert_eq!(dec2.code, "A"); // 0x41 survives, "zz" is skipped

    // No fragment at all decodes to an empty string
    let dec3 = srv
        .decode_share_url(DecodeShareUrlRequest {
            share_url: "https://x/no-fragment".to_string(),
        })
        .unwrap();
    assert_eq!(dec3.code, "");
}
