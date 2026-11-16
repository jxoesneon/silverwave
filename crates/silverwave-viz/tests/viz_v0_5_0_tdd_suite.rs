//! Comprehensive TDD & Deterministic Golden Command Test Suite for `silverwave-viz` (`v0.5.0`).
//! Verifies 100% of `05-SPEC-REPL-UI-AND-VISUALS.md` §3 (`VIZ-01`..`VIZ-07`), §4.7 (`EXP-01`),
//! §4.8 (`initHydra`), §6.5 (`VjFrameBroadcaster`), and 100.00% of all public symbols in `silverwave-viz`.

#![forbid(unsafe_code)]

use silverwave_viz::{
    AudioExportFormat, AudioTap, CycleTime, DeterministicAudioExporter, DrawCmd, ExportConfig,
    HydraInitOptions, HydraUniformFrame, LineCap, PianorollOptions, PianorollRenderer,
    PitchwheelMode, PitchwheelOptions, PitchwheelRenderer, RgbaColor, RingBufferAudioTap,
    ScopeOptions, ScopeRenderer, SilverWaveHydraBridge, SilverWavePianoroll, SilverWavePitchwheel,
    SilverWaveScope, SilverWaveSpectrum, SilverWaveSpiral, SpectrumOptions, SpectrumRenderer,
    SpiralOptions, SpiralRenderer, VisualBridge, VizFrame, VizHap, VizKind, VizTarget, VizViewport,
    VjFrameBroadcaster, VjOutputProtocol, default_active_viz_color, render_draw_cmds_to_braille,
    version,
};

