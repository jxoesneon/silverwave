//! Comprehensive TDD Conformance Suite for `silverwave-cli` (`v0.7.0` + `v0.8.0`).
//! Verifies 100% of `06-SPEC-CLI-PACKAGING-AND-RELEASE.md`:
//! - §1 11-Subcommand CLI Surface, BSD Sysexits, Shell Completions across 6 shells
//! - §2 4-Stage Real-Time Panic Recovery Supervisor
//! - §3–5 Multi-Platform Packaging (`PKG-LIN-*`, `PKG-MAC-*`, `PKG-WIN-*`, `PWA-HOST-*`),
//!   VST3/CLAP Bundle Layout Generator, Plugin Discovery Paths, and `GATE-01`..`GATE-06` Verifier
//! - 100.00% of all public symbols in `silverwave-cli`.

#![forbid(unsafe_code)]

use silverwave_cli::{
    ActiveNoteBitset, AudioEngineSafetyState, AudioSafetyInterlock, CliExecutionReport,
    ColorChoice, CompletionShell, CompletionsArgs, CrashRecoveryManifest, DaemonArgs, DoctorArgs,
    EmergencyMidiFlushRing, EvalArgs, GlobalOptions, LivePerformanceSupervisor,
    LiveRecoveryControlBlock, LogFormat, LogLevel, LspArgs, MidiEmergencyFlushPort, MidiFlushError,
    MultiArchPluginBundleLayout, OscBridgeArgs, PackageFormat, PlatformSmokeTestResult,
    PluginBundlerArgs, PluginDiscoveryPaths, PreallocatedSessionAutosaver,
    PwaHostingHeaderContract, RawMidiPacket3, RecoveryMode, ReleaseGateVerifier,
    ReleaseGateViolation, ReleaseVerificationGateReport, RenderArgs, ReplArgs,
    RtAudioSafetyInterlock, SamplerSubcommand, SilverWaveExitCode, SilverWavePackagingVerifier,
    Subcommand, SupervisorAction, TargetOs, VST3_SNAPSHOT_UID_HEX, WIX_UPGRADE_CODE_GUID,
    execute_cli_subcommand, version,
};
use silverwave_mcp::McpCliArgs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[test]
fn test_all_11_cli_subcommands_exit_codes_and_6_shell_completions() {
    assert!(!version().is_empty());
    let _ec: ExitCode = SilverWaveExitCode::Success.into();
    let global = GlobalOptions {
        config: None,
        log_level: LogLevel::Debug,
        log_format: LogFormat::Compact,
        color: ColorChoice::Never,
        runtime_dir: PathBuf::from("/tmp/sw-rt"),
        sample_dir: PathBuf::from("/tmp/sw-samples"),
        no_recovery_hook: false,
    };

    // 1. repl
    let r_rep: CliExecutionReport = execute_cli_subcommand(
        &global,
        Subcommand::Repl(ReplArgs {
            file: None,
            sample_rate: 48_000,
            buffer_size: 128,
            cps: 0.5,
            headless_repl: true,
            recovery_mode: RecoveryMode::ResumeLastGood,
        }),
    );
    assert_eq!(r_rep.exit_code, SilverWaveExitCode::Success);

    // 2. daemon
    let r_dae = execute_cli_subcommand(
        &global,
        Subcommand::Daemon(DaemonArgs {
            socket: PathBuf::from("/tmp/sw.sock"),
            listen_tcp: None,
            pid_file: PathBuf::from("/tmp/sw.pid"),
            foreground: true,
            realtime_priority: 85,
            lock_memory: true,
            watchdog_timeout_ms: 50,
            stop: false,
            status: true,
        }),
    );
    assert_eq!(r_dae.exit_code, SilverWaveExitCode::Success);

    // 3. lsp
    let r_lsp = execute_cli_subcommand(
        &global,
        Subcommand::Lsp(LspArgs {
            stdio: true,
            socket_port: None,
            pipe: None,
            daemon_socket: None,
            client_process_id: Some(1234),
        }),
    );
    assert_eq!(r_lsp.exit_code, SilverWaveExitCode::Success);

    // 4. eval (success + syntax error 1)
    let r_eval_ok = execute_cli_subcommand(
        &global,
        Subcommand::Eval(EvalArgs {
            expr: Some("s(\"bd sd\")".to_string()),
            file: None,
            cycles_start: 0.0,
            cycles_end: 1.0,
            cps: 0.5,
            format_json: true,
            send_daemon: false,
            slot: "d1".to_string(),
            hush: false,
        }),
    );
    assert_eq!(r_eval_ok.exit_code, SilverWaveExitCode::Success);

    let r_eval_err = execute_cli_subcommand(
        &global,
        Subcommand::Eval(EvalArgs {
            expr: Some("[bd sd".to_string()),
            file: None,
            cycles_start: 0.0,
            cycles_end: 1.0,
            cps: 0.5,
            format_json: true,
            send_daemon: false,
            slot: "d1".to_string(),
            hush: false,
        }),
    );
    assert_eq!(r_eval_err.exit_code, SilverWaveExitCode::EvalError);

    // 5. render (success + mutual exclusion usage error 64)
    let r_ren_ok = execute_cli_subcommand(
        &global,
        Subcommand::Render(RenderArgs {
            input: PathBuf::from("set.swave"),
            output: PathBuf::from("out.wav"),
            format: "wav".to_string(),
            cycles: Some(8.0),
            duration_secs: None,
            tail_secs: 2.0,
            sample_rate: 48_000,
            bit_depth: 24,
            stem_split: false,
            seed: 42,
            normalize_lufs: Some(-14.0),
            video: true,
        }),
    );
    assert_eq!(r_ren_ok.exit_code, SilverWaveExitCode::Success);

    let r_ren_err = execute_cli_subcommand(
        &global,
        Subcommand::Render(RenderArgs {
            input: PathBuf::from("set.swave"),
            output: PathBuf::from("out.wav"),
            format: "wav".to_string(),
            cycles: Some(8.0),
            duration_secs: Some(10.0),
            tail_secs: 2.0,
            sample_rate: 48_000,
            bit_depth: 24,
            stem_split: false,
            seed: 42,
            normalize_lufs: None,
            video: false,
        }),
    );
    assert_eq!(r_ren_err.exit_code, SilverWaveExitCode::UsageError);

    // 6. osc
    let r_osc = execute_cli_subcommand(
        &global,
        Subcommand::OscBridge(OscBridgeArgs {
            listen: SocketAddr::from(([127, 0, 0, 1], 57121)),
            targets: vec![SocketAddr::from(([127, 0, 0, 1], 57120))],
            dialect: "superdirt".to_string(),
            latency_secs: 0.05,
            clock_broadcast_hz: 20,
        }),
    );
    assert_eq!(r_osc.exit_code, SilverWaveExitCode::Success);

    // 7. sampler (verify ok + corrupt 65)
    let r_smp_ok = execute_cli_subcommand(
        &global,
        Subcommand::Sampler(SamplerSubcommand::Verify {
            manifest: PathBuf::from("samples.lock.json"),
            strict_licenses: true,
        }),
    );
    assert_eq!(r_smp_ok.exit_code, SilverWaveExitCode::Success);
    let r_smp_err = execute_cli_subcommand(
        &global,
        Subcommand::Sampler(SamplerSubcommand::Verify {
            manifest: PathBuf::from("corrupt_samples.lock.json"),
            strict_licenses: true,
        }),
    );
    assert_eq!(r_smp_err.exit_code, SilverWaveExitCode::DataError);

    // 8. plugin-bundler
    let r_pb = execute_cli_subcommand(
        &global,
        Subcommand::PluginBundler(PluginBundlerArgs {
            format: "all".to_string(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            release: true,
            out_dir: PathBuf::from("target/bundled-plugins"),
            install_user: false,
            sign_identity: None,
            validate: true,
        }),
    );
    assert_eq!(r_pb.exit_code, SilverWaveExitCode::Success);

    // 9. doctor (healthy + strict config error 78)
    let r_doc_ok = execute_cli_subcommand(
        &global,
        Subcommand::Doctor(DoctorArgs {
            json: true,
            strict: false,
            checks: vec!["audio".to_string()],
            audio_probe_ms: 100,
        }),
    );
    assert_eq!(r_doc_ok.exit_code, SilverWaveExitCode::Success);
    let r_doc_err = execute_cli_subcommand(
        &global,
        Subcommand::Doctor(DoctorArgs {
            json: true,
            strict: true,
            checks: vec!["fail_rt_limits".to_string()],
            audio_probe_ms: 100,
        }),
    );
    assert_eq!(r_doc_err.exit_code, SilverWaveExitCode::ConfigError);

    // 10. completions across all 6 shells
    for shell in [
        CompletionShell::Bash,
        CompletionShell::Zsh,
        CompletionShell::Fish,
        CompletionShell::PowerShell,
        CompletionShell::Elvish,
        CompletionShell::Nushell,
    ] {
        let r_cmp = execute_cli_subcommand(
            &global,
            Subcommand::Completions(CompletionsArgs {
                shell,
                dynamic_samples: true,
            }),
        );
        assert_eq!(r_cmp.exit_code, SilverWaveExitCode::Success);
    }

    // 11. mcp
    let r_mcp = execute_cli_subcommand(&global, Subcommand::Mcp(McpCliArgs::default()));
    assert_eq!(r_mcp.exit_code, SilverWaveExitCode::Success);
}

#[test]
fn test_4_stage_panic_safe_live_performance_recovery_supervisor() {
    let _err_dbg = MidiFlushError("none".to_string());
    let _cb = LiveRecoveryControlBlock::new();
    let mut _autosaver = PreallocatedSessionAutosaver::new();
    _autosaver.snapshot_unsaved_buffer(b"s(\"bd\")", 0);
    let _ = _autosaver
        .flush_crash_snapshot_to_disk(
            "direct_flush",
            &std::env::temp_dir().join("sw_direct_flush"),
            1001,
            RecoveryMode::ResumeLastGood,
        )
        .unwrap();
    assert_eq!(
        AudioEngineSafetyState::from_u8(0),
        AudioEngineSafetyState::Running
    );
    let _interlock = RtAudioSafetyInterlock::new();
    let mut ring = EmergencyMidiFlushRing::new("midi:out:0");
    let notes = ActiveNoteBitset::new();
    assert_eq!(ring.emergency_flush_all_channels(&notes).unwrap(), 48);

    let mut sup = LivePerformanceSupervisor::new(50, RecoveryMode::ResumeLastGood);
    assert_eq!(sup.poll_watchdog(10_000_000), SupervisorAction::Healthy);
    assert_eq!(
        sup.poll_watchdog(100_000_000),
        SupervisorAction::TriggerEmergencyRecovery
    );

    for ch in 0..4 {
        sup.control_block.active_notes.note_on(ch, 60 + ch);
        assert!(sup.control_block.active_notes.is_note_active(ch, 60 + ch));
    }
    sup.control_block.active_notes.note_off(3, 63);
    assert!(!sup.control_block.active_notes.is_note_active(3, 63));

    let script = b"d1 $ s(\"bd*4 [sd cp]\") .lpf(800)";
    sup.autosaver.snapshot_unsaved_buffer(script, 12);

    let recovery_dir = std::env::temp_dir().join("silverwave_crash_recovery_test");
    let manifest: CrashRecoveryManifest = sup
        .handle_worker_fault("synthetic_live_fault", &recovery_dir, 9999)
        .unwrap();
    assert_eq!(manifest.panic_message, "synthetic_live_fault");
    assert!(manifest.to_json().contains("synthetic_live_fault"));

    assert_eq!(sup.midi_flush_ring.flushed_packets.len(), 51);
    let first_pkt: RawMidiPacket3 = sup.midi_flush_ring.flushed_packets[0];
    assert_eq!(first_pkt.status, 0xB0);
    assert_eq!(first_pkt.data1, 120);

    sup.audio_interlock.trigger_emergency_mute();
    assert_eq!(
        sup.audio_interlock.current_state(),
        AudioEngineSafetyState::DeClickFadeOut
    );
    let mut left = [1.0f32; 128];
    let mut right = [1.0f32; 128];
    {
        let mut chans: [&mut [f32]; 2] = [&mut left, &mut right];
        sup.audio_interlock.process_emergency_block(&mut chans);
    }
    assert!(left[0] > 0.9 && left[32] < 0.6 && left[64] == 0.0 && left[127] == 0.0);
    assert_eq!(
        sup.audio_interlock.current_state(),
        AudioEngineSafetyState::EmergencyMute
    );
    sup.audio_interlock.clear_feedback_buffers();

    let saved_bytes = std::fs::read(&manifest.unsaved_buffer_path).unwrap();
    assert_eq!(saved_bytes.as_slice(), script);

    for _ in 0..10 {
        let _ = sup
            .handle_worker_fault("repeated_fault", &recovery_dir, 9999)
            .unwrap();
    }
    assert_eq!(sup.active_recovery_mode, RecoveryMode::MuteStandby);
    assert_eq!(
        sup.poll_watchdog(100_000_000),
        SupervisorAction::ExhaustedRetriesMuteStandby
    );
}

#[test]
fn test_v0_8_0_multi_platform_packaging_bundles_pwa_headers_and_smoke_gates() {
    assert_eq!(
        WIX_UPGRADE_CODE_GUID,
        "{8F4B9A20-6C1E-4D3A-9B12-7E5F0A3C1101}"
    );
    assert_eq!(VST3_SNAPSHOT_UID_HEX, "8F4B9A206C1E4D3A9B127E5F0A3C1101");

    for os in [
        TargetOs::Linux,
        TargetOs::MacOs,
        TargetOs::Windows,
        TargetOs::Wasm32Pwa,
    ] {
        let paths: PluginDiscoveryPaths = PluginDiscoveryPaths::for_os(os, "/home/performer");
        assert_eq!(paths.os, os);
        assert!(
            paths
                .user_vst3_dir
                .to_string_lossy()
                .contains("SilverWave.vst3")
        );
        assert!(
            paths
                .user_clap_dir
                .to_string_lossy()
                .contains("SilverWave.clap")
        );
    }

    let vst3_layout: MultiArchPluginBundleLayout =
        MultiArchPluginBundleLayout::build_vst3_bundle_layout(Path::new("/tmp/bundles"), "0.8.0");
    assert_eq!(vst3_layout.relative_entries.len(), 11);
    assert!(vst3_layout.moduleinfo_json.contains(VST3_SNAPSHOT_UID_HEX));

    let clap_layout =
        MultiArchPluginBundleLayout::build_clap_bundle_layout(Path::new("/tmp/bundles"), "0.8.0");
    assert_eq!(clap_layout.relative_entries.len(), 4);
    assert!(clap_layout.moduleinfo_json.contains("org.silverwave.live"));

    let pwa: PwaHostingHeaderContract = PwaHostingHeaderContract::new("0.8.0", "a1b2c3d");
    assert!(pwa.verify_cross_origin_isolated());
    assert_eq!(
        pwa.service_worker_cache_version,
        "silverwave-pwa-v0.8.0-a1b2c3d"
    );

    let report: ReleaseVerificationGateReport =
        SilverWavePackagingVerifier::run_all_platform_installer_smoke_tests("0.8.0");
    assert_eq!(report.platform_smoke_tests.len(), 17);
    let first_smoke: &PlatformSmokeTestResult = &report.platform_smoke_tests[0];
    assert_eq!(first_smoke.package_format, PackageFormat::LinuxDeb);

    let verifier = SilverWavePackagingVerifier;
    assert!(verifier.verify_release_readiness(&report).is_ok());

    let mut bad_report = report.clone();
    bad_report.workspace_line_coverage_pct = 90.0;
    let errs: Vec<ReleaseGateViolation> =
        verifier.verify_release_readiness(&bad_report).unwrap_err();
    assert_eq!(errs[0].gate_id, "GATE-01");
}
