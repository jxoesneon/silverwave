//! Comprehensive `v1.0.0` GA 1:1+ Parity Matrix & Cross-Crate Integration Test Suite
//! (`00-MASTER-ROADMAP-v0.0.0-to-v1.0.0.md` §2 `v1.0.0`, §3, §4.3).

#![forbid(unsafe_code)]

use silverwave_cli::{
    PARITY_MATRIX_SECTIONS, ParityAuditSummary, ParityCategoryRow,
    verify_full_strudel_tidal_parity_matrix,
};
use silverwave_core::Rational64;
use silverwave_mcp::{MCP_ALL_42_TOOL_NAMES, MCP_RESOURCE_URIS, McpCliArgs, SilverWaveMcpServer};

#[test]
fn test_v1_0_0_ga_parity_matrix_and_cross_crate_signoff() {
    assert_eq!(PARITY_MATRIX_SECTIONS.len(), 6);
    let (rows, summary): (Vec<ParityCategoryRow>, ParityAuditSummary) =
        verify_full_strudel_tidal_parity_matrix();
    assert_eq!(rows.len(), 7);
    assert!(rows.iter().all(|r| r.verified));
    assert_eq!(summary.version, "1.0.0");
    assert_eq!(summary.pain_points_resolved, 12);
    assert_eq!(summary.category_king_features, 10);
    assert_eq!(summary.mcp_tools_count, 42);
    assert_eq!(summary.workspace_crates_count, 11);
    assert!(summary.ga_ready);

    // Cross-crate check: Rational64 + 42-tool MCP server + 10 live resources
    let half = Rational64::new_const(1, 2);
    assert_eq!(half + half, Rational64::from_integer(1));
    let srv = SilverWaveMcpServer::new(McpCliArgs::default());
    assert_eq!(MCP_ALL_42_TOOL_NAMES.len(), 42);
    assert_eq!(MCP_RESOURCE_URIS.len(), 10);
    assert_eq!(srv.orbits.len(), 16);
}
