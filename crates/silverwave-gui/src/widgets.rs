//! Inline Code Widgets (`slider(val, min, max, step)`), Zero-Re-Eval `RefValue` Binding,
//! In-Place `SliderPatch` Mutation (`05-SPEC` §4.3, `SLIDER-01`..`SLIDER-05`),
//! CSS Sanitizer & `.markcss()` Styling (`05-SPEC` §4.4), and Bitwig-Style `ModulationRing` (`CK-02`).

#![forbid(unsafe_code)]

use crate::span_map::{ByteSpan, SpanMap, SpanMapper, TextEdit, Utf16Span};
use silverwave_viz::RgbaColor;
use std::collections::BTreeMap;

/// Unique identifier for an inline widget (`05-SPEC` §2.1).
pub type WidgetId = u32;

/// Descriptor for an extracted `slider(val, min, max, step)` call (`05-SPEC` §4.3, `SLIDER-01`).
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetDescriptor {
    pub id: WidgetId,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub literal_span: ByteSpan,
    pub call_span: ByteSpan,
}

/// Surgical in-place source replacement patch emitted on slider drag (`05-SPEC` §4.3, `SLIDER-04`, `SLIDER-05`).
#[derive(Debug, Clone, PartialEq)]
pub struct SliderPatch {
    pub widget_id: WidgetId,
    pub replace_span: ByteSpan,
    pub replace_utf16_span: Utf16Span,
    pub replacement_text: String,
    pub clamped_value: f64,
}

/// Bitwig-style animated 60 FPS `ModulationRing` overlay around an inline `slider()` (`05-SPEC` §6.2, `CK-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModulationRingOverlay {
    pub widget_id: WidgetId,
    pub base_normalized: f32,
    pub modulated_normalized: f32,
    pub start_angle_rad: f32,
    pub end_angle_rad: f32,
    pub ring_color: RgbaColor,
}

impl ModulationRingOverlay {
    /// Computes the modulation arc angles in radians for `base_val` and `mod_delta` within `[min, max]`.
    pub fn from_values(widget: &WidgetDescriptor, mod_delta: f64, ring_color: RgbaColor) -> Self {
        let span = (widget.max - widget.min).max(1.0e-9);
        let base_n = ((widget.value - widget.min) / span).clamp(0.0, 1.0) as f32;
        let mod_n = (((widget.value + mod_delta) - widget.min) / span).clamp(0.0, 1.0) as f32;
        let min_ang = -core::f32::consts::PI * 0.75;
        let sweep = core::f32::consts::PI * 1.5;
        Self {
            widget_id: widget.id,
            base_normalized: base_n,
            modulated_normalized: mod_n,
            start_angle_rad: min_ang + base_n * sweep,
            end_angle_rad: min_ang + mod_n * sweep,
            ring_color,
        }
    }
}

/// Registry managing inline `slider()` widgets, live `RefValue` slots, and surgical `SliderPatch` mutations (`SLIDER-01`..`SLIDER-05`).
#[derive(Debug, Clone, Default)]
pub struct WidgetRegistry {
    widgets: BTreeMap<WidgetId, WidgetDescriptor>,
    ref_values: BTreeMap<WidgetId, f64>,
}

