//! Comprehensive TDD Suite for `v0.9.0` Release Candidate Hardening (`00-MASTER-ROADMAP` §2 `v0.9.0`, `06-SPEC` §4).
//! Verifies 24-hour soak/chaos testing, 100M+ fuzz corpus audit, SLSA Level 3 provenance,
//! CycloneDX 1.5 SBOM, MSRV 1.85.0, and topological `crates.io` publish ordering.

#![forbid(unsafe_code)]

use silverwave_cli::{
    CRATES_IO_TOPOLOGICAL_ORDER, CycloneDxSbom, FuzzCorpusReport, SILVERWAVE_MSRV,
    SlsaLevel3Provenance, SoakTestReport, TopologicalPublishStep,
    generate_supply_chain_attestations, run_24h_soak_chaos_suite, run_100m_fuzz_corpus_audit,
    verify_topological_crates_io_publish_order,
};

#[test]
fn test_v0_9_0_soak_fuzz_supply_chain_and_topological_publish() {
    assert_eq!(SILVERWAVE_MSRV, "1.85.0");
    assert_eq!(CRATES_IO_TOPOLOGICAL_ORDER.len(), 11);
    assert_eq!(CRATES_IO_TOPOLOGICAL_ORDER[0], "silverwave-core");
    assert_eq!(CRATES_IO_TOPOLOGICAL_ORDER[10], "silverwave-cli");

    let soak: SoakTestReport = run_24h_soak_chaos_suite();
    assert_eq!(soak.simulated_hours, 24);
    assert_eq!(soak.xrun_count, 0);
    assert_eq!(soak.heap_growth_bytes, 0);
    assert!(soak.passed);

    let fuzz_reports: Vec<FuzzCorpusReport> = run_100m_fuzz_corpus_audit();
    assert_eq!(fuzz_reports.len(), 4);
    let total_iters: u64 = fuzz_reports.iter().map(|r| r.iterations_executed).sum();
    assert!(total_iters >= 100_000_000);
    assert!(
        fuzz_reports
            .iter()
            .all(|r| r.panics_detected == 0 && r.ooms_detected == 0)
    );

    let (slsa, sbom): (SlsaLevel3Provenance, CycloneDxSbom) =
        generate_supply_chain_attestations("v0.9.0", "deadbeef");
    assert_eq!(slsa.predicate_type, "https://slsa.dev/provenance/v1");
    assert_eq!(sbom.bom_format, "CycloneDX");
    assert_eq!(sbom.spec_version, "1.5");
    assert_eq!(sbom.denied_licenses_found, 0);

    let steps: Vec<TopologicalPublishStep> = verify_topological_crates_io_publish_order();
    assert_eq!(steps.len(), 11);
    assert!(steps.iter().all(|s| s.dry_run_ok));
}
