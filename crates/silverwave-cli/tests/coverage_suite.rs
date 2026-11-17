//! Line-coverage gap suite for `silverwave-cli`.
//! Exercises the previously uncovered regions of `cli.rs`, `main.rs`, `packaging.rs`,
//! and `supervisor.rs`: `GlobalOptions::default()`, subcommand dispatch arms
//! (daemon `--stop`, `eval --hush`, `eval` missing-file `NoInput`, sampler
//! `Index`/`Pack`/`Import`/`List`), the `main()` binary entry point via process
//! spawn, `PwaHostingHeaderContract::default()`, `GATE-04/05/06` + `PKG-SMOKE`
//! release-gate violation branches, `Default` impls, and the Stage 1/2
//! panic-recovery MIDI note-off + audio interlock state paths.

#![forbid(unsafe_code)]

use silverwave_cli::{
    ActiveNoteBitset, AudioEngineSafetyState, AudioSafetyInterlock, ColorChoice, DaemonArgs,
    EmergencyMidiFlushRing, EvalArgs, GlobalOptions, LivePerformanceSupervisor,
    LiveRecoveryControlBlock, LogFormat, LogLevel, PreallocatedSessionAutosaver,
    PwaHostingHeaderContract, RecoveryMode, ReleaseGateVerifier, RtAudioSafetyInterlock,
    SamplerSubcommand, SilverWaveExitCode, SilverWavePackagingVerifier, Subcommand,
    execute_cli_subcommand,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

/// Covers `cli.rs` `GlobalOptions::default()` (lines 75-85) and asserts the
/// normative spec defaults (`06-SPEC` §1.1).
#[test]
fn global_options_default_matches_spec() {
    let g = GlobalOptions::default();
    assert!(g.config.is_none());
    assert_eq!(g.log_level, LogLevel::Info);
    assert_eq!(g.log_format, LogFormat::Pretty);
    assert_eq!(g.color, ColorChoice::Auto);
    assert_eq!(g.runtime_dir, PathBuf::from("/tmp/silverwave-runtime"));
    assert_eq!(g.sample_dir, PathBuf::from("/tmp/silverwave-samples"));
    assert!(!g.no_recovery_hook);
}

/// Covers `cli.rs` daemon `stop` dispatch arm (lines 274-278).
#[test]
fn daemon_stop_subcommand_returns_clean_shutdown() {
    let global = GlobalOptions::default();
    let report = execute_cli_subcommand(
        &global,
        Subcommand::Daemon(DaemonArgs {
            socket: PathBuf::from("/tmp/sw-cov.sock"),
            listen_tcp: None,
            pid_file: PathBuf::from("/tmp/sw-cov.pid"),
            foreground: false,
            realtime_priority: 70,
            lock_memory: false,
            watchdog_timeout_ms: 50,
            stop: true,
            status: false,
        }),
    );
    assert_eq!(report.exit_code, SilverWaveExitCode::Success);
    assert_eq!(report.stdout_text, "silverwave daemon stopped cleanly");
    assert!(report.stderr_text.is_empty());
}

/// Covers `cli.rs` eval `--hush` early return (lines 297-301) and the
/// missing-file `NoInput` guard (lines 303-310).
#[test]
fn eval_hush_short_circuits_and_missing_file_is_no_input() {
    let global = GlobalOptions::default();
    let base = EvalArgs {
        expr: Some("s(\"bd sd\")".to_string()),
        file: None,
        cycles_start: 0.0,
        cycles_end: 4.0,
        cps: 0.5,
        format_json: true,
        send_daemon: false,
        slot: "d1".to_string(),
        hush: true,
    };
    let hushed = execute_cli_subcommand(&global, Subcommand::Eval(base.clone()));
    assert_eq!(hushed.exit_code, SilverWaveExitCode::Success);
    assert_eq!(hushed.stdout_text, "{\"hushed\":true}");

    let missing = execute_cli_subcommand(
        &global,
        Subcommand::Eval(EvalArgs {
            file: Some(PathBuf::from("scores/missing_take.swave")),
            hush: false,
            ..base
        }),
    );
    assert_eq!(missing.exit_code, SilverWaveExitCode::NoInput);
    assert!(missing.stdout_text.is_empty());
    assert_eq!(missing.stderr_text, "Input file does not exist");
}

/// Covers `cli.rs` sampler `Index`/`Pack` shared arm (lines 382-386) and the
/// `Import` (388-392) / `List` (393-397) arms.
#[test]
fn sampler_index_pack_import_list_dispatch_arms() {
    let global = GlobalOptions::default();

    let indexed = execute_cli_subcommand(
        &global,
        Subcommand::Sampler(SamplerSubcommand::Index {
            dir: PathBuf::from("samples/dirt"),
            output: PathBuf::from("out/samples.lock.json"),
        }),
    );
    assert_eq!(indexed.exit_code, SilverWaveExitCode::Success);
    assert_eq!(
        indexed.stdout_text,
        "{\"written\":\"out/samples.lock.json\"}"
    );

    let packed = execute_cli_subcommand(
        &global,
        Subcommand::Sampler(SamplerSubcommand::Pack {
            dir: PathBuf::from("samples/dirt"),
            output: PathBuf::from("out/dirt.swpack"),
        }),
    );
    assert_eq!(packed.exit_code, SilverWaveExitCode::Success);
    assert_eq!(packed.stdout_text, "{\"written\":\"out/dirt.swpack\"}");

    let imported = execute_cli_subcommand(
        &global,
        Subcommand::Sampler(SamplerSubcommand::Import {
            source: "https://samples.example/dirt.zip".to_string(),
            bank_name: Some("dirt".to_string()),
            convert_flac: true,
        }),
    );
    assert_eq!(imported.exit_code, SilverWaveExitCode::Success);
    assert_eq!(
        imported.stdout_text,
        "{\"imported\":\"https://samples.example/dirt.zip\"}"
    );

    let listed = execute_cli_subcommand(
        &global,
        Subcommand::Sampler(SamplerSubcommand::List {
            filter: Some("dirt".to_string()),
            format_json: true,
        }),
    );
    assert_eq!(listed.exit_code, SilverWaveExitCode::Success);
    assert_eq!(
        listed.stdout_text,
        "[{\"bank\":\"dirt-samples\",\"variants\":218}]"
    );
}

/// Covers `main.rs` `fn main` (lines 6-23) by spawning the compiled
/// `silverwave-cli` binary; it runs the headless `repl` default subcommand and
/// maps the report exit code through `ExitCode`.
#[test]
fn silverwave_binary_main_prints_repl_banner_and_exits_zero() {
    let exe = env!("CARGO_BIN_EXE_silverwave-cli");
    let out = std::process::Command::new(exe)
        .output()
        .expect("failed to spawn silverwave-cli binary");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("silverwave repl started (48000 Hz, 128 frames, 0.50 cps)"),
        "unexpected stdout: {stdout:?}"
    );
}

