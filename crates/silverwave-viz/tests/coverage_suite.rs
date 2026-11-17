//! Coverage completion suite for `silverwave-viz` — exercises edge branches and
//! alternate code paths not hit by `viz_v0_5_0_tdd_suite.rs`.

#![forbid(unsafe_code)]

use silverwave_viz::{
    AudioExportFormat, DeterministicAudioExporter, DrawCmd, ExportConfig, HydraInitOptions,
    HydraUniformFrame, LineCap, PianorollOptions, PianorollRenderer, PitchwheelOptions,
    PitchwheelRenderer, RgbaColor, ScopeOptions, ScopeRenderer, SilverWaveHydraBridge,
    SilverWavePianoroll, SilverWavePitchwheel, SilverWaveScope, SilverWaveSpectrum,
    SilverWaveSpiral, SpectrumOptions, SpectrumRenderer, SpiralOptions, SpiralRenderer,
    VisualBridge, VizHap, VizViewport, render_draw_cmds_to_braille,
};

/// `bridges.rs` — `HydraUniformFrame::default()` and `compute_uniform_frame` edge paths
/// (empty audio -> `rms == 0.0`, `detect_audio` FFT binning loop).
#[test]
fn test_hydra_uniform_frame_default_and_empty_audio() {
    let frame = HydraUniformFrame::default();
    assert_eq!(frame.cps, 0.5);
    assert_eq!(frame.fft_bins, [0.0; 4]);
    assert_eq!(frame.rms_level, 0.0);
    assert_eq!(frame.peak_level, 0.0);
    assert_eq!(frame.active_hap_count, 0);

    let mut bridge = SilverWaveHydraBridge::new();
    bridge.init_hydra(HydraInitOptions::default());
    assert!(bridge.initialized);

    // Empty `audio_samples` -> `rms == 0.0` early branch; non-empty spectrum -> FFT bin loop.
    let uf = bridge.compute_uniform_frame(1.0, 2.0, 0.5, 3, &[], &[-10.0; 8]);
    assert_eq!(uf.rms_level, 0.0);
    assert_eq!(uf.peak_level, 0.0);
    assert_eq!(uf.active_hap_count, 3);
    for bin in uf.fft_bins {
        assert!((bin - 0.875).abs() < 1.0e-5); // (-10 + 80) / 80
    }

    // `detect_audio: false` skips the FFT loop entirely; non-finite audio contributes 0.
    let mut quiet_opts = HydraInitOptions::default();
    quiet_opts.detect_audio = false;
    bridge.init_hydra(quiet_opts);
    let uf2 =
        bridge.compute_uniform_frame(0.0, 0.0, 0.5, 0, &[f32::NAN, f32::INFINITY], &[-40.0; 4]);
    assert_eq!(uf2.fft_bins, [0.0; 4]);
    assert_eq!(uf2.rms_level, 0.0);
    assert_eq!(uf2.peak_level, 0.0);
}

