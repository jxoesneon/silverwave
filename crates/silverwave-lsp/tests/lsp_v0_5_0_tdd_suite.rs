//! Comprehensive TDD Test Suite for `silverwave-lsp` (`v0.5.0`, `LSP-01`, `LSP-02`).

#![forbid(unsafe_code)]

use silverwave_lsp::{
    version, EditorRpcNotification, EditorRpcRequest, PositionEncoding, SilverWaveLspServer,
};
use silverwave_viz::AudioExportFormat;

#[test]
fn test_lsp_01_02_eval_decorations_and_halt() {
    assert!(!version().is_empty());
    let mut server = SilverWaveLspServer::new(PositionEncoding::Utf16);
    assert_eq!(server.encoding, PositionEncoding::Utf16);

    // LSP-01: `Eval` -> `EvalFlash` + `DocumentDecorations` + `ActiveHighlights`
    let notifs = server
        .handle_request(EditorRpcRequest::Eval {
            uri: "file:///live.sw".to_string(),
            text: r#"// @title LSP Test
s("bd sd").lpf(slider(400, 100, 2000, 10))._pianoroll()"#
                .to_string(),
            range: None,
            version: 7,
        })
        .unwrap();
    assert_eq!(notifs.len(), 3);
    assert!(matches!(notifs[0], EditorRpcNotification::EvalFlash { .. }));
    assert!(matches!(
        notifs[1],
        EditorRpcNotification::DocumentDecorations { version: 7, .. }
    ));
    match &notifs[2] {
        EditorRpcNotification::ActiveHighlights { spans, .. } => {
            assert!(!spans.is_empty());
        }
        _ => panic!("expected ActiveHighlights"),
    }

    // UpdateWidget request
    let w_notifs = server
        .handle_request(EditorRpcRequest::UpdateWidget {
            uri: "file:///live.sw".to_string(),
            widget_id: 1,
            value: 900.0,
        })
        .unwrap();
    assert_eq!(w_notifs.len(), 1);

    // ExportAudio request
    let exp_notifs = server
        .handle_request(EditorRpcRequest::ExportAudio {
            uri: "file:///live.sw".to_string(),
            format: AudioExportFormat::WavPcm16,
            cycles: 1.0,
            output_path: "/tmp/out.wav".to_string(),
        })
        .unwrap();
    assert!(exp_notifs.is_empty());

    // LSP-02: `Halt` cancels pending `ActiveHighlights` and emits empty `spans: []`
    let halt_notifs = server
        .handle_request(EditorRpcRequest::Halt {
            uri: Some("file:///live.sw".to_string()),
        })
        .unwrap();
    assert_eq!(
        halt_notifs,
        vec![EditorRpcNotification::ActiveHighlights {
            uri: "file:///live.sw".to_string(),
            version: 7,
            spans: Vec::new(),
        }]
    );
}