/// Covers `packaging.rs` `PwaHostingHeaderContract::default()` (lines 172-174)
/// and re-verifies the COOP/COEP cross-origin isolation contract.
#[test]
fn pwa_header_contract_default_is_cross_origin_isolated() {
    let contract = PwaHostingHeaderContract::default();
    assert_eq!(
        contract.service_worker_cache_version,
        "silverwave-pwa-v0.8.0-main"
    );
    assert!(!contract.auto_skip_waiting_while_audio_running);
    assert!(contract.verify_cross_origin_isolated());
    assert_eq!(
        contract.headers.get("Cross-Origin-Embedder-Policy"),
        Some(&"require-corp".to_string())
    );
}

/// Covers `packaging.rs` `verify_release_readiness` violation branches:
/// `GATE-04` (lines 353-356), `GATE-05` (359-362), `GATE-06` (365-368), and
/// `PKG-SMOKE` (376-379).
#[test]
fn release_gate_verifier_reports_each_violation_branch() {
    let verifier = SilverWavePackagingVerifier;
    let good = SilverWavePackagingVerifier::run_all_platform_installer_smoke_tests("1.0.0");
    assert!(verifier.verify_release_readiness(&good).is_ok());

    // GATE-04: pluginval strictness 10 / clap-validator failure.
    let mut gate04 = good.clone();
    gate04.pluginval_strictness_10_passed = false;
    let violations = verifier.verify_release_readiness(&gate04).unwrap_err();
    assert!(violations.iter().any(|v| v.gate_id == "GATE-04"));

    let mut gate04b = good.clone();
    gate04b.clap_validator_passed = false;
    let violations = verifier.verify_release_readiness(&gate04b).unwrap_err();
    assert!(violations.iter().any(|v| v.gate_id == "GATE-04"));

    // GATE-05: SLSA L3 provenance / CycloneDX SBOM failure.
    let mut gate05 = good.clone();
    gate05.slsa_level_3_verified = false;
    let violations = verifier.verify_release_readiness(&gate05).unwrap_err();
    assert!(violations.iter().any(|v| v.gate_id == "GATE-05"));

    let mut gate05b = good.clone();
    gate05b.cyclonedx_sbom_valid = false;
    let violations = verifier.verify_release_readiness(&gate05b).unwrap_err();
    assert!(violations.iter().any(|v| v.gate_id == "GATE-05"));

    // GATE-06: cargo-deny license audit / PWA COOP-COEP verification failure.
    let mut gate06 = good.clone();
    gate06.cargo_deny_passed = false;
    let violations = verifier.verify_release_readiness(&gate06).unwrap_err();
    assert!(violations.iter().any(|v| v.gate_id == "GATE-06"));

    let mut gate06b = good.clone();
    gate06b.pwa_cross_origin_isolated_verified = false;
    let violations = verifier.verify_release_readiness(&gate06b).unwrap_err();
    assert!(violations.iter().any(|v| v.gate_id == "GATE-06"));

    // PKG-SMOKE: an installer smoke-test field flipped to false.
    for flip in [
        |r: &mut silverwave_cli::ReleaseVerificationGateReport| {
            r.platform_smoke_tests[0].install_succeeded = false;
        },
        |r: &mut silverwave_cli::ReleaseVerificationGateReport| {
            r.platform_smoke_tests[1].cli_version_verified = false;
        },
        |r: &mut silverwave_cli::ReleaseVerificationGateReport| {
            r.platform_smoke_tests[2].deterministic_render_sha256_matched = false;
        },
        |r: &mut silverwave_cli::ReleaseVerificationGateReport| {
            r.platform_smoke_tests[3].clean_uninstall_verified = false;
        },
    ] {
        let mut report = good.clone();
        flip(&mut report);
        let violations = verifier.verify_release_readiness(&report).unwrap_err();
        assert!(
            violations.iter().any(|v| v.gate_id == "PKG-SMOKE"),
            "expected PKG-SMOKE violation, got {violations:?}"
        );
    }
}

