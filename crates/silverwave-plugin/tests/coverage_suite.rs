//! Line-coverage gap suite for `silverwave-plugin`.
//! Exercises previously uncovered regions of `layout.rs`, `params.rs`,
//! `processor.rs`, and `transport.rs`: `MultiBusAudioLayout::default()`,
//! `DawMacroParamBank::default()`, the macro-slot `ParamValue` automation arm,
//! `schedule_orbit_impulse` capacity push, `process_daw_buffer` automation +
//! sample-accurate MIDI emission paths, `DawTransportSynchronizer::default()`,
//! the `advance_buffer_span` early-`None` guards and host-transport `pos_beats`
//! skip paths, `cycle_to_frame_offset` degenerate guards, and the
//! `TonalTheoryDescriptor::resolve_scale` `"dorian"` + fallback arms.

#![forbid(unsafe_code)]

use silverwave_core::{Rational64, TimeSpan};
use silverwave_io::{HostTransportSnapshot, PluginMidiEvent, PluginSyncMode};
use silverwave_plugin::{
    DawAutomationEvent, DawMacroParamBank, DawTransportSynchronizer, MultiBusAudioLayout,
    NUM_DAW_MACRO_PARAMS, NUM_PLUGIN_STEREO_ORBIT_BUSES, OrbitBusRole, ScheduledOrbitImpulse,
    SilverWaveDawPlugin, TonalTheoryDescriptor,
};