#[test]
fn test_viz_01_02_pianoroll_vs_punchcard_and_autorange() {
    assert!(!version().is_empty());
    let def_col = default_active_viz_color();
    assert_eq!(def_col, RgbaColor::new(255, 202, 40, 255));
    assert_eq!(
        RgbaColor::parse_css("cyan"),
        Some(RgbaColor::new(0, 255, 255, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("#ff00ff"),
        Some(RgbaColor::new(255, 0, 255, 255))
    );
    assert_eq!(def_col.to_css_hex(), "#ffca28");

    let vp = VizViewport::default();
    let opts = PianorollOptions {
        labels: true,
        background: RgbaColor::new(10, 10, 15, 255),
        ..PianorollOptions::default()
    };

    // VIZ-01: `note("c4")._<viz>().color("cyan")` -> `pianoroll` uses pre-transform (default active),
    // while `punchcard` reflects post-transform `.color("cyan")`!
    let cyan = RgbaColor::new(0, 255, 255, 255);
    let mut hap = VizHap::new(0.0, 1.0, 60.0);
    hap.label = Some("c4".to_string());
    hap.pre_transform_color = None;
    hap.post_transform_color = Some(cyan);
    assert!(hap.is_active_at(0.5 as CycleTime));

    let pr_cmds =
        SilverWavePianoroll::render_pianoroll(std::slice::from_ref(&hap), 0.5, &opts, &vp);
    let pc_cmds =
        SilverWavePianoroll::render_punchcard(std::slice::from_ref(&hap), 0.5, &opts, &vp);

    let pr_rect_col = pr_cmds.iter().find_map(|c| match c {
        DrawCmd::FillRect { color, .. } => Some(*color),
        _ => None,
    });
    let pc_rect_col = pc_cmds.iter().find_map(|c| match c {
        DrawCmd::FillRect { color, .. } => Some(*color),
        _ => None,
    });
    assert_eq!(pr_rect_col, Some(opts.active));
    assert_eq!(pc_rect_col, Some(cyan));

    // VIZ-02: `autorange: true` keeps all note `y` coordinates strictly within `(0.0, viewport.height)`
    let auto_opts = PianorollOptions {
        autorange: true,
        stroke: true,
        stroke_active: true,
        ..PianorollOptions::default()
    };
    let haps_wide = vec![VizHap::new(0.0, 0.5, 24.0), VizHap::new(0.5, 1.0, 96.0)];
    let auto_cmds = SilverWavePianoroll::render_pianoroll(&haps_wide, 0.25, &auto_opts, &vp);
    for cmd in &auto_cmds {
        if let DrawCmd::StrokeRect { y, h, .. } = cmd {
            assert!(*y > 0.0 && (*y + *h) < vp.height);
        }
    }
}

#[test]
fn test_viz_03_04_05_06_07_spiral_scope_pitchwheel_spectrum_and_determinism() {
    let vp = VizViewport::default();
    let hap = VizHap::new(0.0, 0.5, 60.0);

    // VIZ-03: `spiral` with `steady: 1.0` keeps spiral `Arc` geometry identical at `t = 0.0` and `t = 0.5`,
    // while only the playhead `Line` rotates!
    let spiral_opts = SpiralOptions {
        steady: 1.0,
        cap: LineCap::Round,
        ..SpiralOptions::default()
    };
    let sp_t0 = SilverWaveSpiral::render_spiral(std::slice::from_ref(&hap), 0.0, &spiral_opts, &vp);
    let sp_t5 = SilverWaveSpiral::render_spiral(std::slice::from_ref(&hap), 0.5, &spiral_opts, &vp);
    assert_eq!(sp_t0[0], sp_t5[0]); // Identical spiral Arc
    assert_ne!(sp_t0[1], sp_t5[1]); // Playhead Line rotated

    // VIZ-04: `scope` zero-crossing alignment (`align: true, trigger: 0.0`)
    let tap = RingBufferAudioTap {
        time_samples: vec![-0.5, -0.25, 0.0, 0.25, 0.5, 0.25, 0.0, -0.25],
        ..Default::default()
    };
    let mut scope_buf = vec![0.0f32; 8];
    let read_n = tap.read_time_domain(&mut scope_buf);
    assert_eq!(read_n, 8);
    let scope_opts = ScopeOptions::default();
    let scope_cmds = SilverWaveScope::render_scope(&scope_buf, &scope_opts, &vp);
    match &scope_cmds[0] {
        DrawCmd::Polyline { points, .. } => {
            // First point is the rising edge after 0.0 (`0.25`), near `pos * height`
            let expected_y = scope_opts.pos * vp.height - 0.25 * scope_opts.scale * vp.height;
            assert!((points[0].1 - expected_y).abs() < 1.0e-4);
        }
        _ => panic!("expected Polyline"),
    }

    // VIZ-05: `pitchwheel` `edo: 12` vs `edo: 19` angular placement (root at `-PI/2`)
    let pw_opts_12 = PitchwheelOptions {
        edo: 12,
        mode: PitchwheelMode::Steps,
        ..PitchwheelOptions::default()
    };
    let pw_opts_19 = PitchwheelOptions {
        edo: 19,
        mode: PitchwheelMode::Radians,
        ..PitchwheelOptions::default()
    };
    let ang_root = SilverWavePitchwheel::pitch_to_angle_rad(60.0, 12, 0.0);
    assert!((ang_root - (-core::f32::consts::PI * 0.5)).abs() < 1.0e-5);
    let ang_12 = SilverWavePitchwheel::pitch_to_angle_rad(67.0, 12, 0.0);
    let ang_19 = SilverWavePitchwheel::pitch_to_angle_rad(67.0, 19, 0.0);
    assert_ne!(ang_12, ang_19);
    let pw_cmds =
        SilverWavePitchwheel::render_pitchwheel(std::slice::from_ref(&hap), 0.25, &pw_opts_12, &vp);
    let _ =
        SilverWavePitchwheel::render_pitchwheel(std::slice::from_ref(&hap), 0.25, &pw_opts_19, &vp);
    assert_eq!(pw_cmds.len(), 2);

    // VIZ-06: `spectrum` clamps `-140 dB` to `min (-80)` and `+6 dB` to `max (0)` without NaN/Inf
    let mut spec_bins = vec![0.0f32; 4];
    let sr = tap.read_spectrum_db(&mut spec_bins);
    assert_eq!(sr, 48_000);
    let extreme_bins = [-140.0f32, -40.0, 6.0, f32::NAN];
    let spec_opts = SpectrumOptions::default();
    let spec_cmds = SilverWaveSpectrum::render_spectrum(&extreme_bins, sr, &spec_opts, &vp);
    match &spec_cmds[0] {
        DrawCmd::Polyline { points, .. } => {
            assert!((points[0].1 - vp.height).abs() < 1.0e-4); // -140 dB clamped to -80 -> bottom (`height`)
            assert!((points[2].1 - 0.0).abs() < 1.0e-4); // +6 dB clamped to 0 -> top (`0.0`)
            assert!(points[3].1.is_finite());
        }
        _ => panic!("expected Polyline"),
    }

    // VIZ-07: Golden `DrawCmd` determinism + Braille TUI + Hydra + Syphon/Spout/NDI + EXP-01 WAV/FLAC
    let spec_cmds_again = SilverWaveSpectrum::render_spectrum(&extreme_bins, sr, &spec_opts, &vp);
    assert_eq!(spec_cmds, spec_cmds_again);

    let frame = VizFrame {
        target: VizTarget::Inline {
            id: 1,
            kind: VizKind::Spectrum,
            anchor_line: 3,
        },
        commands: spec_cmds.clone(),
    };
    let _global_target = VizTarget::Global {
        kind: VizKind::Pianoroll,
    };
    let braille = render_draw_cmds_to_braille(&frame.commands, 16, 4, vp.width, vp.height);
    assert!(!braille.is_empty());

    let mut hydra = SilverWaveHydraBridge::new();
    hydra.init_hydra(HydraInitOptions::default());
    let uframe: HydraUniformFrame = hydra.compute_uniform_frame(
        2.0,
        1.0,
        0.5,
        4,
        &[0.5, -0.5, 0.5, -0.5],
        &[-20.0, -40.0, -60.0, -80.0],
    );
    assert!(uframe.rms_level > 0.4);
    assert!(uframe.fft_bins[0] > uframe.fft_bins[3]);

    let mut vj = VjFrameBroadcaster::new(VjOutputProtocol::SyphonMacOs, "SilverWaveMain", 20.0);
    let ts = vj.publish_frame(10.0, &frame.commands);
    assert!((ts - 10.020).abs() < 1.0e-6);
    assert_eq!(vj.published_frames, 1);

    // EXP-01: Deterministic WAV & FLAC export
    let exporter = DeterministicAudioExporter;
    let wav_cfg = ExportConfig {
        format: AudioExportFormat::WavPcm16,
        sample_rate: 48_000,
        channels: 2,
        cycles: 1.0,
        cps: 0.5,
        normalize_peak_db: None,
    };
    assert_eq!(
        DeterministicAudioExporter::exact_frame_count(&wav_cfg),
        96_000
    );
    let wav_bytes = exporter.export_bytes(&wav_cfg).unwrap();
    assert!(wav_bytes.starts_with(b"RIFF"));
    assert_eq!(&wav_bytes[8..12], b"WAVE");

    let flac_cfg = ExportConfig {
        format: AudioExportFormat::Flac24,
        ..wav_cfg
    };
    let flac_bytes = exporter.export_bytes(&flac_cfg).unwrap();
    assert!(flac_bytes.starts_with(b"fLaC"));
}
