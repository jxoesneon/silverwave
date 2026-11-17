//! Line-coverage gap suite for `silverwave-wasm`.
//! Exercises the previously uncovered regions of `lib.rs`: the
//! `evaluate_block` `block_range` path (line 123), the `metadata.title ==
//! None` `"autosave"` fallback (line 131), and the `"flac"` export-format arm
//! (line 208). The crate compiles for host in tests, so the wasm-facing
//! `SilverwaveWasmHandle` methods are called directly.

#![forbid(unsafe_code)]

use silverwave_wasm::SilverwaveWasmHandle;

/// Covers `lib.rs` line 123: `evaluate` with `Some(block_range)` dispatches to
/// `ReplEngine::evaluate_block` and persists the sketch under its `@title`.
#[test]
fn evaluate_with_block_range_dispatches_to_evaluate_block() {
    let mut handle = SilverwaveWasmHandle::new();
    let code = "// @title Block Jam\ns(\"bd sd\")";
    let outcome = handle
        .evaluate(code, Some((0, code.len())))
        .expect("block eval should succeed");
    assert_eq!(outcome.metadata.title.as_deref(), Some("Block Jam"));
    assert_eq!(outcome.eval_generation, 1);
    assert_eq!(
        handle.offline_store.last_sketch_key.as_deref(),
        Some("Block Jam")
    );
    assert_eq!(
        handle.offline_store.restore_last_sketch().as_deref(),
        Some(code)
    );
}

/// Covers `lib.rs` line 131: buffers without a `// @title` header fall back to
/// the `"autosave"` `IndexedDB` sketch key.
#[test]
fn evaluate_without_title_uses_autosave_fallback_key() {
    let mut handle = SilverwaveWasmHandle::new();
    let code = "s(\"bd*4\")";
    let outcome = handle
        .evaluate(code, None)
        .expect("untitled eval should succeed");
    assert!(outcome.metadata.title.is_none());
    assert_eq!(
        handle.offline_store.last_sketch_key.as_deref(),
        Some("autosave")
    );
    assert_eq!(
        handle.offline_store.restore_last_sketch().as_deref(),
        Some(code)
    );
}

/// Covers `lib.rs` line 208: `export_audio("flac")` selects
/// `AudioExportFormat::Flac24` and emits the deterministic `fLaC` container;
/// the catch-all arm still yields RIFF/WAVE for any other format string.
#[test]
fn export_audio_flac_arm_and_default_wav_fallback() {
    let mut handle = SilverwaveWasmHandle::new();

    let flac = handle.export_audio("flac", 1.0).expect("flac export");
    assert!(flac.starts_with(b"fLaC"), "expected fLaC container");

    // Case-insensitive match also lands on the flac arm.
    let flac_upper = handle.export_audio("FLAC", 1.0).expect("flac export");
    assert!(flac_upper.starts_with(b"fLaC"));

    let wav = handle.export_audio("wav", 1.0).expect("wav export");
    assert!(wav.starts_with(b"RIFF"), "expected RIFF/WAVE container");
}

/// Covers `lib.rs` line 98 — `restore_last_sketch` on a store with no saved
/// sketch takes the `?` early-return edge and yields `None`.
#[test]
fn restore_last_sketch_without_save_returns_none() {
    let handle = SilverwaveWasmHandle::new();
    assert_eq!(handle.offline_store.restore_last_sketch(), None);
}

/// Covers `lib.rs` lines 123 and 125 — the `?` error edges of both
/// `evaluate` dispatch arms on syntactically invalid code.
#[test]
fn evaluate_error_edges_propagate_repl_error() {
    let mut handle = SilverwaveWasmHandle::new();
    // `Some(block_range)` arm -> `evaluate_block` error propagates (line 123).
    assert!(handle.evaluate("s((bd", Some((0, 5))).is_err());
    // `None` arm -> `evaluate_buffer` error propagates (line 125).
    assert!(handle.evaluate("s((bd", None).is_err());
}

/// Covers `lib.rs` line 216 — `dispatch_command(ExportAudio)` fails for
/// `cycles <= 0` and the `?` propagates `ReplError::ExportError`.
#[test]
fn export_audio_error_edge_propagates() {
    let mut handle = SilverwaveWasmHandle::new();
    assert!(handle.export_audio("wav", 0.0).is_err());
    assert!(handle.export_audio("flac", -1.0).is_err());
}