/// `bridges.rs` — every `DrawCmd` variant arm of `render_draw_cmds_to_braille`,
/// including off-viewport `plot()` coordinates and the `Clear` no-op arm.
#[test]
fn test_braille_all_draw_cmd_variants() {
    let col = RgbaColor::new(255, 255, 255, 255);
    let cmds = vec![
        DrawCmd::Clear { color: col },
        DrawCmd::FillRect {
            x: 10.0,
            y: 10.0,
            w: 40.0,
            h: 20.0,
            color: col,
        },
        DrawCmd::StrokeRect {
            x: 100.0,
            y: 30.0,
            w: 50.0,
            h: 10.0,
            thickness: 1.0,
            color: col,
        },
        DrawCmd::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 200.0,
            y2: 50.0,
            thickness: 1.0,
            cap: LineCap::Butt,
            color: col,
        },
        DrawCmd::Polyline {
            points: vec![(0.0, 0.0), (100.0, 50.0), (199.0, 10.0)],
            thickness: 1.0,
            cap: LineCap::Round,
            color: col,
            closed: false,
        },
        DrawCmd::Circle {
            cx: 150.0,
            cy: 25.0,
            radius: 10.0,
            fill: Some(col),
            stroke: None,
        },
        DrawCmd::Arc {
            cx: 50.0,
            cy: 40.0,
            radius: 12.0,
            start_rad: 0.0,
            end_rad: 3.0,
            thickness: 1.0,
            cap: LineCap::Square,
            color: col,
        },
        DrawCmd::Text {
            x: 20.0,
            y: 20.0,
            content: "bd".to_string(),
            font_family: "mono".to_string(),
            size_px: 10.0,
            color: col,
        },
        // Off-viewport points exercise `plot()`'s bounds check.
        DrawCmd::Circle {
            cx: -50.0,
            cy: -50.0,
            radius: 1.0,
            fill: None,
            stroke: None,
        },
        DrawCmd::Text {
            x: 10_000.0,
            y: 9_999.0,
            content: "x".to_string(),
            font_family: "m".to_string(),
            size_px: 1.0,
            color: col,
        },
    ];
    let out = render_draw_cmds_to_braille(&cmds, 16, 4, 200.0, 50.0);
    assert_eq!(out.lines().count(), 4);
    assert!(out.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c)));

    // Degenerate `cols`/`rows` are clamped to 1.
    let tiny = render_draw_cmds_to_braille(&cmds, 0, 0, 200.0, 50.0);
    assert_eq!(tiny.chars().count(), 1);

    // Empty command stream -> all-blank braille grid.
    let blank = render_draw_cmds_to_braille(&[], 2, 1, 10.0, 10.0);
    assert_eq!(blank, "\u{2800}\u{2800}");
}

/// `bridges.rs` — `ExportConfig::default()`, the WavPcm24 / WavFloat32
/// `bytes_per_sample` and `audio_fmt` arms, and the invalid-config error return.
#[test]
fn test_export_config_default_and_wav_variants() {
    let cfg = ExportConfig::default();
    assert_eq!(cfg.format, AudioExportFormat::WavPcm16);
    assert_eq!(cfg.sample_rate, 48_000);
    assert_eq!(cfg.channels, 2);
    assert_eq!(cfg.cycles, 4.0);
    assert_eq!(cfg.cps, 0.5);
    assert_eq!(cfg.normalize_peak_db, None);

    let exporter = DeterministicAudioExporter;

    // WavPcm24 -> `bytes_per_sample == 3`, PCM `audio_fmt == 1`.
    let wav24 = ExportConfig {
        format: AudioExportFormat::WavPcm24,
        cycles: 0.01,
        ..cfg.clone()
    };
    let b24 = exporter.export_bytes(&wav24).unwrap();
    assert!(b24.starts_with(b"RIFF"));
    assert_eq!(&b24[20..22], &1u16.to_le_bytes()); // PCM
    assert_eq!(&b24[34..36], &24u16.to_le_bytes()); // bits per sample
    let frames24 = DeterministicAudioExporter::exact_frame_count(&wav24);
    assert_eq!(b24.len(), 44 + frames24 * 2 * 3);

    // WavFloat32 -> `bytes_per_sample == 4`, IEEE float `audio_fmt == 3`.
    let wavf = ExportConfig {
        format: AudioExportFormat::WavFloat32,
        cycles: 0.01,
        ..cfg.clone()
    };
    let bf = exporter.export_bytes(&wavf).unwrap();
    assert_eq!(&bf[20..22], &3u16.to_le_bytes()); // IEEE float
    assert_eq!(&bf[34..36], &32u16.to_le_bytes());

    // Invalid configuration -> `Err` early return.
    for bad in [
        ExportConfig {
            sample_rate: 0,
            ..cfg.clone()
        },
        ExportConfig {
            channels: 0,
            ..cfg.clone()
        },
        ExportConfig {
            cycles: 0.0,
            ..cfg.clone()
        },
        ExportConfig {
            cycles: -2.0,
            ..cfg
        },
    ] {
        let err = exporter.export_bytes(&bad).unwrap_err();
        assert_eq!(err, "invalid export configuration");
    }

    // Flac16 path as well (Flac24 already covered elsewhere).
    let flac16 = ExportConfig {
        format: AudioExportFormat::Flac16,
        ..ExportConfig::default()
    };
    assert!(exporter.export_bytes(&flac16).unwrap().starts_with(b"fLaC"));
}

