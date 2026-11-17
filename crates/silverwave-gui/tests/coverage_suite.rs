//! Coverage completion suite for `silverwave-gui` — exercises edge branches and
//! alternate code paths not hit by `gui_v0_5_0_tdd_suite.rs`.

#![forbid(unsafe_code)]

use silverwave_gui::{
    ByteSpan, ClockSource, DocMetadata, HighlightStyle, InteractiveTour, MetadataParser,
    ModulationRingOverlay, ReplCommand, ReplEngine, ReplError, ShareCodec,
    SilverWaveMetadataParser, SilverWaveShareCodec, SpanMap, SpanMapper, SpanValidity, TextEdit,
    TrackerPhraseView, Utf16Span, WidgetRegistry, sanitize_mark_css,
};
use silverwave_viz::{AudioExportFormat, RgbaColor};

/// `metadata.rs` — every metadata key arm, `@author`/`@description` aliases,
/// `extra` fallback, empty-value `continue`, and the empty-key `_ => {}` arm.
#[test]
fn test_metadata_parser_all_key_arms() {
    let src = "\
// @title My Live Set
// @author Eduardo
/* @genre Algorave */
* @license CC0
// @description A long form piece
// @custom_tag some value
// @empty
// @ dangling-value
plain code line s(\"bd\")
//@bare
";
    let meta: DocMetadata = SilverWaveMetadataParser::parse_metadata(src);
    assert_eq!(meta.title.as_deref(), Some("My Live Set"));
    assert_eq!(meta.by.as_deref(), Some("Eduardo")); // `@author` alias
    assert_eq!(meta.genre.as_deref(), Some("Algorave"));
    assert_eq!(meta.license.as_deref(), Some("CC0"));
    assert_eq!(meta.details.as_deref(), Some("A long form piece")); // `@description` alias
    assert_eq!(
        meta.extra,
        vec![("custom_tag".to_string(), "some value".to_string())]
    );
    // `// @empty` (empty value -> `continue`), `// @ dangling-value` (empty key -> `_ => {}`),
    // and `//@bare` (empty value) leave no residue.
    assert_eq!(meta.extra.len(), 1);

    // `@by` direct arm (not the alias) and CRLF line endings.
    let crlf = SilverWaveMetadataParser::parse_metadata("// @by Ana\r\n// @title T\r\n");
    assert_eq!(crlf.by.as_deref(), Some("Ana"));
    assert_eq!(crlf.title.as_deref(), Some("T"));
}

/// `metadata.rs` — base64url encoder remainder arms (`len % 3 == 0 / 1 / 2`).
#[test]
fn test_share_codec_encode_remainders() {
    for src in ["abc", "a", "ab", "abcd", "hello world"] {
        let frag = SilverWaveShareCodec::encode_url_fragment(src);
        assert!(frag.starts_with("#code="));
        assert!(!frag["#code=".len()..].contains('=')); // no padding emitted
        let decoded = SilverWaveShareCodec::decode_url_fragment(&frag).unwrap();
        assert_eq!(decoded, src);
    }
}

/// `metadata.rs` — decoder: alternate prefixes, empty payload, bad length,
/// every invalid-character `ok_or_else` arm, `-`/`_` alphabet chars,
/// non-UTF-8 payload, and both 1 MiB size guards.
#[test]
fn test_share_codec_decode_all_paths() {
    // Prefix variants: `#code=`, `code=`, `#sw=`, and bare payload.
    let frag = SilverWaveShareCodec::encode_url_fragment("bd sd");
    let payload = frag.strip_prefix("#code=").unwrap().to_string();
    for candidate in [
        frag.clone(),
        format!("code={payload}"),
        format!("#sw={payload}"),
        format!("  {payload}  "), // whitespace trimmed -> bare payload
        payload,
    ] {
        assert_eq!(
            SilverWaveShareCodec::decode_url_fragment(&candidate).unwrap(),
            "bd sd"
        );
    }

    // Empty fragment (both bare and after prefix strip).
    for bad in ["", "   ", "#code=", "code=", "#sw="] {
        let err = SilverWaveShareCodec::decode_url_fragment(bad).unwrap_err();
        assert!(matches!(
            err,
            ReplError::ShareDecodeError { ref message } if message == "empty share fragment"
        ));
    }

    // `len % 4 == 1` -> invalid length early return.
    let err = SilverWaveShareCodec::decode_url_fragment("#code=AAAAA").unwrap_err();
    assert!(matches!(
        err,
        ReplError::ShareDecodeError { ref message } if message == "invalid base64url length"
    ));

    // Invalid character in each slot of a 4-char quad.
    for bad in ["!AAA", "A!AA", "AA!A", "AAA!"] {
        let err = SilverWaveShareCodec::decode_url_fragment(&format!("#code={bad}")).unwrap_err();
        assert!(matches!(
            err,
            ReplError::ShareDecodeError { ref message } if message == "invalid base64url character"
        ));
    }
    // Invalid character in each slot of a 2-char remainder.
    for bad in ["!A", "A!"] {
        assert!(matches!(
            SilverWaveShareCodec::decode_url_fragment(&format!("#code={bad}")),
            Err(ReplError::ShareDecodeError { .. })
        ));
    }
    // Invalid character in each slot of a 3-char remainder.
    for bad in ["!AA", "A!A", "AA!"] {
        assert!(matches!(
            SilverWaveShareCodec::decode_url_fragment(&format!("#code={bad}")),
            Err(ReplError::ShareDecodeError { .. })
        ));
    }

    // `-` (62) and `_` (63) alphabet chars decode fine.
    assert_eq!(
        SilverWaveShareCodec::decode_url_fragment("#code=A-").unwrap(),
        "\u{3}" // (0 << 2) | (62 >> 4)
    );
    assert_eq!(
        SilverWaveShareCodec::decode_url_fragment("#code=A_").unwrap(),
        "\u{3}" // (0 << 2) | (63 >> 4)
    );
    // `_w` -> single byte 0xFF -> valid base64, invalid UTF-8.
    let err = SilverWaveShareCodec::decode_url_fragment("#code=_w").unwrap_err();
    assert!(matches!(
        err,
        ReplError::ShareDecodeError { ref message } if message == "decoded share payload is not valid UTF-8"
    ));

    // In-loop 1 MiB guard: 349_526 quads -> 1_048_578 decoded bytes > limit.
    let huge = "A".repeat(1_398_104);
    let err = SilverWaveShareCodec::decode_url_fragment(&format!("#code={huge}")).unwrap_err();
    assert!(matches!(
        err,
        ReplError::ShareDecodeError { ref message } if message == "share payload exceeds 1 MiB limit"
    ));

    // Post-loop 1 MiB guard: 349_525 quads (1_048_575 bytes) + 3-char remainder (+2 bytes).
    let mut edge = "A".repeat(1_398_100);
    edge.push_str("AAA");
    let err = SilverWaveShareCodec::decode_url_fragment(&format!("#code={edge}")).unwrap_err();
    assert!(matches!(
        err,
        ReplError::ShareDecodeError { ref message } if message == "share payload exceeds 1 MiB limit"
    ));

    // Boundary-legal remainder decodes: 2-char and 3-char valid tails.
    assert_eq!(
        SilverWaveShareCodec::decode_url_fragment("#code=QQ").unwrap(),
        "A"
    );
    assert_eq!(
        SilverWaveShareCodec::decode_url_fragment("#code=QUI").unwrap(),
        "AB"
    );
}

/// `repl.rs` — every `ReplCommand` dispatch arm including the `ExportAudio`
/// error mapping, plus `set_cps` validation paths.
#[test]
fn test_repl_dispatch_all_commands() {
    let mut repl = ReplEngine::new();

    let r = repl
        .dispatch_command(ReplCommand::EvaluateBuffer {
            code: r#"s("bd sd")"#.to_string(),
        })
        .unwrap();
    assert_eq!(r, None);
    assert!(repl.is_playing());
    // Quoted token -> `ActiveHighlight` pushed (line 419).
    assert_eq!(repl.state.active_highlights.len(), 1);
    assert_eq!(repl.state.active_highlights[0].span, ByteSpan::new(3, 8));

    let r = repl
        .dispatch_command(ReplCommand::EvaluateBlock {
            code: r#"s("hh*4")"#.to_string(),
            block_span: ByteSpan::new(0, 9),
        })
        .unwrap();
    assert_eq!(r, None);

    // `SetCps` arm + `set_cps` guard: non-positive/non-finite values are ignored.
    repl.dispatch_command(ReplCommand::SetCps(0.75)).unwrap();
    assert_eq!(repl.cps(), 0.75);
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        repl.dispatch_command(ReplCommand::SetCps(bad)).unwrap();
        assert_eq!(repl.cps(), 0.75);
    }

    // `DragSlider` arm with a real slider widget.
    repl.dispatch_command(ReplCommand::EvaluateBuffer {
        code: r#"s("bd*4").gain(slider(0.5, 0, 1, 0.1))"#.to_string(),
    })
    .unwrap();
    let wid = repl.widget_registry.all_widgets()[0].id;
    let r = repl
        .dispatch_command(ReplCommand::DragSlider {
            widget_id: wid,
            new_value: 0.9,
        })
        .unwrap();
    assert_eq!(r, None);
    assert_eq!(repl.widget_registry.get_ref_value(wid), Some(0.9));

    // `ExportAudio` success returns bytes; `cycles <= 0` maps to `ExportError`.
    let bytes = repl
        .dispatch_command(ReplCommand::ExportAudio {
            format: AudioExportFormat::WavPcm16,
            cycles: 0.5,
        })
        .unwrap()
        .unwrap();
    assert!(bytes.starts_with(b"RIFF"));
    let err = repl
        .dispatch_command(ReplCommand::ExportAudio {
            format: AudioExportFormat::Flac16,
            cycles: 0.0,
        })
        .unwrap_err();
    assert!(matches!(err, ReplError::ExportError { .. }));

    // `Halt` arm.
    let r = repl.dispatch_command(ReplCommand::Halt).unwrap();
    assert_eq!(r, None);
    assert!(!repl.is_playing());
    assert_eq!(repl.state.all_notes_off_count, 1);
}