/// Covers `supervisor.rs` `Default` impls (lines 45-47, 101-103, 147-149,
/// 222-224, 346-348, 435-437) and `AudioEngineSafetyState::from_u8(3)` (line 31).
#[test]
fn supervisor_default_impls_and_state_decode() {
    assert_eq!(
        AudioEngineSafetyState::from_u8(0),
        AudioEngineSafetyState::Running
    );
    assert_eq!(
        AudioEngineSafetyState::from_u8(1),
        AudioEngineSafetyState::DeClickFadeOut
    );
    assert_eq!(
        AudioEngineSafetyState::from_u8(2),
        AudioEngineSafetyState::EmergencyMute
    );
    assert_eq!(
        AudioEngineSafetyState::from_u8(3),
        AudioEngineSafetyState::RecoveringFadeIn
    );
    assert_eq!(
        AudioEngineSafetyState::from_u8(255),
        AudioEngineSafetyState::EmergencyMute
    );

    let bitset = ActiveNoteBitset::default();
    bitset.note_on(0, 60);
    assert!(bitset.is_note_active(0, 60));
    bitset.note_off(0, 60);
    assert!(!bitset.is_note_active(0, 60));

    let control = LiveRecoveryControlBlock::default();
    assert_eq!(
        control.engine_state.load(Ordering::Acquire),
        AudioEngineSafetyState::Running as u8
    );

    let ring = EmergencyMidiFlushRing::default();
    assert_eq!(ring.port_name, "midi:out:0");
    assert_eq!(ring.flushed_packets.capacity(), 2096);

    let interlock = RtAudioSafetyInterlock::default();
    assert_eq!(interlock.current_state(), AudioEngineSafetyState::Running);

    let saver = PreallocatedSessionAutosaver::default();
    assert_eq!(saver.last_known_good_code, "d1 $ s(\"bd sd\")");
    assert_eq!(saver.unsaved_buffer.capacity(), 131_072);

    let supervisor = LivePerformanceSupervisor::default();
    assert_eq!(supervisor.watchdog_timeout_ns, 50 * 1_000_000);
    assert_eq!(
        supervisor.active_recovery_mode,
        RecoveryMode::ResumeLastGood
    );
    assert_eq!(supervisor.max_restarts_per_minute, 10);
    assert_eq!(supervisor.worker_generation, 1);
}

