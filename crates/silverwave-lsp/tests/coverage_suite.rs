//! Line-coverage gap suite for `silverwave-lsp` — exercises the `?` error
//! edges in `SilverWaveLspServer::handle_request`: `Eval` on syntactically
//! invalid text (lib.rs:111) and `ExportAudio` with a non-positive cycle
//! count (lib.rs:173).

#![forbid(unsafe_code)]

use silverwave_gui::ReplError;
use silverwave_lsp::{EditorRpcRequest, PositionEncoding, SilverWaveLspServer};
use silverwave_viz::AudioExportFormat;

/// `Eval` with unbalanced delimiters -> `evaluate_buffer` fails and the `?`
/// propagates `ReplError::ParseError` out of `handle_request` (lib.rs:111).
#[test]
fn eval_with_invalid_text_propagates_parse_error() {
    let mut server = SilverWaveLspServer::new(PositionEncoding::Utf16);
    let err = server
        .handle_request(EditorRpcRequest::Eval {
            uri: "file:///bad.sw".to_string(),
            text: "s((bd".to_string(),
            range: None,
            version: 2,
        })
        .unwrap_err();
    assert!(matches!(err, ReplError::ParseError { .. }));
}

/// `ExportAudio` with `cycles <= 0` -> the deterministic exporter rejects the
/// config and the dispatch `?` propagates `ReplError::ExportError`
/// (lib.rs:173).
#[test]
fn export_audio_with_invalid_cycles_propagates_export_error() {
    let mut server = SilverWaveLspServer::new(PositionEncoding::Utf16);
    let err = server
        .handle_request(EditorRpcRequest::ExportAudio {
            uri: "file:///live.sw".to_string(),
            format: AudioExportFormat::WavPcm16,
            cycles: 0.0,
            output_path: "/tmp/out.wav".to_string(),
        })
        .unwrap_err();
    assert!(matches!(err, ReplError::ExportError { .. }));
}