/// `renderers.rs` — `pianoroll` early-`continue` arms, inactive-color branch,
/// vertical layout, flip flags, and stroke rendering.
#[test]
fn test_pianoroll_edge_branches() {
    let vp = VizViewport::default();
    let now = 1.0f64;

    let mut hidden = VizHap::new(-2.0, -1.0, 60.0); // `end_cycle < 0` -> `hide_negative` skip
    hidden.label = Some("neg".to_string());
    let far = VizHap::new(10.0, 11.0, 60.0); // outside render window -> window skip
    let inactive = VizHap::new(0.0, 0.5, 64.0); // inside window but not active at `now`
    let active = VizHap::new(0.5, 2.0, 72.0);

    let opts = PianorollOptions {
        hide_negative: true,
        hide_inactive: true,
        ..PianorollOptions::default()
    };
    let cmds = SilverWavePianoroll::render_pianoroll(
        &[hidden, far, inactive, active.clone()],
        now,
        &opts,
        &vp,
    );
    // Only `active` rect + playhead line.
    assert_eq!(cmds.len(), 2);
    assert!(matches!(cmds[0], DrawCmd::FillRect { .. }));
    assert!(matches!(cmds[1], DrawCmd::Line { .. }));

    // `colorize_inactive: false` -> raw `opts.inactive` for non-active haps.
    let opts2 = PianorollOptions {
        colorize_inactive: false,
        inactive: RgbaColor::new(1, 2, 3, 255),
        ..PianorollOptions::default()
    };
    let mut colored_inactive = VizHap::new(0.0, 0.5, 64.0);
    colored_inactive.pre_transform_color = Some(RgbaColor::new(9, 9, 9, 255));
    let cmds2 = SilverWavePianoroll::render_pianoroll(&[colored_inactive], now, &opts2, &vp);
    let fill_col = cmds2.iter().find_map(|c| match c {
        DrawCmd::FillRect { color, .. } => Some(*color),
        _ => None,
    });
    assert_eq!(fill_col, Some(RgbaColor::new(1, 2, 3, 255)));

    // `vertical: true` + `flip_time` + `flip_values` -> vertical coordinate tuple & flipped axes.
    let opts3 = PianorollOptions {
        vertical: true,
        flip_time: true,
        flip_values: true,
        stroke: true,
        labels: true,
        background: RgbaColor::new(0, 0, 0, 255),
        ..PianorollOptions::default()
    };
    let mut labeled = VizHap::new(0.5, 2.0, 72.0);
    labeled.label = Some("c5".to_string());
    let cmds3 = SilverWavePianoroll::render_pianoroll(&[labeled], now, &opts3, &vp);
    assert!(matches!(cmds3[0], DrawCmd::Clear { .. })); // opaque background -> Clear
    assert!(
        cmds3
            .iter()
            .any(|c| matches!(c, DrawCmd::StrokeRect { .. }))
    );
    assert!(
        cmds3
            .iter()
            .any(|c| matches!(c, DrawCmd::Text { content, .. } if content == "c5"))
    );

    // `stroke_active` on an active hap produces StrokeRect too.
    let opts4 = PianorollOptions {
        stroke_active: true,
        ..PianorollOptions::default()
    };
    let cmds4 = SilverWavePianoroll::render_pianoroll(&[active], now, &opts4, &vp);
    assert!(
        cmds4
            .iter()
            .any(|c| matches!(c, DrawCmd::StrokeRect { .. }))
    );
}