impl WidgetRegistry {
    /// Creates an empty `WidgetRegistry`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Scans `source` for all `slider(val, min, max, step)` calls and populates `self` (`SLIDER-01`).
    pub fn extract_from_source(&mut self, source: &str) -> Vec<WidgetDescriptor> {
        self.widgets.clear();
        self.ref_values.clear();
        let mut out = Vec::new();
        let needle = "slider(";
        let mut search_from = 0usize;
        let mut next_id: WidgetId = 1;

        while let Some(rel_idx) = source[search_from..].find(needle) {
            let call_start = search_from + rel_idx;
            let args_start = call_start + needle.len();
            if let Some(rel_close) = source[args_start..].find(')') {
                let call_end = args_start + rel_close + 1;
                let inside = &source[args_start..(args_start + rel_close)];
                let parts: Vec<&str> = inside.split(',').collect();
                if !parts.is_empty() {
                    let first_raw = parts[0];
                    let lead_ws = first_raw.len() - first_raw.trim_start().len();
                    let trimmed = first_raw.trim();
                    let lit_start = args_start + lead_ws;
                    let lit_end = lit_start + trimmed.len();

                    if let Ok(val) = trimmed.parse::<f64>() {
                        let min = parts
                            .get(1)
                            .and_then(|s| s.trim().parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let max = parts
                            .get(2)
                            .and_then(|s| s.trim().parse::<f64>().ok())
                            .unwrap_or(1.0);
                        let step = parts
                            .get(3)
                            .and_then(|s| s.trim().parse::<f64>().ok())
                            .unwrap_or(0.01);
                        let desc = WidgetDescriptor {
                            id: next_id,
                            value: val,
                            min,
                            max,
                            step,
                            literal_span: ByteSpan::new(lit_start, lit_end),
                            call_span: ByteSpan::new(call_start, call_end),
                        };
                        self.widgets.insert(next_id, desc.clone());
                        self.ref_values.insert(next_id, val);
                        out.push(desc);
                        next_id += 1;
                    }
                }
                search_from = call_end;
            } else {
                break;
            }
        }
        out
    }

    /// Reads the live `RefValue` for `widget_id` (`SLIDER-03`).
    pub fn get_ref_value(&self, widget_id: WidgetId) -> Option<f64> {
        self.ref_values.get(&widget_id).copied()
    }

    /// Returns a reference to a registered `WidgetDescriptor`.
    pub fn get_widget(&self, widget_id: WidgetId) -> Option<&WidgetDescriptor> {
        self.widgets.get(&widget_id)
    }

    /// Returns all registered `WidgetDescriptor`s in ID order.
    pub fn all_widgets(&self) -> Vec<WidgetDescriptor> {
        self.widgets.values().cloned().collect()
    }

    /// Commits a slider drag without re-evaluating the pattern, updating `source`, `span_map`, and
    /// shifting all downstream slider spans on the same or subsequent lines (`SLIDER-02`..`SLIDER-05`).
    pub fn commit_slider_drag(
        &mut self,
        widget_id: WidgetId,
        raw_new_value: f64,
        source: &mut String,
        span_map: &mut SpanMap,
    ) -> Option<SliderPatch> {
        let desc = self.widgets.get(&widget_id)?.clone();
        let mut clamped = raw_new_value.clamp(desc.min, desc.max);
        if desc.step > 0.0 {
            let steps = ((clamped - desc.min) / desc.step).round();
            clamped = (desc.min + steps * desc.step).clamp(desc.min, desc.max);
            // Round to avoid floating-point tail noise like `0.30000000000000004`
            clamped = (clamped * 1_000_000.0).round() / 1_000_000.0;
        }
        let replacement_text = format_clean_float(clamped);
        let old_span = desc.literal_span;
        let utf16_span = span_map
            .byte_span_to_utf16_span(old_span)
            .unwrap_or(Utf16Span::new(old_span.start, old_span.end));

        let old_len = old_span.len();
        let new_len = replacement_text.len();
        let delta = (new_len as isize) - (old_len as isize);

        if old_span.end <= source.len() {
            source.replace_range(old_span.start..old_span.end, &replacement_text);
        }

        span_map.apply_edit(TextEdit {
            span: old_span,
            new_len_bytes: new_len,
            new_len_utf16: replacement_text.encode_utf16().count(),
        });

        // Update this widget and shift all downstream widgets (`SLIDER-05`)
        for (id, w) in self.widgets.iter_mut() {
            if *id == widget_id {
                w.value = clamped;
                w.literal_span = ByteSpan::new(old_span.start, old_span.start + new_len);
                w.call_span.end = ((w.call_span.end as isize) + delta).max(0) as usize;
            } else if w.literal_span.start >= old_span.end {
                w.literal_span.start = ((w.literal_span.start as isize) + delta).max(0) as usize;
                w.literal_span.end = ((w.literal_span.end as isize) + delta).max(0) as usize;
                w.call_span.start = ((w.call_span.start as isize) + delta).max(0) as usize;
                w.call_span.end = ((w.call_span.end as isize) + delta).max(0) as usize;
            }
        }
        self.ref_values.insert(widget_id, clamped);

        Some(SliderPatch {
            widget_id,
            replace_span: old_span,
            replace_utf16_span: utf16_span,
            replacement_text,
            clamped_value: clamped,
        })
    }
}

fn format_clean_float(v: f64) -> String {
    let s = format!("{:.6}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Native desktop (`egui`) and TUI representation of `.markcss()` & `.color()` (`05-SPEC` §4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeHighlightStyle {
    pub outline: Option<(f32, RgbaColor)>,
    pub background: Option<RgbaColor>,
    pub foreground: Option<RgbaColor>,
    pub underline: bool,
}

/// Complete token highlight style (`05-SPEC` §4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightStyle {
    pub color: RgbaColor,
    pub mark_css_raw: Option<String>,
    pub native: NativeHighlightStyle,
}

impl HighlightStyle {
    /// Builds a `HighlightStyle` from a primary `color` and optional `.markcss()` string,
    /// sanitizing unsafe CSS properties (`url(...)`, `expression(...)`, `position`, `display`, `width`, `height` — `05-SPEC` §4.4).
    pub fn from_color_and_css(color: RgbaColor, raw_css: Option<&str>) -> Self {
        let mut native = NativeHighlightStyle {
            outline: Some((2.0, color)),
            background: None,
            foreground: None,
            underline: false,
        };
        let sanitized = raw_css.map(sanitize_mark_css).filter(|s| !s.is_empty());

        if let Some(ref css) = sanitized {
            for decl in css.split(';') {
                if let Some((k, v)) = decl.split_once(':') {
                    let key = k.trim().to_ascii_lowercase();
                    let val = v.trim().to_ascii_lowercase();
                    match key.as_str() {
                        "background" | "background-color" => {
                            native.background = RgbaColor::parse_css(&val);
                        }
                        "color" => {
                            native.foreground = RgbaColor::parse_css(&val);
                        }
                        "text-decoration" if val.contains("underline") => {
                            native.underline = true;
                        }
                        _ => {}
                    }
                }
            }
        }

        Self {
            color,
            mark_css_raw: sanitized,
            native,
        }
    }
}

/// Sanitizes `.markcss()` against the `05-SPEC` §4.4 allowlist (`outline`, `border`, `background`,
/// `color`, `text-decoration`, `box-shadow`, `opacity`, `border-radius`).
pub fn sanitize_mark_css(raw_css: &str) -> String {
    const ALLOWED_PROPS: &[&str] = &[
        "outline",
        "border",
        "background",
        "background-color",
        "color",
        "text-decoration",
        "box-shadow",
        "opacity",
        "border-radius",
    ];
    let mut kept = Vec::new();
    for decl in raw_css.split(';') {
        let Some((k, v)) = decl.split_once(':') else {
            continue;
        };
        let prop = k.trim().to_ascii_lowercase();
        let val_lower = v.trim().to_ascii_lowercase();
        if !ALLOWED_PROPS.contains(&prop.as_str()) {
            continue;
        }
        if val_lower.contains("url(")
            || val_lower.contains("expression(")
            || val_lower.contains("javascript:")
        {
            continue;
        }
        kept.push(format!("{}:{}", prop, v.trim()));
    }
    kept.join(";")
}

/// Active token highlight emitted at 60 Hz (`05-SPEC` §4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveHighlight {
    pub span: ByteSpan,
    pub utf16_span: Utf16Span,
    pub style: HighlightStyle,
}