/// `repl.rs` — `check_balanced_syntax` error paths: empty buffer, unmatched close,
/// mismatched pairs for `[`/`{`, escapes inside strings, unclosed string, unclosed delimiter.
#[test]
fn test_repl_syntax_error_paths() {
    let mut repl = ReplEngine::new();

    // Empty / whitespace-only buffer -> early `ParseError`.
    for empty in ["", "   \n  "] {
        let err = repl.evaluate_buffer(empty).unwrap_err();
        assert!(matches!(
            err,
            ReplError::ParseError { ref message, .. } if message == "cannot evaluate empty buffer"
        ));
    }

    // Unmatched closing delimiter (empty stack pop).
    let err = repl.evaluate_buffer("a)b").unwrap_err();
    assert!(matches!(
        err,
        ReplError::ParseError { ref message, ref span }
            if message.contains("unmatched closing delimiter") && *span == Some(ByteSpan::new(1, 2))
    ));

    // Mismatched pairs -> `expected` arms for `[` and `{`.
    let err = repl.evaluate_buffer("x[)y").unwrap_err();
    assert!(matches!(
        err,
        ReplError::ParseError { ref message, .. } if message.contains("mismatched delimiter")
    ));
    let err = repl.evaluate_buffer("x{)y").unwrap_err();
    assert!(matches!(
        err,
        ReplError::ParseError { ref message, .. } if message.contains("mismatched delimiter")
    ));

    // Escaped backslash / escaped quote inside a string -> `prev_escape` paths.
    repl.evaluate_buffer(r#"s("a\\b")"#).unwrap();
    repl.evaluate_buffer(r#"s("a\"b")"#).unwrap();

    // Unclosed string literal.
    let err = repl.evaluate_buffer(r#"s("bd"#).unwrap_err();
    assert!(matches!(
        err,
        ReplError::ParseError { ref message, .. } if message == "unclosed string literal"
    ));

    // Unclosed delimiter.
    let err = repl.evaluate_buffer("s(1").unwrap_err();
    assert!(matches!(
        err,
        ReplError::ParseError { ref message, .. } if message.contains("unclosed delimiter")
    ));
}

/// `span_map.rs` — `ByteSpan::is_empty`/`len`, `utf16_to_byte` mid-surrogate and
/// beyond-end `None`s, `byte_span_to_lsp_range` invalid-span `None`, and every
/// `remap_span` edit-overlap arm.
#[test]
fn test_span_map_edge_cases() {
    let empty = ByteSpan::new(4, 4);
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    let inverted = ByteSpan::new(9, 5);
    assert!(inverted.is_empty());
    assert_eq!(ByteSpan::new(2, 7).len(), 5);

    // `utf16_to_byte` beyond the document end -> `None`; mid-surrogate -> `None`.
    let map = SpanMap::new("a\u{1F300}z"); // 'a' + emoji (4 bytes, 2 UTF-16) + 'z'
    assert_eq!(map.utf16_to_byte(0), Some(0));
    assert_eq!(map.utf16_to_byte(1), Some(1)); // wait: byte after 'a'
    assert_eq!(map.utf16_to_byte(2), None); // mid-surrogate pair
    assert_eq!(map.utf16_to_byte(3), Some(5)); // 'z'
    assert_eq!(map.utf16_to_byte(4), Some(6)); // end of document
    assert_eq!(map.utf16_to_byte(5), None); // beyond end
    assert_eq!(map.utf16_to_byte(usize::MAX), None);

    // `byte_span_to_lsp_range` rejection paths.
    assert_eq!(map.byte_span_to_lsp_range(ByteSpan::new(5, 3)), None); // start > end
    assert_eq!(map.byte_span_to_lsp_range(ByteSpan::new(0, 99)), None); // end > len
    assert_eq!(map.byte_span_to_lsp_range(ByteSpan::new(2, 5)), None); // non-boundary start
    assert_eq!(map.byte_span_to_lsp_range(ByteSpan::new(1, 4)), None); // non-boundary end
    let r = map.byte_span_to_lsp_range(ByteSpan::new(0, 6)).unwrap();
    assert_eq!(r.start.character, 0);
    assert_eq!(r.end.character, 4); // 'a' + emoji(2 utf16) + 'z'

    // `remap_span`: every edit-overlap arm.
    // Exact with `delta == 0` (edit before, same length) and `delta != 0` -> Shifted.
    let mut m = SpanMap::new("0123456789abcdef");
    m.apply_edit(TextEdit {
        span: ByteSpan::new(0, 2),
        new_len_bytes: 2,
        new_len_utf16: 2,
    });
    assert_eq!(
        m.remap_span(ByteSpan::new(5, 9)),
        (Some(ByteSpan::new(5, 9)), SpanValidity::Exact)
    );
    let mut m2 = SpanMap::new("0123456789abcdef");
    m2.apply_edit(TextEdit {
        span: ByteSpan::new(0, 0),
        new_len_bytes: 3,
        new_len_utf16: 3,
    });
    assert_eq!(
        m2.remap_span(ByteSpan::new(5, 9)),
        (Some(ByteSpan::new(8, 12)), SpanValidity::Shifted)
    );

    // Edit strictly after -> unchanged.
    let mut m3 = SpanMap::new("0123456789abcdef");
    m3.apply_edit(TextEdit {
        span: ByteSpan::new(12, 14),
        new_len_bytes: 0,
        new_len_utf16: 0,
    });
    assert_eq!(
        m3.remap_span(ByteSpan::new(5, 9)),
        (Some(ByteSpan::new(5, 9)), SpanValidity::Exact)
    );

    // Edit covering the span -> `Invalidated`.
    let mut m4 = SpanMap::new("0123456789abcdef");
    m4.apply_edit(TextEdit {
        span: ByteSpan::new(4, 11),
        new_len_bytes: 1,
        new_len_utf16: 1,
    });
    assert_eq!(
        m4.remap_span(ByteSpan::new(5, 9)),
        (None, SpanValidity::Invalidated)
    );

    // Edit overlapping start -> clip start to end of replacement (`Clipped`).
    let mut m5 = SpanMap::new("0123456789abcdef");
    m5.apply_edit(TextEdit {
        span: ByteSpan::new(4, 7),
        new_len_bytes: 1,
        new_len_utf16: 1,
    });
    assert_eq!(
        m5.remap_span(ByteSpan::new(5, 9)),
        (Some(ByteSpan::new(5, 7)), SpanValidity::Clipped)
    );

    // Edit overlapping end -> clip end to start of edit (`Clipped`).
    let mut m6 = SpanMap::new("0123456789abcdef");
    m6.apply_edit(TextEdit {
        span: ByteSpan::new(7, 12),
        new_len_bytes: 4,
        new_len_utf16: 4,
    });
    assert_eq!(
        m6.remap_span(ByteSpan::new(5, 9)),
        (Some(ByteSpan::new(5, 7)), SpanValidity::Clipped)
    );

    // Edit strictly inside -> expand/contract `cur.end` (`Clipped`).
    let mut m7 = SpanMap::new("0123456789abcdef");
    m7.apply_edit(TextEdit {
        span: ByteSpan::new(6, 8),
        new_len_bytes: 5,
        new_len_utf16: 5,
    });
    assert_eq!(
        m7.remap_span(ByteSpan::new(5, 9)),
        (Some(ByteSpan::new(5, 12)), SpanValidity::Clipped)
    );

    // Multiple edits compose; `Shifted` status is preserved once set.
    let mut m8 = SpanMap::new("0123456789abcdef");
    m8.apply_edit(TextEdit {
        span: ByteSpan::new(0, 0),
        new_len_bytes: 2,
        new_len_utf16: 2,
    });
    m8.apply_edit(TextEdit {
        span: ByteSpan::new(2, 3),
        new_len_bytes: 5,
        new_len_utf16: 5,
    });
    let (remapped, validity) = m8.remap_span(ByteSpan::new(5, 9));
    assert_eq!(validity, SpanValidity::Shifted);
    assert_eq!(remapped, Some(ByteSpan::new(11, 15))); // +2 then +4
}

/// `tracker.rs` — plock update of an existing param, invalid step index,
/// `InteractiveTour::default()`, full 5-stage progression, post-tour `true`,
/// and a rejected (non-matching) code.
#[test]
fn test_tracker_and_tour_edges() {
    let mut tracker = TrackerPhraseView::from_mini_pattern("c4 e4 -", "saw");
    assert_eq!(tracker.rows.len(), 3);
    assert_eq!(tracker.rows[2].note, "---"); // '-' treated as rest like '~'

    // New plock then update of the *same* param -> `existing.1 = value` arm.
    tracker.set_step_plock(0, "lpf", 800.0).unwrap();
    tracker.set_step_plock(0, "lpf", 1200.0).unwrap();
    assert_eq!(tracker.rows[0].plocks, vec![("lpf".to_string(), 1200.0)]);
    // Out-of-range step index -> `None`.
    assert!(tracker.set_step_plock(99, "lpf", 1.0).is_none());
    assert_eq!(tracker.to_mini_pattern(), "c4 e4 ~");

    // `InteractiveTour::default()` delegates to `new()`.
    let tour_default = InteractiveTour::default();
    assert_eq!(tour_default.stages.len(), 5);
    assert_eq!(tour_default.current_stage, 0);

    // Wrong code -> `false`, stage does not advance.
    let mut tour = InteractiveTour::new();
    assert!(!tour.verify_and_advance(r#"s("sd*2")"#));
    assert_eq!(tour.current_stage, 0);

    // Play through all 5 canonical stages; calls past the end return `true`.
    let codes = [
        r#"s("bd*4")"#,
        r#"s("bd sd [~ bd] sd, hh*8")"#,
        r#"note("c2(3,8)").s("sawtooth").lpf(800)"#,
        r#"s("bd*4").lpf(slider(800, 100, 4000, 10))"#,
        r#"s("bd sd hh cp")._punchcard()"#,
    ];
    for code in codes {
        assert!(tour.verify_and_advance(code));
    }
    assert_eq!(tour.current_stage, 5);
    assert!(tour.stages.iter().all(|s| s.completed));
    assert!(tour.verify_and_advance("anything at all")); // past-end guard -> `true`
    assert_eq!(tour.current_stage, 5);
}

/// `widgets.rs` — `extract_from_source` unclosed/partial/non-numeric calls,
/// `commit_slider_drag` on a source shorter than the stored span, `-0.0`
/// formatting, `ModulationRingOverlay`, CSS foreground/decoration parsing,
/// and `sanitize_mark_css` continue-arms.
#[test]
fn test_widget_registry_and_css_edges() {
    let mut reg = WidgetRegistry::new();

    // Multiple sliders on one line (id increments), a non-numeric first arg (skipped),
    // and an unclosed trailing `slider(` (`break`).
    let src = r#"slider(0.1,0,1,0.05) slider(abc,0,1) slider(0.9) s("bd") slider(0.5"#;
    let extracted = reg.extract_from_source(src);
    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted[0].id, 1);
    assert_eq!(extracted[1].id, 2);
    // Missing args fall back to defaults: min 0, max 1, step 0.01.
    assert_eq!(extracted[1].min, 0.0);
    assert_eq!(extracted[1].max, 1.0);
    assert_eq!(extracted[1].step, 0.01);

    // Re-extraction clears prior state.
    let again = reg.extract_from_source("slider(1,0,2,0.5)");
    assert_eq!(again.len(), 1);
    assert_eq!(reg.all_widgets().len(), 1);

    // `commit_slider_drag` where the stored span exceeds a (diverged) source ->
    // `replace_range` skipped but patch still emitted.
    let mut short_src = String::from("x");
    let mut sm = SpanMap::new(&short_src.clone());
    let patch = reg
        .commit_slider_drag(1, 1.5, &mut short_src, &mut sm)
        .unwrap();
    assert_eq!(patch.replacement_text, "1.5"); // within `max == 2`, on `step == 0.5`
    assert_eq!(short_src, "x"); // `old_span.end > source.len()` -> no splice

    // `-0.0` on a `step <= 0` slider formats as `"0"` via the `s == "-0"` arm.
    let mut reg2 = WidgetRegistry::new();
    let e = reg2.extract_from_source("slider(0,-1,1,0)");
    let mut src2 = "slider(0,-1,1,0)".to_string();
    let mut sm2 = SpanMap::new(&src2.clone());
    let patch2 = reg2
        .commit_slider_drag(e[0].id, -0.0, &mut src2, &mut sm2)
        .unwrap();
    assert_eq!(patch2.replacement_text, "0");

    // Unknown widget id -> `None`.
    assert!(
        reg2.commit_slider_drag(999, 0.5, &mut src2, &mut sm2)
            .is_none()
    );
    assert_eq!(reg2.get_widget(999), None);

    // `ModulationRingOverlay::from_values` with a zero-width range (span floor).
    let mut reg3 = WidgetRegistry::new();
    let e3 = reg3.extract_from_source("slider(5,5,5,1)");
    let ring = ModulationRingOverlay::from_values(&e3[0], 0.0, RgbaColor::new(1, 2, 3, 255));
    assert!(ring.start_angle_rad.is_finite());
    assert_eq!(ring.base_normalized, 0.0);

    // `HighlightStyle` — `color:` declaration, unknown allowed key (`_ => {}`),
    // `text-decoration` without "underline", and `None` / fully-stripped CSS.
    let st = HighlightStyle::from_color_and_css(
        RgbaColor::new(255, 0, 0, 255),
        Some("color: cyan; opacity: 0.5; text-decoration: none; border: 1px"),
    );
    assert_eq!(st.native.foreground, Some(RgbaColor::new(0, 255, 255, 255)));
    assert!(!st.native.underline); // "none" doesn't contain "underline"
    assert!(st.mark_css_raw.is_some());

    let st_none = HighlightStyle::from_color_and_css(RgbaColor::new(0, 0, 0, 255), None);
    assert_eq!(st_none.mark_css_raw, None);
    assert_eq!(st_none.native.background, None);

    // CSS that fully sanitizes away -> `mark_css_raw == None` via `filter`.
    let st_strip = HighlightStyle::from_color_and_css(
        RgbaColor::new(0, 0, 0, 255),
        Some("width: 10px; background: url(http://evil)"),
    );
    assert_eq!(st_strip.mark_css_raw, None);

    // `sanitize_mark_css` — declaration without `:` (`continue`), disallowed prop
    // (`continue`), and dangerous value schemes (`url(`, `expression(`, `javascript:`).
    assert_eq!(sanitize_mark_css("garbage-no-colon"), "");
    assert_eq!(sanitize_mark_css("position: fixed"), "");
    assert_eq!(sanitize_mark_css("background: url(x)"), "");
    assert_eq!(sanitize_mark_css("color: expression(alert)"), "");
    assert_eq!(sanitize_mark_css("color: javascript:evil"), "");
    assert_eq!(
        sanitize_mark_css("outline: 2px solid red; junk; color:blue"),
        "outline:2px solid red;color:blue"
    );
}

/// `repl.rs` — the remaining zero-segment edges: `drag_slider`'s `?`
/// early-return on an unregistered widget id (line 245), the `?` error
/// edges of the `EvaluateBuffer`/`EvaluateBlock` dispatch arms (lines 254
/// and 258), and the no-quoted-token else edge of
/// `extract_quoted_token_highlights` (line 416 region).
#[test]
fn test_repl_dispatch_error_edges_and_highlight_false_edge() {
    let mut repl = ReplEngine::new();

    // `commit_slider_drag` returns `None` for an unknown widget id -> the
    // `?` in `drag_slider` takes its early-return edge (repl.rs:245).
    assert_eq!(repl.drag_slider(999, 0.5), None);

    // Invalid code through `EvaluateBuffer` -> `evaluate_buffer` error
    // propagates through the dispatch `?` (repl.rs:254).
    let err = repl
        .dispatch_command(ReplCommand::EvaluateBuffer {
            code: "s((bd".to_string(),
        })
        .unwrap_err();
    assert!(matches!(err, ReplError::ParseError { .. }));

    // Invalid code through `EvaluateBlock` -> same `?` edge on the sibling
    // arm (repl.rs:258).
    let err = repl
        .dispatch_command(ReplCommand::EvaluateBlock {
            code: "s((sd".to_string(),
            block_span: ByteSpan::new(0, 5),
        })
        .unwrap_err();
    assert!(matches!(err, ReplError::ParseError { .. }));

    // Source without any `"` -> `extract_quoted_token_highlights` takes the
    // false edge of its `if let` and pushes no `ActiveHighlight`
    // (repl.rs:416 region).
    repl.evaluate_buffer("s bd sd").unwrap();
    assert!(repl.state.active_highlights.is_empty());
}

/// `span_map.rs` — `byte_span_to_utf16_span`'s `?` early-return when
/// `span.end` is not convertible (line 117): non-boundary and
/// beyond-the-end byte offsets.
#[test]
fn test_byte_span_to_utf16_span_end_failure() {
    let map = SpanMap::new("héllo"); // 'é' occupies bytes 1..3
    // Valid `start`, mid-codepoint `end` -> `byte_to_utf16(end)` -> `None`.
    assert_eq!(map.byte_span_to_utf16_span(ByteSpan::new(0, 2)), None);
    // Valid `start`, `end` beyond the buffer -> same `?` edge.
    assert_eq!(map.byte_span_to_utf16_span(ByteSpan::new(0, 99)), None);
    // Fully valid span still converts both ends.
    assert_eq!(
        map.byte_span_to_utf16_span(ByteSpan::new(0, 3)),
        Some(Utf16Span::new(0, 2))
    );
}