/// `renderers.rs` — `spiral` `rel_end <= 0` skip, `log_spiral` radius arm, and all
/// color-resolution branches (steady / active / colorize_inactive / plain inactive).
#[test]
fn test_spiral_edge_branches() {
    let vp = VizViewport::default();

    // `rel_end <= 0.0` -> `continue`; also covers `log_spiral` via a retained hap.
    let opts = SpiralOptions {
        log_spiral: true,
        ..SpiralOptions::default()
    };
    let skipped = VizHap::new(-20.0, -15.0, 60.0);
    let kept = VizHap::new(0.0, 1.0, 60.0);
    let cmds = SilverWaveSpiral::render_spiral(&[skipped, kept.clone()], 0.25, &opts, &vp);
    assert_eq!(cmds.len(), 2); // one Arc + playhead Line
    assert!(matches!(cmds[0], DrawCmd::Arc { .. }));

    // Active hap with `post_transform_color` -> custom color wins.
    let mut colored = VizHap::new(0.0, 1.0, 60.0);
    colored.post_transform_color = Some(RgbaColor::new(0, 255, 0, 255));
    let cmds_a = SilverWaveSpiral::render_spiral(&[colored], 0.25, &SpiralOptions::default(), &vp);
    if let DrawCmd::Arc { color, .. } = cmds_a[0] {
        assert_eq!(color, RgbaColor::new(0, 255, 0, 255));
    } else {
        panic!("expected Arc");
    }

    // Inactive hap + `colorize_inactive: true` -> `post_transform_color.unwrap_or(inactive_col)`.
    let mut inactive_colored = VizHap::new(2.0, 3.0, 60.0);
    inactive_colored.post_transform_color = Some(RgbaColor::new(7, 8, 9, 255));
    let opts_ci = SpiralOptions {
        colorize_inactive: true,
        ..SpiralOptions::default()
    };
    let cmds_i = SilverWaveSpiral::render_spiral(&[inactive_colored], 0.25, &opts_ci, &vp);
    if let DrawCmd::Arc { color, .. } = cmds_i[0] {
        assert_eq!(color, RgbaColor::new(7, 8, 9, 255));
    } else {
        panic!("expected Arc");
    }

    // Inactive hap + `colorize_inactive: false` -> bare `inactive_col`.
    let plain_inactive = VizHap::new(2.0, 3.0, 60.0);
    let gutter = RgbaColor::new(4, 5, 6, 255);
    let opts_plain = SpiralOptions {
        inactive_color: Some(gutter),
        ..SpiralOptions::default()
    };
    let cmds_p = SilverWaveSpiral::render_spiral(&[plain_inactive], 0.25, &opts_plain, &vp);
    if let DrawCmd::Arc { color, .. } = cmds_p[0] {
        assert_eq!(color, gutter);
    } else {
        panic!("expected Arc");
    }

    // `steady >= 1.0` -> `inactive_col` regardless of activity.
    let active_hap = VizHap::new(0.0, 1.0, 60.0);
    let opts_steady = SpiralOptions {
        steady: 1.0,
        inactive_color: Some(gutter),
        ..SpiralOptions::default()
    };
    let cmds_s = SilverWaveSpiral::render_spiral(&[active_hap], 0.25, &opts_steady, &vp);
    if let DrawCmd::Arc { color, .. } = cmds_s[0] {
        assert_eq!(color, gutter);
    } else {
        panic!("expected Arc");
    }

    // Explicit `size` + `padding` exercise the diameter expression.
    let opts_sized = SpiralOptions {
        size: Some(500.0),
        padding: 20.0,
        ..SpiralOptions::default()
    };
    let _ = SilverWaveSpiral::render_spiral(&[kept], 0.25, &opts_sized, &vp);
}

/// `renderers.rs` — `scope` empty-input early return, rising-edge `break`,
/// no-crossing scan, single-sample `n <= 1`, and non-finite clamping.
#[test]
fn test_scope_edge_branches() {
    let vp = VizViewport::default();
    let opts = ScopeOptions::default();

    // Empty input -> `Vec::new()`.
    assert!(SilverWaveScope::render_scope(&[], &opts, &vp).is_empty());

    // Rising-edge trigger crossing -> `found = w + 1; break`.
    let crossing = [-0.5f32, -0.25, 0.3, 0.1, -0.4, -0.1, 0.2, 0.0];
    let cmds = SilverWaveScope::render_scope(&crossing, &opts, &vp);
    if let DrawCmd::Polyline { points, .. } = &cmds[0] {
        assert_eq!(points.len(), 6); // slice starts at index 2
    } else {
        panic!("expected Polyline");
    }

    // No crossing -> `found` stays 0, whole buffer rendered.
    let all_neg = [-0.5f32; 8];
    let cmds_nc = SilverWaveScope::render_scope(&all_neg, &opts, &vp);
    if let DrawCmd::Polyline { points, .. } = &cmds_nc[0] {
        assert_eq!(points.len(), 8);
    } else {
        panic!("expected Polyline");
    }

    // `align: false` skips trigger search; single sample -> `n <= 1` -> `x == 0.0`.
    let no_align = ScopeOptions {
        align: false,
        ..ScopeOptions::default()
    };
    let one = SilverWaveScope::render_scope(&[0.5], &no_align, &vp);
    if let DrawCmd::Polyline { points, .. } = &one[0] {
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].0, 0.0);
    } else {
        panic!("expected Polyline");
    }

    // Non-finite samples clamp to 0 contribution; out-of-range samples clamp to ±2.
    let weird = [f32::NAN, f32::INFINITY, 5.0, -5.0];
    let cmds_w = SilverWaveScope::render_scope(&weird, &no_align, &vp);
    if let DrawCmd::Polyline { points, .. } = &cmds_w[0] {
        for &(_, y) in points {
            assert!(y.is_finite());
        }
    } else {
        panic!("expected Polyline");
    }
}

