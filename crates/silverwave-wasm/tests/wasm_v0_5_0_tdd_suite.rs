//! Comprehensive TDD Test Suite for `silverwave-wasm` (`v0.5.0`, `PWA-01`).

#![forbid(unsafe_code)]

use silverwave_wasm::{
    version, FrameUpdate, PwaCacheStrategy, PwaOfflineStore, SilverwaveWasmHandle,
    WasmHighlightEntry, WasmInlineVizEntry,
};

#[test]
fn test_pwa_01_wasm_handle_offline_cache_and_frame_ticks() {
    assert!(!version().is_empty());
    let mut wasm = SilverwaveWasmHandle::new();
    assert_eq!(
        wasm.offline_store.cached_assets.get("silverwave_bg.wasm"),
        Some(&PwaCacheStrategy::CacheFirstImmutable)
    );

    // Simulate offline mode (`navigator.onLine == false` — `PWA-01`)
    wasm.offline_store.online = false;
    let code = r#"// @title Offline Jam
s("bd sd").gain(slider(0.8, 0, 1, 0.05))._scope()"#;
    let out = wasm.evaluate(code, None).unwrap();
    assert_eq!(out.metadata.title.as_deref(), Some("Offline Jam"));
    assert_eq!(
        wasm.offline_store.restore_last_sketch().as_deref(),
        Some(code)
    );

    let patch = wasm.update_slider(1, 0.5).unwrap();
    assert_eq!(patch.replacement_text, "0.5");

    wasm.set_cps(0.5);
    let frame: FrameUpdate = wasm.tick_frame(1000.0);
    assert!(!frame.highlights.is_empty());
    let _h0: &WasmHighlightEntry = &frame.highlights[0];
    let _v0: &WasmInlineVizEntry = &frame.inline_viz[0];
    assert!(frame.hydra_uniforms.is_some());

    let url = wasm.encode_share_url(code);
    assert_eq!(wasm.decode_share_url(&url).unwrap(), code);

    let wav = wasm.export_audio("wav", 1.0).unwrap();
    assert!(wav.starts_with(b"RIFF"));
    let mut store = PwaOfflineStore::default();
    store.save_sketch("manual", r#"s("bd")"#);
    wasm.halt();
}