/// Covers `supervisor.rs` `process_emergency_block` state arms: `Running`
/// no-op (line 253), `EmergencyMute` zeroing (lines 273-275), and the
/// `RecoveringFadeIn` raised-cosine fade-in (lines 279-292).
#[test]
fn emergency_block_covers_running_mute_and_fadein_arms() {
    let interlock = RtAudioSafetyInterlock::new();
    let mut buf = [1.0f32; 128];

    // Running => {} : block left untouched.
    interlock.process_emergency_block(&mut [&mut buf]);
    assert!(buf.iter().all(|s| *s == 1.0));
    assert_eq!(interlock.current_state(), AudioEngineSafetyState::Running);

    // EmergencyMute => all samples forced to 0.0.
    interlock.state.store(
        AudioEngineSafetyState::EmergencyMute as u8,
        Ordering::SeqCst,
    );
    interlock.process_emergency_block(&mut [&mut buf]);
    assert!(buf.iter().all(|s| *s == 0.0));

    // RecoveringFadeIn => raised-cosine fade-in then transition back to Running.
    interlock.state.store(
        AudioEngineSafetyState::RecoveringFadeIn as u8,
        Ordering::SeqCst,
    );
    let mut buf2 = [1.0f32; 128];
    buf2[10] = f32::NAN; // non-finite sample must be sanitized to 0.0
    interlock.process_emergency_block(&mut [&mut buf2]);
    assert_eq!(buf2[0], 0.0, "fade-in gain is 0 at frame 0");
    assert_eq!(buf2[10], 0.0, "NaN sample sanitized");
    assert_eq!(buf2[64], 1.0, "post-ramp samples pass through");
    assert!(
        buf2[32] > 0.0 && buf2[32] < 1.0,
        "mid-ramp sample attenuated: {}",
        buf2[32]
    );
    assert_eq!(interlock.current_state(), AudioEngineSafetyState::Running);
}