/// `renderers.rs` — `pitch_to_angle_rad` for `edo == 0`, every `parse_root_pc`
/// match arm via `render_pitchwheel`, `circle == 0` skip, and the
/// `hapcircles == 0` inactive-hap `continue`.
#[test]
fn test_pitchwheel_all_roots_and_edges() {
    let vp = VizViewport::default();
    let active = VizHap::new(0.0, 1.0, 60.0);
    let future = VizHap::new(5.0, 6.0, 67.0);

    // `edo == 0` clamps to 1 division.
    let a = SilverWavePitchwheel::pitch_to_angle_rad(60.0, 0, 0.0);
    assert!((a - (-core::f32::consts::PI * 0.5)).abs() < 1.0e-5);

    // Every note-name arm of `parse_root_pc` (incl. flats, lowercase, whitespace, fallback).
    let roots = [
        "C", "C#", "DB", "D", "D#", "EB", "E", "F", "F#", "GB", "G", "G#", "AB", "A", "A#", "BB",
        "B", " db ", "x9", "",
    ];
    for root in roots {
        let opts = PitchwheelOptions {
            root: root.to_string(),
            ..PitchwheelOptions::default()
        };
        let cmds =
            SilverWavePitchwheel::render_pitchwheel(std::slice::from_ref(&active), 0.5, &opts, &vp);
        assert_eq!(cmds.len(), 2); // ring circle + hap circle
    }

    // `circle == 0` -> no ring; `hapcircles == 0` + inactive hap -> skipped hap dot.
    let opts = PitchwheelOptions {
        circle: 0,
        hapcircles: 0,
        ..PitchwheelOptions::default()
    };
    let cmds = SilverWavePitchwheel::render_pitchwheel(&[future.clone()], 0.5, &opts, &vp);
    assert!(cmds.is_empty());

    // Same setup but an active hap still renders its dot (inactive `theme_gutter_fg` otherwise).
    let opts2 = PitchwheelOptions {
        circle: 0,
        hapcircles: 0,
        ..PitchwheelOptions::default()
    };
    let cmds2 =
        SilverWavePitchwheel::render_pitchwheel(std::slice::from_ref(&active), 0.5, &opts2, &vp);
    assert_eq!(cmds2.len(), 1);

    // Inactive hap with `hapcircles > 0` uses `theme_gutter_fg` fill.
    let mut inactive_colored = future;
    inactive_colored.post_transform_color = Some(RgbaColor::new(9, 9, 9, 255));
    let opts3 = PitchwheelOptions {
        circle: 0,
        hapcircles: 1,
        ..PitchwheelOptions::default()
    };
    let cmds3 = SilverWavePitchwheel::render_pitchwheel(&[inactive_colored], 0.5, &opts3, &vp);
    if let DrawCmd::Circle { fill, .. } = cmds3[0] {
        assert_eq!(fill, Some(vp.theme_gutter_fg));
    } else {
        panic!("expected Circle");
    }
}

