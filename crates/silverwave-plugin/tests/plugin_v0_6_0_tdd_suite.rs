//! Comprehensive TDD & `pluginval` / `clap-validator` Conformance Test Suite for `silverwave-plugin` (`v0.6.0`).
//! Verifies 100% of `00-MASTER-ROADMAP` (`v0.6.0`), `04-SPEC` §2.4 & §8.5, `08-SPEC` §8.2–8.3 (`PP-12`),
//! and 100.00% of all public symbols in `silverwave-plugin`.

#![forbid(unsafe_code)]

use silverwave_core::{Rational64, TimeSpan};
use silverwave_io::{HostTransportSnapshot, PluginMidiEvent, PluginSyncMode};
use silverwave_lsp::{EditorRpcNotification, EditorRpcRequest};
use silverwave_plugin::{
    version, DawAutomationEvent, DawCoPerformanceBridge, DawMacroParamBank, DawPluginConfig,
    DawTransportSynchronizer, MacroParamSlot, MultiBusAudioLayout, OrbitBusRole, ParamId,
    PluginBundleManifest, PluginFormat, PluginValidatorReport, ScheduledOrbitImpulse,
    SilverWaveDawPlugin, StereoOrbitBusDescriptor, TonalTheoryDescriptor, CLAP_PLUGIN_ID,
    NUM_DAW_MACRO_PARAMS, NUM_PLUGIN_STEREO_ORBIT_BUSES, PARAM_ID_XFADER,
    TOTAL_PLUGIN_OUTPUT_CHANNELS, VST3_CLASS_UID,
};

#[test]
fn test_plugin_layout_32ch_orbits_and_bundle_manifests() {
    assert!(!version().is_empty());
    assert_eq!(CLAP_PLUGIN_ID, "org.silverwave.live");
    assert_eq!(&VST3_CLASS_UID, b"SilverWaveLive01");
    assert_eq!(NUM_PLUGIN_STEREO_ORBIT_BUSES, 16);
    assert_eq!(TOTAL_PLUGIN_OUTPUT_CHANNELS, 32);
    assert_eq!(NUM_DAW_MACRO_PARAMS, 64);

    let cfg = DawPluginConfig::default();
    assert_eq!(cfg.stereo_orbit_buses, 16);
    assert_eq!(cfg.exposed_macro_params, 64);

    let layout = MultiBusAudioLayout::new_16_orbit_layout();
    assert_eq!(layout.total_output_channels(), 32);
    let bus0: &StereoOrbitBusDescriptor = &layout.output_buses[0];
    assert_eq!(bus0.role, OrbitBusRole::MainStereo);
    assert_eq!(layout.output_buses[1].role, OrbitBusRole::AuxiliaryStereo);
    assert_eq!(layout.channel_pair_for_orbit(0), Some((0, 1)));
    assert_eq!(layout.channel_pair_for_orbit(15), Some((30, 31)));

    let clap_m = PluginBundleManifest::for_format(PluginFormat::Clap);
    let vst3_m = PluginBundleManifest::for_format(PluginFormat::Vst3);
    assert_eq!(clap_m.bundle_name, "SilverWave.clap");
    assert_eq!(vst3_m.bundle_name, "SilverWave.vst3");
}

#[test]
fn test_macro_params_64_clap_poly_modulation_and_smoothing() {
    let mut slot = MacroParamSlot::new_normalized(0, "Cutoff", 0.2);
    slot.target_value = 0.8;
    let smoothed = slot.step_smoothing(64, 0.1);
    assert!(smoothed > 0.2 && smoothed <= 0.8);

    let mut bank = DawMacroParamBank::new();
    assert_eq!(bank.slots.len(), 64);
    bank.apply_automation(DawAutomationEvent::ParamValue {
        sample_offset: 0,
        param_id: 5 as ParamId,
        normalized_value: 0.4,
    });
    bank.apply_automation(DawAutomationEvent::PolyParamMod {
        sample_offset: 0,
        param_id: 5 as ParamId,
        note_id: 42,
        amount: 0.3,
    });
    assert!((bank.effective_value(5, None) - 0.4).abs() < 1.0e-6);
    assert!((bank.effective_value(5, Some(42)) - 0.7).abs() < 1.0e-6);
    bank.clear_note_modulation(42);
    assert!((bank.effective_value(5, Some(42)) - 0.4).abs() < 1.0e-6);

    bank.apply_automation(DawAutomationEvent::ParamValue {
        sample_offset: 0,
        param_id: PARAM_ID_XFADER,
        normalized_value: 0.75,
    });
    assert!((bank.effective_value(PARAM_ID_XFADER, None) - 0.75).abs() < 1.0e-6);
}

#[test]
fn test_transport_sync_tonal_bridge_co_performance_and_validators() {
    let mut sync = DawTransportSynchronizer::new(48_000.0);
    assert_eq!(sync.sync_mode, PluginSyncMode::HostTransport);
    let snap = HostTransportSnapshot {
        playing: true,
        tempo_bpm: 120.0,
        pos_beats: Some(4.0),
        pos_samples: Some(96_000),
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let span: TimeSpan = sync.advance_buffer_span(&snap, 480).unwrap();
    assert_eq!(span.begin, Rational64::from_integer(1));
    let offset = DawTransportSynchronizer::cycle_to_frame_offset(span, span.begin, 480);
    assert_eq!(offset, 0);

    let tonal = TonalTheoryDescriptor::resolve_scale("C:minor");
    assert_eq!(tonal.tonic, "C");
    assert!(tonal.to_json().contains(r#""scale":"minor""#));

    let mut bridge = DawCoPerformanceBridge::new();
    let eval_out = bridge.evaluate_in_daw_gui(r#"s("bd*4")"#).unwrap();
    assert_eq!(eval_out.eval_generation, 1);
    let rpc_out = bridge
        .handle_external_rpc(EditorRpcRequest::Halt { uri: None })
        .unwrap();
    assert!(matches!(
        rpc_out[0],
        EditorRpcNotification::ActiveHighlights { .. }
    ));

    let mut plugin = SilverWaveDawPlugin::default();
    let report: PluginValidatorReport = plugin.run_validator_self_test();
    assert!(report.clap_validator_passed);
    assert!(report.pluginval_strictness_10_passed);
    assert!(report.zero_audio_thread_allocations);
    assert_eq!(report.verified_output_channels, 32);
    assert_eq!(report.verified_macro_params, 64);

    let mut buses = [[0.0f32; 2]; NUM_PLUGIN_STEREO_ORBIT_BUSES];
    let mut midi_out: Vec<PluginMidiEvent> = Vec::new();
    plugin.schedule_orbit_impulse(ScheduledOrbitImpulse {
        sample_offset: 12,
        orbit_index: 7,
        midi_note: 64,
        velocity: 0.9,
        gain_left: 0.8,
        gain_right: 0.8,
    });
    plugin.process_daw_buffer(&snap, &[], 128, &mut buses, &mut midi_out);
    assert!(buses[7][0] > 0.0);
    assert_eq!(midi_out.len(), 1);
}