/// Covers `supervisor.rs` `handle_worker_fault` `ResumeLastGood` respawn arm
/// (line 516 region), the Stage 1 tracked-note note-off flush, and the
/// `MuteStandby` transition after `max_restarts_per_minute` is exceeded.
#[test]
fn handle_worker_fault_flushes_notes_and_transitions_to_mute_standby() {
    let dir = std::env::temp_dir().join(format!("sw-cov-fault-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let mut sup = LivePerformanceSupervisor::new(50, RecoveryMode::ResumeLastGood);
    sup.control_block.active_notes.note_on(0, 60);
    sup.control_block.active_notes.note_on(9, 127);

    let manifest = sup
        .handle_worker_fault("worker panic: eval blew up", &dir, 4242)
        .expect("fault handling should succeed");
    assert_eq!(manifest.recovery_action, RecoveryMode::ResumeLastGood);
    assert_eq!(manifest.process_id, 4242);
    assert_eq!(sup.worker_generation, 2);
    assert_eq!(sup.restarts_in_current_window, 1);
    assert_eq!(
        sup.audio_interlock.current_state(),
        AudioEngineSafetyState::RecoveringFadeIn
    );

    // Stage 1 emitted CC120/CC123/CC64 on all 16 channels plus explicit
    // note-offs for the two tracked active notes, which were also cleared.
    let packets = &sup.midi_flush_ring.flushed_packets;
    assert_eq!(packets.len(), 16 * 3 + 2);
    assert!(packets.iter().any(|p| p.status == 0x80 && p.data1 == 60));
    assert!(packets.iter().any(|p| p.status == 0x89 && p.data1 == 127));
    assert!(!sup.control_block.active_notes.is_note_active(0, 60));

    // Stage 3 persisted the .swave autosave + .crash.json manifest atomically.
    let swave = dir.join("session-20261116T022000Z-4242.swave");
    let crash = dir.join("session-20261116T022000Z-4242.crash.json");
    assert!(swave.exists(), "missing {swave:?}");
    assert!(crash.exists(), "missing {crash:?}");
    let crash_json = std::fs::read_to_string(&crash).unwrap();
    assert!(crash_json.contains("\"panic_message\":\"worker panic: eval blew up\""));

    // After > 10 restarts in the window the supervisor drops to MuteStandby.
    for _ in 0..11 {
        sup.handle_worker_fault("repeat crash", &dir, 4242).unwrap();
    }
    assert_eq!(sup.active_recovery_mode, RecoveryMode::MuteStandby);
    assert_eq!(
        sup.audio_interlock.current_state(),
        AudioEngineSafetyState::EmergencyMute
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Covers `cli.rs` eval `expr: None` default arm (line 312): `unwrap_or_else`
/// supplies the `"s(\"bd sd\")"` fallback expression.
#[test]
fn eval_defaults_expr_when_omitted() {
    let global = GlobalOptions::default();
    let report = execute_cli_subcommand(
        &global,
        Subcommand::Eval(EvalArgs {
            expr: None,
            file: None,
            cycles_start: 0.0,
            cycles_end: 4.0,
            cps: 0.5,
            format_json: true,
            send_daemon: false,
            slot: "d1".to_string(),
            hush: false,
        }),
    );
    assert_eq!(report.exit_code, SilverWaveExitCode::Success);
    assert!(
        report.stdout_text.contains("bd sd"),
        "unexpected stdout: {:?}",
        report.stdout_text
    );
    assert!(report.stderr_text.is_empty());
}

/// Covers `supervisor.rs` `process_emergency_block` `DeClickFadeOut` arm with
/// non-finite input: the `is_finite()` false edge sanitizes `NaN`/`inf`
/// samples to `0.0` (line 257 else region).
#[test]
fn emergency_block_declick_sanitizes_non_finite_samples() {
    let interlock = RtAudioSafetyInterlock::new();
    interlock.trigger_emergency_mute();
    assert_eq!(
        interlock.current_state(),
        AudioEngineSafetyState::DeClickFadeOut
    );

    let mut buf = [1.0f32; 128];
    buf[10] = f32::NAN;
    buf[20] = f32::INFINITY;
    interlock.process_emergency_block(&mut [&mut buf]);

    assert_eq!(buf[0], 1.0, "fade-out gain is 1 at frame 0");
    assert_eq!(buf[10], 0.0, "NaN sample sanitized");
    assert_eq!(buf[20], 0.0, "inf sample sanitized");
    assert!(
        buf[32] > 0.0 && buf[32] < 1.0,
        "mid-ramp sample attenuated: {}",
        buf[32]
    );
    assert_eq!(buf[64], 0.0, "post-ramp frames are zeroed");
    assert_eq!(
        interlock.current_state(),
        AudioEngineSafetyState::EmergencyMute
    );
}

/// Covers `supervisor.rs` `flush_crash_snapshot_to_disk` error edges:
/// `fs::create_dir_all` failure (line 383), `.swave.tmp` write failure
/// (line 389), atomic `rename` failure (line 390), and `.crash.json` write
/// failure (line 407), plus the `?` propagation edge in `handle_worker_fault`
/// (line 499).
#[test]
fn autosaver_flush_error_edges_propagate() {
    let base = std::env::temp_dir().join(format!("sw-cov-err-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let saver = PreallocatedSessionAutosaver::new();

    // 383: `create_dir_all` fails when a path component is a regular file.
    let blocker = base.join("blocker-file");
    std::fs::write(&blocker, b"x").unwrap();
    let bad_dir = blocker.join("sub");
    assert!(
        saver
            .flush_crash_snapshot_to_disk("boom", &bad_dir, 1, RecoveryMode::MuteStandby)
            .is_err()
    );

    // 389: the `.swave.tmp` path already exists as a directory -> `fs::write` fails.
    let d2 = base.join("write-tmp-blocked");
    std::fs::create_dir_all(d2.join("session-20261116T022000Z-2.swave.tmp")).unwrap();
    assert!(
        saver
            .flush_crash_snapshot_to_disk("boom", &d2, 2, RecoveryMode::MuteStandby)
            .is_err()
    );

    // 390: the final `.swave` path exists as a directory -> `fs::rename` fails.
    let d3 = base.join("rename-target-blocked");
    std::fs::create_dir_all(d3.join("session-20261116T022000Z-3.swave")).unwrap();
    assert!(
        saver
            .flush_crash_snapshot_to_disk("boom", &d3, 3, RecoveryMode::MuteStandby)
            .is_err()
    );

    // 407: the `.crash.json` path exists as a directory -> `fs::write` fails.
    let d4 = base.join("json-target-blocked");
    std::fs::create_dir_all(d4.join("session-20261116T022000Z-4.crash.json")).unwrap();
    assert!(
        saver
            .flush_crash_snapshot_to_disk("boom", &d4, 4, RecoveryMode::MuteStandby)
            .is_err()
    );

    // 499: `handle_worker_fault` propagates the Stage 3 autosave I/O error.
    let mut sup = LivePerformanceSupervisor::new(50, RecoveryMode::ResumeLastGood);
    assert!(sup.handle_worker_fault("boom", &bad_dir, 9).is_err());

    let _ = std::fs::remove_dir_all(&base);
}

/// Covers `supervisor.rs` `handle_worker_fault` `else` respawn arm with a
/// non-`ResumeLastGood` recovery mode (line 516 edge): `MuteStandby` skips the
/// `RecoveringFadeIn` state store while still bumping `worker_generation`.
#[test]
fn handle_worker_fault_mute_standby_skips_fade_in() {
    let dir = std::env::temp_dir().join(format!("sw-cov-mute-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let mut sup = LivePerformanceSupervisor::new(50, RecoveryMode::MuteStandby);
    let manifest = sup
        .handle_worker_fault("worker panic", &dir, 777)
        .expect("fault handling should succeed");
    assert_eq!(manifest.recovery_action, RecoveryMode::MuteStandby);
    assert_eq!(sup.worker_generation, 2);
    assert_eq!(sup.restarts_in_current_window, 1);
    assert_eq!(
        sup.audio_interlock.current_state(),
        AudioEngineSafetyState::DeClickFadeOut
    );

    let _ = std::fs::remove_dir_all(&dir);
}