/// `renderers.rs` — `spectrum` empty-input return, per-value clamp arms
/// (finite, `+INF`, `-INF`/`NaN`), and single-bin `n <= 1` x-position.
#[test]
fn test_spectrum_edge_values() {
    let vp = VizViewport::default();
    let opts = SpectrumOptions::default();

    assert!(SilverWaveSpectrum::render_spectrum(&[], 48_000, &opts, &vp).is_empty());

    // `min >= max` clamps `max_db` to `min + 1`.
    let clamped_opts = SpectrumOptions {
        min: 0,
        max: 0,
        thickness: 0,
        speed: 1,
    };
    let bins = [f32::INFINITY, f32::NEG_INFINITY, f32::NAN, -0.5];
    let cmds = SilverWaveSpectrum::render_spectrum(&bins, 48_000, &clamped_opts, &vp);
    if let DrawCmd::Polyline {
        points, thickness, ..
    } = &cmds[0]
    {
        assert_eq!(points.len(), 4);
        for &(_, y) in points {
            assert!(y.is_finite());
        }
        assert_eq!(points[0].1, 0.0); // +INF -> max_db -> top
        assert!((points[1].1 - vp.height).abs() < 1.0e-4); // -INF -> min_db -> bottom
        assert_eq!(*thickness, 1.0); // `thickness.max(1)`
    } else {
        panic!("expected Polyline");
    }

    // Single bin -> `n <= 1` -> `x == 0.0`.
    let one = SilverWaveSpectrum::render_spectrum(&[-40.0], 48_000, &opts, &vp);
    if let DrawCmd::Polyline { points, .. } = &one[0] {
        assert_eq!(points[0].0, 0.0);
        assert!((points[0].1 - vp.height * 0.5).abs() < 1.0e-4); // -40 of -80..0
    } else {
        panic!("expected Polyline");
    }
}

/// `types.rs` — every `RgbaColor::parse_css` arm (all named colors, 6- and 8-digit
/// hex, bad lengths, bad digits, unknown names) and both `to_css_hex` forms.
#[test]
fn test_rgba_color_parse_css_and_hex() {
    assert_eq!(
        RgbaColor::parse_css("cyan"),
        Some(RgbaColor::new(0, 255, 255, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("magenta"),
        Some(RgbaColor::new(255, 0, 255, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("yellow"),
        Some(RgbaColor::new(255, 255, 0, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("red"),
        Some(RgbaColor::new(255, 0, 0, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("green"),
        Some(RgbaColor::new(0, 255, 0, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("blue"),
        Some(RgbaColor::new(0, 0, 255, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("white"),
        Some(RgbaColor::new(255, 255, 255, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("black"),
        Some(RgbaColor::new(0, 0, 0, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("transparent"),
        Some(RgbaColor::new(0, 0, 0, 0))
    );
    assert_eq!(
        RgbaColor::parse_css("amber"),
        Some(RgbaColor::new(255, 202, 40, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("GOLD"),
        Some(RgbaColor::new(255, 202, 40, 255))
    );

    // 6- and 8-digit hex, plus surrounding whitespace/case handling.
    assert_eq!(
        RgbaColor::parse_css(" #112233 "),
        Some(RgbaColor::new(0x11, 0x22, 0x33, 255))
    );
    assert_eq!(
        RgbaColor::parse_css("#A1B2C3D4"),
        Some(RgbaColor::new(0xA1, 0xB2, 0xC3, 0xD4))
    );

    // Bad hex length (5 / 7 / 9 digits) -> `None` else-arm; non-hex digits -> `?` early `None`.
    assert_eq!(RgbaColor::parse_css("#12345"), None);
    assert_eq!(RgbaColor::parse_css("#1234567"), None);
    assert_eq!(RgbaColor::parse_css("#zzzzzz"), None);
    assert_eq!(RgbaColor::parse_css("#12345zz0"), None);
    assert_eq!(RgbaColor::parse_css("octarine"), None);
    assert_eq!(RgbaColor::parse_css(""), None);
    assert_eq!(RgbaColor::parse_css("#"), None);

    // `to_css_hex` emits the alpha suffix only when `a != 255`.
    assert_eq!(RgbaColor::new(1, 2, 3, 255).to_css_hex(), "#010203");
    assert_eq!(RgbaColor::new(1, 2, 3, 128).to_css_hex(), "#01020380");
    assert_eq!(RgbaColor::new(0, 0, 0, 0).to_css_hex(), "#00000000");
}
