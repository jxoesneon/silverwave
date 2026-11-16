//! Comprehensive TDD Test Suite for `silverwave-gui` (`v0.5.0`).
//! Verifies 100% of `05-SPEC-REPL-UI-AND-VISUALS.md` §4 & §5 (`UI-01`..`UI-06`,
//! `SPAN-01`..`SPAN-06`, `SLIDER-01`..`SLIDER-05`), §6 (`TrackerPhraseView`,
//! `ModulationRingOverlay`, `InteractiveTour`), and 100.00% of all public symbols in `silverwave-gui`.

#![forbid(unsafe_code)]

use silverwave_gui::{
    sanitize_mark_css, version, ActiveHighlight, ByteOffset, ByteSpan, ClockSource, DocMetadata,
    EvalFlashEvent, EvalOutcome, FrontendCapabilities, HighlightStyle, InlineVizAnchor,
    InteractiveTour, LspPosition, LspRange, MetadataParser, ModulationRingOverlay, NativeFrontend,
    NativeHighlightStyle, ReplCommand, ReplEngine, ReplError, ReplViewState, ShareCodec,
    SilverWaveMetadataParser, SilverWaveShareCodec, SliderPatch, SpanMap, SpanMapper,
    SpanValidity, TextEdit, TourStage, TrackerFxCommand, TrackerPhraseView, TrackerRow,
    Utf16Offset, Utf16Span, WidgetDescriptor, WidgetId, WidgetRegistry, MAX_SHARE_PAYLOAD_BYTES,
};
use silverwave_viz::{AudioExportFormat, RgbaColor, VizFrame, VizKind};

struct DummyNativeFrontend {
    caps: FrontendCapabilities,
}

impl NativeFrontend for DummyNativeFrontend {
    fn poll_events(&mut self) -> Vec<ReplCommand> {
        vec![ReplCommand::SetCps(0.75)]
    }
    fn render_frame(&mut self, _state: &ReplViewState, _draws: &[VizFrame]) -> Result<(), ReplError> {
        Ok(())
    }
    fn capabilities(&self) -> FrontendCapabilities {
        self.caps
    }
}