fn playing_snapshot(pos_beats: Option<f64>) -> HostTransportSnapshot {
    HostTransportSnapshot {
        playing: true,
        tempo_bpm: 120.0,
        pos_beats,
        pos_samples: Some(0),
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

/// Covers `layout.rs` `MultiBusAudioLayout::default()` (lines 71-73).
#[test]
fn multibus_layout_default_matches_canonical_16_orbit_layout() {
    let layout = MultiBusAudioLayout::default();
    assert!(layout.has_stereo_sidechain_input);
    assert_eq!(layout.output_buses.len(), NUM_PLUGIN_STEREO_ORBIT_BUSES);
    assert_eq!(layout.total_output_channels(), 32);
    assert_eq!(layout.output_buses[0].role, OrbitBusRole::MainStereo);
    assert_eq!(layout.output_buses[15].role, OrbitBusRole::AuxiliaryStereo);
    assert_eq!(layout.channel_pair_for_orbit(15), Some((30, 31)));
}

/// Covers `params.rs` `DawMacroParamBank::default()` (lines 83-85) and the
/// macro-slot `ParamValue` automation arm (lines 118-121).
#[test]
fn macro_param_bank_default_and_slot_param_value_arm() {
    let mut bank = DawMacroParamBank::default();
    assert_eq!(bank.slots.len(), NUM_DAW_MACRO_PARAMS);
    assert_eq!(bank.slots[5].name, "Macro Slider 5");

    // Hits the `else if let Some(slot)` arm for a non-xfader in-range param id.
    bank.apply_automation(DawAutomationEvent::ParamValue {
        sample_offset: 32,
        param_id: 5,
        normalized_value: 0.4,
    });
    assert!((bank.slots[5].current_value - 0.4).abs() < 1.0e-6);
    assert!((bank.slots[5].target_value - 0.4).abs() < 1.0e-6);
    assert!((bank.effective_value(5, None) - 0.4).abs() < 1.0e-6);

    // Out-of-range non-xfader param id takes the `None` side of `get_mut`.
    bank.apply_automation(DawAutomationEvent::ParamValue {
        sample_offset: 0,
        param_id: 200,
        normalized_value: 0.9,
    });
    assert!((bank.slots[5].current_value - 0.4).abs() < 1.0e-6);
}

/// Covers `processor.rs` `schedule_orbit_impulse` capacity push (line 70
/// region), the `process_daw_buffer` automation-event loop (line 88), and the
/// `emit_sample_accurate_midi` Note-On emission (line 113 region).
#[test]
fn processor_schedules_impulses_applies_automation_and_emits_midi() {
    let mut plugin = SilverWaveDawPlugin::default();
    assert!(plugin.config.emit_sample_accurate_midi);

    plugin.schedule_orbit_impulse(ScheduledOrbitImpulse {
        sample_offset: 12,
        orbit_index: 7,
        midi_note: 64,
        velocity: 0.9,
        gain_left: 0.8,
        gain_right: 0.8,
    });

    let automation = [DawAutomationEvent::ParamValue {
        sample_offset: 0,
        param_id: 5,
        normalized_value: 0.4,
    }];
    let mut buses = [[0.0f32; 2]; NUM_PLUGIN_STEREO_ORBIT_BUSES];
    let mut midi_out: Vec<PluginMidiEvent> = Vec::new();
    let snap = playing_snapshot(Some(4.0));
    plugin.process_daw_buffer(&snap, &automation, 128, &mut buses, &mut midi_out);

    // Automation event was applied through the loop (line 88).
    assert!((plugin.params.slots[5].current_value - 0.4).abs() < 1.0e-6);
    // Impulse rendered onto orbit bus 7.
    assert!(buses[7][0] > 0.0);
    assert!(buses[7][1] > 0.0);
    // Sample-accurate Note-On emitted (lines 104-113).
    assert_eq!(midi_out.len(), 1);
    assert_eq!(
        midi_out[0],
        PluginMidiEvent::NoteOn {
            timing: 12,
            channel: 7,
            note: 64,
            velocity: 0.9,
        }
    );

    // Overflow past the 256-impulse pre-allocation takes the `else` side of the
    // `len < capacity` guard — excess impulses are silently dropped (`PP-12`).
    for _ in 0..300 {
        plugin.schedule_orbit_impulse(ScheduledOrbitImpulse {
            sample_offset: 0,
            orbit_index: 1,
            midi_note: 36,
            velocity: 0.5,
            gain_left: 0.5,
            gain_right: 0.5,
        });
    }
    midi_out.clear();
    plugin.process_daw_buffer(&snap, &[], 128, &mut buses, &mut midi_out);
    assert_eq!(midi_out.len(), 256);
}

/// Covers `transport.rs` `DawTransportSynchronizer::default()` (lines 25-27),
/// the `advance_buffer_span` `None` guards (line 57), the host-transport
/// `pos_beats` else/skip path (line 66 region), and both `cycle_to_frame_offset`
/// degenerate guards (lines 87 and 91).
#[test]
fn transport_synchronizer_default_and_guard_paths() {
    let mut sync = DawTransportSynchronizer::default();
    assert_eq!(sync.sync_mode, PluginSyncMode::HostTransport);
    assert_eq!(sync.sample_rate, 48_000.0);

    // `!playing` early return (line 57).
    let stopped = HostTransportSnapshot {
        playing: false,
        ..playing_snapshot(Some(4.0))
    };
    assert!(sync.advance_buffer_span(&stopped, 128).is_none());

    // `buffer_frames == 0` early return (line 57).
    assert!(
        sync.advance_buffer_span(&playing_snapshot(Some(4.0)), 0)
            .is_none()
    );

    // `pos_beats: None` skips the host-transport re-sync block (line 66 region)
    // but still advances by internal free-run and returns a span.
    let before = sync.current_cycle_f64;
    let free_span = sync
        .advance_buffer_span(&playing_snapshot(None), 480)
        .expect("free-running span");
    assert!(free_span.end > free_span.begin);
    assert!(sync.current_cycle_f64 > before);

    // `InternalOrLink` sync mode also skips the host pos_beats re-sync block.
    sync.sync_mode = PluginSyncMode::InternalOrLink;
    let link_span = sync
        .advance_buffer_span(&playing_snapshot(Some(4.0)), 480)
        .expect("internal-clock span");
    assert!(link_span.end > link_span.begin);

    // `HostTransport` + `pos_beats` takes the re-sync block's true side:
    // `current_cycle_f64` snaps to host position (monotonic-clamped).
    sync.sync_mode = PluginSyncMode::HostTransport;
    sync.current_cycle_f64 = 0.0;
    let host_span = sync
        .advance_buffer_span(&playing_snapshot(Some(8.0)), 480)
        .expect("host-transport span");
    assert_eq!(host_span.begin, Rational64::from_integer(2));

    // `buffer_frames == 0` guard in `cycle_to_frame_offset` (line 87).
    assert_eq!(
        DawTransportSynchronizer::cycle_to_frame_offset(link_span, link_span.begin, 0),
        0
    );

    // Degenerate zero-duration span (`dur <= 1e-12`) guard (line 91).
    let degenerate = TimeSpan {
        begin: Rational64::from_integer(2),
        end: Rational64::from_integer(2),
    };
    assert_eq!(
        DawTransportSynchronizer::cycle_to_frame_offset(
            degenerate,
            Rational64::from_integer(2),
            256
        ),
        0
    );
}

/// Covers `transport.rs` `TonalTheoryDescriptor::resolve_scale` `"dorian"` arm
/// (lines 119-122) and the catch-all fallback arm (lines 123-126).
#[test]
fn tonal_resolve_dorian_and_fallback_scale_arms() {
    let dorian = TonalTheoryDescriptor::resolve_scale("D:dorian");
    assert_eq!(dorian.tonic, "D");
    assert_eq!(dorian.scale_name, "dorian");
    assert_eq!(
        dorian.intervals,
        vec!["1P", "2M", "3m", "4P", "5P", "6M", "7m"]
    );
    assert!(dorian.pitch_classes.contains(&"Bb".to_string()));
    assert!(dorian.to_json().contains(r#""scale":"dorian""#));

    // Catch-all arm: known major and unknown scale names both resolve to major.
    let major = TonalTheoryDescriptor::resolve_scale("G:major");
    assert_eq!(major.scale_name, "major");
    assert_eq!(
        major.intervals,
        vec!["1P", "2M", "3M", "4P", "5P", "6M", "7M"]
    );

    let fallback = TonalTheoryDescriptor::resolve_scale("E:locrian");
    assert_eq!(fallback.scale_name, "locrian");
    assert_eq!(fallback.pitch_classes[6], "B");

    // No ':' separator defaults to major on the same fallback arm.
    let bare = TonalTheoryDescriptor::resolve_scale("F");
    assert_eq!(bare.tonic, "F");
    assert_eq!(bare.scale_name, "major");
}

/// Remaining zero-segment edges: `layout.rs:112` `?` early-`None`,
/// `processor.rs:99` `else` arm (`macro0 <= 0.0` -> unity scale),
/// `processor.rs:112` `emit_sample_accurate_midi == false` edge, and the
/// `transport.rs:75-76` `?` non-finite `Rational64` guards.
#[test]
fn plugin_remaining_zero_segment_edges() {
    // `layout.rs:112` — orbit index beyond the 16 stereo buses -> `None`.
    let layout = MultiBusAudioLayout::default();
    assert_eq!(layout.channel_pair_for_orbit(16), None);
    assert_eq!(layout.channel_pair_for_orbit(usize::MAX), None);

    // `processor.rs:99` else arm — automation drives macro 0 to `0.0`, so the
    // `macro0 > 0.0` guard fails and `scale` falls back to `1.0`.
    let mut plugin = SilverWaveDawPlugin::default();
    plugin.schedule_orbit_impulse(ScheduledOrbitImpulse {
        sample_offset: 0,
        orbit_index: 3,
        midi_note: 60,
        velocity: 1.0,
        gain_left: 0.5,
        gain_right: 0.5,
    });
    let zero_macro = [DawAutomationEvent::ParamValue {
        sample_offset: 0,
        param_id: 0,
        normalized_value: 0.0,
    }];
    let mut buses = [[0.0f32; 2]; NUM_PLUGIN_STEREO_ORBIT_BUSES];
    let mut midi_out: Vec<PluginMidiEvent> = Vec::new();
    let snap = playing_snapshot(Some(2.0));
    plugin.process_daw_buffer(&snap, &zero_macro, 128, &mut buses, &mut midi_out);
    // scale == 1.0 -> `gain * velocity` lands unscaled on orbit bus 3.
    assert!((buses[3][0] - 0.5).abs() < 1.0e-6);
    assert!((buses[3][1] - 0.5).abs() < 1.0e-6);

    // `processor.rs:112` — `emit_sample_accurate_midi == false` skips the
    // `out_midi.push` while still writing the orbit bus.
    plugin.config.emit_sample_accurate_midi = false;
    plugin.schedule_orbit_impulse(ScheduledOrbitImpulse {
        sample_offset: 4,
        orbit_index: 0,
        midi_note: 36,
        velocity: 0.7,
        gain_left: 0.6,
        gain_right: 0.6,
    });
    midi_out.clear();
    plugin.process_daw_buffer(&snap, &[], 128, &mut buses, &mut midi_out);
    assert!(midi_out.is_empty());
    assert!(buses[0][0] > 0.0);

    // `transport.rs:75` — `pos_beats = inf` makes `start_c` non-finite, so the
    // first `from_f64_approx` `?` returns `None`.
    let mut sync = DawTransportSynchronizer::new(48_000.0);
    let inf_pos = playing_snapshot(Some(f64::INFINITY));
    assert!(sync.advance_buffer_span(&inf_pos, 128).is_none());

    // `transport.rs:76` — infinite `tempo_bpm` makes `end_c` non-finite, so
    // the second `from_f64_approx` `?` returns `None`.
    let mut sync2 = DawTransportSynchronizer::new(48_000.0);
    let mut hot = playing_snapshot(Some(4.0));
    hot.tempo_bpm = f64::INFINITY;
    assert!(sync2.advance_buffer_span(&hot, 128).is_none());
}