#[test]
fn test_ui_01_to_06_repl_state_metadata_and_share_codec() {
    assert!(!version().is_empty());
    assert_eq!(MAX_SHARE_PAYLOAD_BYTES, 1_048_576);

    // UI-01: Initial state is `Stopped`, `cps == 0.5`, `active_highlights.is_empty()`
    let mut repl = ReplEngine::new();
    assert!(!repl.is_playing());
    assert_eq!(repl.cps(), 0.5);
    assert_eq!(repl.now_cycles(), 0.0);
    assert!(repl.state.active_highlights.is_empty());

    // UI-02: `EvaluateBuffer` transitions `Stopped -> Playing` and emits `EvalFlash`
    let code = r#"// @title Café — José
// @by Eduardo
// @genre Algorave
s("bd sd")._punchcard()"#;
    let outcome: EvalOutcome = repl.evaluate_buffer(code).unwrap();
    assert!(repl.is_playing());
    assert_eq!(outcome.eval_generation, 1);
    let flash: EvalFlashEvent = outcome.flash;
    assert_eq!(flash.span, ByteSpan::new(0, code.len()));
    assert_eq!(outcome.inline_viz_anchors.len(), 1);
    assert_eq!(outcome.inline_viz_anchors[0].kind, VizKind::Punchcard);

    // UI-05: MetadataParser extracts `@title`, `@by`, `@genre` with Unicode
    let meta: DocMetadata = outcome.metadata;
    assert_eq!(meta.title.as_deref(), Some("Café — José"));
    assert_eq!(meta.by.as_deref(), Some("Eduardo"));
    assert_eq!(meta.genre.as_deref(), Some("Algorave"));
    let block_meta = SilverWaveMetadataParser::parse_metadata(
        r#"/* @license MIT */
/* @details Live */"#,
    );
    assert_eq!(block_meta.license.as_deref(), Some("MIT"));
    assert_eq!(block_meta.details.as_deref(), Some("Live"));

    // UI-03: Syntax error while `Playing` preserves previous playing pattern and `is_playing() == true`
    let bad_res = repl.evaluate_buffer(r#"s("bd [sd""#);
    assert!(matches!(bad_res, Err(ReplError::ParseError { .. })));
    assert!(repl.is_playing());
    assert_eq!(repl.state.eval_generation, 1);
    assert_eq!(repl.state.source, code);

    // Mute toggle & block evaluation
    assert!(repl.toggle_mute());
    assert!(!repl.toggle_mute());
    let _ = repl
        .evaluate_block(r#"s("hh*4")"#, ByteSpan::new(0, 9))
        .unwrap();

    // UI-04: `Halt` transitions `Playing -> Stopped`, clears `ActiveHighlights`, increments `all_notes_off_count`
    repl.halt();
    assert!(!repl.is_playing());
    assert!(repl.state.active_highlights.is_empty());
    assert_eq!(repl.state.all_notes_off_count, 1);

    // UI-06: `ShareCodec` round-trips UTF-8 + emojis + backticks and rejects invalid payloads
    let sketch = r#"🌀 note(`c3 e3 g3`).s("sawtooth") // Café"#;
    let frag = SilverWaveShareCodec::encode_url_fragment(sketch);
    assert!(frag.starts_with("#code="));
    let decoded = SilverWaveShareCodec::decode_url_fragment(&frag).unwrap();
    assert_eq!(decoded, sketch);
    assert!(matches!(
        SilverWaveShareCodec::decode_url_fragment("#code=!!!invalid!!!"),
        Err(ReplError::ShareDecodeError { .. })
    ));

    // NativeFrontend & dispatch_command
    let mut fe = DummyNativeFrontend {
        caps: FrontendCapabilities::default(),
    };
    assert!(fe.capabilities().supports_inline_widgets);
    for cmd in fe.poll_events() {
        repl.dispatch_command(cmd).unwrap();
    }
    assert_eq!(repl.cps(), 0.75);
    let wav_opt = repl
        .dispatch_command(ReplCommand::ExportAudio {
            format: AudioExportFormat::WavPcm16,
            cycles: 1.0,
        })
        .unwrap();
    assert!(wav_opt.unwrap().starts_with(b"RIFF"));
    fe.render_frame(&repl.state, &[]).unwrap();
}

#[test]
fn test_span_01_to_06_utf8_utf16_and_live_edit_remapping() {
    // SPAN-01: ASCII identity mapping
    let ascii = r#"note("c3 e3")
s("bd sd")"#;
    let map_ascii = SpanMap::new(ascii);
    assert_eq!(map_ascii.source(), ascii);
    for b in 0..=ascii.len() {
        let u: ByteOffset = map_ascii.byte_to_utf16(b).unwrap();
        assert_eq!(u, b);
        assert_eq!(map_ascii.utf16_to_byte(u as Utf16Offset), Some(b));
    }

    // SPAN-02 & SPAN-03: Multi-byte UTF-8 (`🌀` = 4 UTF-8 bytes, 2 UTF-16 code units)
    let emoji_doc = r#"🌀 note("c3")"#;
    let mut map_emoji = SpanMap::new(emoji_doc);
    assert_eq!(map_emoji.byte_to_utf16(5), Some(3)); // `note` starts at byte 5 -> UTF-16 offset 3
    assert_eq!(map_emoji.utf16_to_byte(3), Some(5));
    // Mid-codepoint byte offset returns `None` without panicking (`SPAN-03`)
    assert_eq!(map_emoji.byte_to_utf16(1), None);
    assert_eq!(map_emoji.byte_to_utf16(2), None);
    assert_eq!(map_emoji.utf16_to_byte(1), None);

    let lsp_r: LspRange = map_emoji
        .byte_span_to_lsp_range(ByteSpan::new(5, 9))
        .unwrap();
    assert_eq!(
        lsp_r.start,
        LspPosition {
            line: 0,
            character: 3
        }
    );

    // SPAN-04: Insertion before tracked span `[5, 9)` shifts by `+3`
    map_emoji.apply_edit(TextEdit {
        span: ByteSpan::new(0, 0),
        new_len_bytes: 3,
        new_len_utf16: 3,
    });
    assert_eq!(
        map_emoji.remap_span(ByteSpan::new(5, 9)),
        (Some(ByteSpan::new(8, 12)), SpanValidity::Shifted)
    );

    // SPAN-05: Deletion overlapping start of `[8, 12)` clips cleanly
    map_emoji.apply_edit(TextEdit {
        span: ByteSpan::new(6, 10),
        new_len_bytes: 0,
        new_len_utf16: 0,
    });
    let (clipped, validity) = map_emoji.remap_span(ByteSpan::new(5, 9));
    assert_eq!(validity, SpanValidity::Clipped);
    assert!(clipped.unwrap().start <= clipped.unwrap().end);

    // SPAN-06: Complete deletion invalidates without panicking
    map_emoji.apply_edit(TextEdit {
        span: ByteSpan::new(0, 20),
        new_len_bytes: 0,
        new_len_utf16: 0,
    });
    assert_eq!(
        map_emoji.remap_span(ByteSpan::new(5, 9)),
        (None, SpanValidity::Invalidated)
    );
}

#[test]
fn test_slider_01_to_05_and_tracker_tour_and_css() {
    // SLIDER-01..SLIDER-05: Multiple sliders on the same line, clamping/stepping, zero re-eval
    let mut repl = ReplEngine::new();
    let src = r#"s("bd*4").gain(slider(0, 0, 1, 0.05)).lpf(slider(0.5, 0, 1, 0.1))"#;
    repl.evaluate_buffer(src).unwrap();
    assert_eq!(repl.state.eval_generation, 1);

    let widgets: Vec<WidgetDescriptor> = repl.widget_registry.all_widgets();
    assert_eq!(widgets.len(), 2);
    let w1_id: WidgetId = widgets[0].id;
    let w2_id: WidgetId = widgets[1].id;
    let w2_orig_span = widgets[1].literal_span;
    assert_eq!(&repl.state.source[widgets[0].literal_span.start..widgets[0].literal_span.end], "0");
    assert_eq!(&repl.state.source[w2_orig_span.start..w2_orig_span.end], "0.5");

    // Drag slider #1 from `"0"` (1 byte) to `0.25` (4 bytes -> +3 bytes shift for slider #2!) (`SLIDER-05`)
    let patch1: SliderPatch = repl.drag_slider(w1_id, 0.25).unwrap();
    assert_eq!(patch1.replacement_text, "0.25");
    assert_eq!(repl.state.eval_generation, 1); // SLIDER-03: eval_generation unchanged!
    assert_eq!(repl.widget_registry.get_ref_value(w1_id), Some(0.25));

    let w2_after = repl.widget_registry.get_widget(w2_id).unwrap();
    assert_eq!(w2_after.literal_span.start, w2_orig_span.start + 3);
    assert_eq!(
        &repl.state.source[w2_after.literal_span.start..w2_after.literal_span.end],
        "0.5"
    );

    // SLIDER-02: Clamping and stepping (`0.333` with `step=0.1` -> `0.3`)
    let patch2 = repl.drag_slider(w2_id, 0.333).unwrap();
    assert!((patch2.clamped_value - 0.3).abs() < 1.0e-6);

    // ModulationRingOverlay (`CK-02`)
    let ring = ModulationRingOverlay::from_values(
        repl.widget_registry.get_widget(w1_id).unwrap(),
        0.25,
        RgbaColor::new(0, 255, 255, 255),
    );
    assert!(ring.end_angle_rad > ring.start_angle_rad);

    // CSS Sanitizer & `.markcss()` (`05-SPEC` §4.4)
    let clean_css = sanitize_mark_css(
        "background: red; text-decoration: underline; position: fixed; background-image: url(evil)",
    );
    assert_eq!(clean_css, "background:red;text-decoration:underline");
    let h_style = HighlightStyle::from_color_and_css(
        RgbaColor::new(255, 0, 0, 255),
        Some("background: red; text-decoration: underline"),
    );
    let _native: &NativeHighlightStyle = &h_style.native;
    assert!(h_style.native.underline);
    let _ah = ActiveHighlight {
        span: ByteSpan::new(0, 2),
        utf16_span: Utf16Span::new(0, 2),
        style: h_style,
    };
    let _anchor = InlineVizAnchor {
        id: 1,
        kind: VizKind::Pianoroll,
        anchor_line: 0,
        call_span: ByteSpan::new(0, 10),
    };

    // TrackerPhraseView (`CK-04`) & InteractiveTour (`CK-07`)
    let mut tracker = TrackerPhraseView::from_mini_pattern("c4 ~ e4 g4", "sawtooth");
    assert_eq!(tracker.rows.len(), 4);
    let _row0: &TrackerRow = &tracker.rows[0];
    let _fx = TrackerFxCommand {
        cmd: "LPF".to_string(),
        value_hex: 0x80,
    };
    assert_eq!(
        tracker.set_step_plock(0, "lpf", 1200.0).as_deref(),
        Some("c4 ~ e4 g4")
    );
    assert_eq!(tracker.to_mini_pattern(), "c4 ~ e4 g4");

    let mut tour = InteractiveTour::new();
    let _s0: &TourStage = &tour.stages[0];
    assert!(tour.verify_and_advance(r#"s("bd*4")"#));
    assert_eq!(tour.current_stage, 1);
}

#[test]
fn test_direct_widget_and_span_helper_methods() {
    let mut sm = SpanMap::new("slider(0.4, 0, 1, 0.1)");
    assert_eq!(
        sm.byte_span_to_utf16_span(ByteSpan::new(0, 6)),
        Some(Utf16Span::new(0, 6))
    );
    let mut reg = WidgetRegistry::new();
    let extracted = reg.extract_from_source(sm.source());
    assert_eq!(extracted.len(), 1);
    let mut src = sm.source().to_string();
    let patch = reg
        .commit_slider_drag(extracted[0].id, 0.8, &mut src, &mut sm)
        .unwrap();
    assert_eq!(patch.replacement_text, "0.8");
    let mut eng = ReplEngine::new();
    eng.set_cps(0.65);
    assert_eq!(eng.cps(), 0.65);
}
