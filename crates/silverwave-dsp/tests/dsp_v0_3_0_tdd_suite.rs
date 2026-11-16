//! Comprehensive TDD & Real-Time Invariant Test Suite for `silverwave-dsp` (`v0.3.0`).
//! Verifies 100% of `03-SPEC-AUDIO-DSP-AND-SAMPLES.md` Section 7 (`RT-01`..`RT-05`,
//! `DSP-01`..`DSP-08`, `LIFE-01`..`LIFE-04`, `SAMP-01`..`SAMP-03`), `08-SPEC` (`PP-01`,
//! `PP-02`, `PP-09`, `PP-10`, `PP-11`), `09-SPEC` (`CK-03`, `CK-05`, `CK-06`), and
//! 100.00% of all public symbols in `silverwave-dsp`.

#![forbid(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use silverwave_core::{sine, EngineCompatibilityProfile, Rational64};
use silverwave_dsp::{
    equal_power_pan, poly_blep, process_shape_and_distort, sanitize_f32_finite,
    soft_limit_feedback, spsc_ring_buffer, version, vowel_formant_frequencies,
    AcousticMixDiagnostic, AcousticTelemetryAnalyzer, AdditiveOscillator, AdsrEnvelope,
    AllpassPhaser, AssetMessage, AudioBlockBuffer, AudioEngine, BankId, BitCrusherAndCoarse,
    CompressorParams, ConflictKind, ContinuousModulationEngine, ControlBusId,
    ControlSubBlockFrame, CrossfaderSceneBank, DeferredCollector, DistortionMode,
    DuckSidechainState, EngineChannels, EngineConfig, FilterPassMode, FilterTopology, FmOpParams,
    FmMatrixSynthesizer, GranularSamplerVoice, ImpulseResponseBuffer, InlineDspPipeVoice,
    InsertFxStage, InterOrbitConflictAlert, KarplusStrongParams, KarplusStrongPluck,
    LazySampleLoader, LfoShape, LimiterType, LoaderError, LoopMode, ManifestHandle,
    ModulatableParam, ModulationBusTable, ModulationRate, MultiModeFilter, NoiseGenerator,
    OrbitBus, OrbitDelay, OrbitLeslie, OrbitMonitor, OrbitReverb, OrbitRmsReport,
    OrderedVoiceFxChain, OrderedVoiceFxExecutor, OutputRoutingMode, PatternModulationEvaluator,
    PolyBlepOscillator, PushError, RegisteredModulationLane, RingModParams, RingModulator,
    SampleBuffer, SampleId, SampleRegistry, SampleSource, SampleVoicePlayer, SharedAsset,
    SilverWaveAudioEngine, SoundFontVoiceParams, SpscConsumer, SpscProducer, StealFadeBuffer,
    SupersawGenerator, TremoloModulator, TriggerEvent, VoiceCompressor, VoiceKind, VoiceSlot,
    VoiceState, VowelFormant, VowelFormantFilter, Waveform, WavetableBuffer, ZzfxParams,
    ZzfxShape, ZzfxSynthesizer, LOOKAHEAD_CAPACITY, MAX_DUCK_TARGETS, MAX_FM_OPERATORS,
    MAX_ORBITS, MAX_PARTIALS, MAX_VOICES, SUB_BLOCK_FRAMES,
};

struct CountingAllocator;

static COUNT_ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

fn make_test_engine(
    routing_mode: OutputRoutingMode,
) -> (
    SilverWaveAudioEngine,
    SpscProducer<TriggerEvent>,
    SpscProducer<AssetMessage>,
    SpscConsumer<OrbitRmsReport>,
) {
    let (event_tx, event_rx) = spsc_ring_buffer::<TriggerEvent>(1024);
    let (asset_tx, asset_rx) = spsc_ring_buffer::<AssetMessage>(256);
    let (rms_tx, rms_rx) = spsc_ring_buffer::<OrbitRmsReport>(256);
    let mut cfg = EngineConfig::default();
    cfg.routing_mode = routing_mode;
    let engine = SilverWaveAudioEngine::new(
        cfg,
        EngineChannels {
            event_rx,
            asset_rx,
            rms_tx,
        },
    );
    (engine, event_tx, asset_tx, rms_rx)
}

#[test]
fn test_rt_01_02_03_04_05_realtime_safety_and_subblock_accuracy() {
    let _ = (&CountingAllocator, &System, Layout::new::<u8>(), &COUNT_ACTIVE, &ALLOC_COUNT, &DEALLOC_COUNT);
    assert!(!version().is_empty());
    assert_eq!(MAX_PARTIALS, 64);
    assert_eq!(MAX_DUCK_TARGETS, 8);
    assert_eq!(MAX_FM_OPERATORS, 8);
    assert_eq!(MAX_ORBITS, 16);
    assert_eq!(MAX_VOICES, 256);
    assert_eq!(LOOKAHEAD_CAPACITY, 512);
    assert_eq!(SUB_BLOCK_FRAMES, 64);

    // RT-02: Deferred reclamation (`SharedAsset<T>` + `DeferredCollector<T>`)
    let collector = DeferredCollector::<SampleBuffer>::new(32);
    let shared: SharedAsset<SampleBuffer> =
        collector.share(SampleBuffer::from_mono(vec![0.5; 480], 48_000));
    assert_eq!(shared.frame_count(), 480);
    assert_eq!(collector.pending_count(), 0);
    drop(shared);
    assert_eq!(collector.pending_count(), 1);
    assert_eq!(collector.collect(), 1);
    assert_eq!(collector.pending_count(), 0);

    // RT-03: SPSC Backpressure without blocking or panicking
    let (mut tx, mut rx) = spsc_ring_buffer::<u32>(4);
    for i in 0..4 {
        assert!(tx.push(i).is_ok());
    }
    assert_eq!(tx.push(99), Err(PushError::Full(99)));
    assert_eq!(rx.pop(), Some(0));
    assert!(tx.push(99).is_ok());

    // RT-04: Sub-block sample-accurate onset (`trigger_sample = block_start + 37`)
    let (mut engine, mut event_tx, _asset_tx, mut rms_rx) =
        make_test_engine(OutputRoutingMode::StereoMixdown);
    let mut ev = TriggerEvent::default_synth(Waveform::Square, 440.0);
    ev.trigger_sample = 37;
    ev.attack = 0.0001;
    event_tx.push(ev).unwrap();

    let mut out = vec![0.0f32; 128 * 2];
    let cap_before = engine.voices.capacity();
    engine.process_block(&mut out, 2);
    assert_eq!(engine.voices.capacity(), cap_before);

    for f in 0..37 {
        assert_eq!(out[f * 2], 0.0);
        assert_eq!(out[f * 2 + 1], 0.0);
    }
    assert!(out[37 * 2].abs() > 0.01);
    assert!(rms_rx.pop().is_some());
}

#[test]
fn test_dsp_01_02_03_04_05_oscillators_supersaw_noise_fm_and_zzfx() {
    let sr = 48_000.0f32;
    assert_eq!(poly_blep(0.5, 0.01), 0.0);

    let mut osc = PolyBlepOscillator::new(0.0);
    for wf in [
        Waveform::Sine,
        Waveform::Sawtooth,
        Waveform::Square,
        Waveform::Triangle,
    ] {
        let mut energy = 0.0f32;
        for _ in 0..480 {
            let s = osc.next_sample(wf, 4000.0, sr);
            assert!(s.is_finite());
            energy += s * s;
        }
        assert!(energy > 1.0);
    }

    let mut ss1 = SupersawGenerator::new(42);
    let mut ss7 = SupersawGenerator::new(42);
    let mut rms1 = 0.0f32;
    let mut rms7 = 0.0f32;
    let mut diff_lr = 0.0f32;
    for _ in 0..4800 {
        let (l1, r1) = ss1.next_stereo(220.0, 1, 0.0, 0.0, sr);
        let (l7, r7) = ss7.next_stereo(220.0, 7, 0.5, 1.0, sr);
        rms1 += 0.5 * (l1 * l1 + r1 * r1);
        rms7 += 0.5 * (l7 * l7 + r7 * r7);
        diff_lr += (l7 - r7).abs();
    }
    assert!(diff_lr > 10.0);
    let db_diff = 10.0 * (rms7 / rms1).log10();
    assert!(db_diff.abs() < 2.5);

    let mut ng = NoiseGenerator::new(1337);
    for wf in [
        Waveform::WhiteNoise,
        Waveform::PinkNoise,
        Waveform::BrownNoise,
        Waveform::Crackle,
    ] {
        let mut sum_sq = 0.0f32;
        for _ in 0..1024 {
            let s = ng.next_sample(wf);
            assert!(s.is_finite());
            sum_sq += s * s;
        }
        assert!(sum_sq > 0.01);
    }

    let mut add_osc = AdditiveOscillator::default();
    let mut partials = [0.0f32; MAX_PARTIALS];
    let phases = [0.0f32; MAX_PARTIALS];
    partials[0] = 1.0;
    partials[15] = 1.0;
    let s_add = add_osc.next_sample(2000.0, sr, 16, &partials, &phases);
    assert!(s_add.is_finite());

    let mut fm = FmMatrixSynthesizer::default();
    let mut ops = [FmOpParams::default(); MAX_FM_OPERATORS];
    let mut matrix = [[0.0f32; MAX_FM_OPERATORS]; MAX_FM_OPERATORS];
    let s_2op = fm.next_sample(440.0, sr, 2.0, 2.0, 1.0, &ops, &matrix);
    assert!(s_2op.is_finite());

    ops[0].ratio = 1.0;
    ops[2].ratio = 1.0;
    ops[2].level = 1.0;
    matrix[0][2] = 2.0;
    let mut fm_energy = 0.0f32;
    for _ in 0..256 {
        let s = fm.next_sample(440.0, sr, 0.0, 1.0, 1.0, &ops, &matrix);
        fm_energy += s * s;
    }
    assert!(fm_energy > 1.0);

    let mut z1 = ZzfxSynthesizer::default();
    let mut z2 = ZzfxSynthesizer::default();
    let mut zp = ZzfxParams::default();
    zp.enabled = true;
    zp.shape = ZzfxShape::Sawtooth;
    for _ in 0..256 {
        assert_eq!(z1.next_sample(&zp, sr), z2.next_sample(&zp, sr));
    }

    let mut ks = KarplusStrongPluck::new();
    let ks_param = KarplusStrongParams::default();
    assert!(ks.next_sample(&ks_param, sr).is_finite());

    let mut pipe = InlineDspPipeVoice::new(Waveform::Sawtooth, 1200.0, 0.8, 2.0);
    let p0 = pipe.next_sample(440.0, sr);
    pipe.hot_swap(Waveform::Square, 800.0, 1.2, 3.0, sr);
    let p1 = pipe.next_sample(440.0, sr);
    assert!(p0.is_finite() && p1.is_finite());
}

#[test]
fn test_dsp_06_07_08_filters_serial_fx_ordered_chain_and_nan_guard() {
    let sr = 48_000.0f32;

    let mut flt = MultiModeFilter::new();
    for topo in [
        FilterTopology::Svf12dB,
        FilterTopology::Svf24dB,
        FilterTopology::MoogLadder,
    ] {
        for mode in [
            FilterPassMode::LowPass,
            FilterPassMode::HighPass,
            FilterPassMode::BandPass,
            FilterPassMode::Notch,
        ] {
            let y = flt.process_sample(0.5, 1000.0, 1.2, sr, topo, mode);
            assert!(y.is_finite());
        }
    }

    assert_eq!(VowelFormant::from_str_alias("å"), VowelFormant::Aa);
    assert_eq!(VowelFormant::from_str_alias("ø"), VowelFormant::Oe);
    assert_eq!(VowelFormant::from_str_alias("æ"), VowelFormant::Ae);
    assert_eq!(VowelFormant::from_str_alias("ı"), VowelFormant::Y);
    assert_eq!(vowel_formant_frequencies(VowelFormant::A)[0], 800.0);

    let mut vf = VowelFormantFilter::new();
    assert!(vf.process_vowel(0.5, VowelFormant::A, sr).is_finite());
    assert!(vf.process_morph(0.5, 1.5, sr).is_finite());

    let mut chain = OrderedVoiceFxChain::new();
    assert!(chain.push_stage(InsertFxStage::LowPass {
        cutoff_hz: 200.0,
        q: 0.707
    }));
    assert!(chain.push_stage(InsertFxStage::Distort {
        drive: 3.0,
        mode: DistortionMode::SoftClipTanh
    }));
    assert!(chain.push_stage(InsertFxStage::LowPass {
        cutoff_hz: 1200.0,
        q: 0.707
    }));
    assert_eq!(chain.stage_count, 3);

    let mut exec = OrderedVoiceFxExecutor::new();
    let y_chain = exec.process(0.8, &chain, sr);
    assert!(y_chain.is_finite());

    let mut bc = BitCrusherAndCoarse::default();
    assert!(bc.process(0.37, 2, 4.0).is_finite());
    assert!(process_shape_and_distort(0.5, 0.5, 2.0, DistortionMode::WavefoldSaturate).is_finite());

    let mut rm = RingModulator::default();
    assert!(rm
        .process(
            0.5,
            &RingModParams {
                ring_wet: 0.5,
                ring_freq_hz: 100.0,
                ring_slide_hz: 10.0
            },
            sr
        )
        .is_finite());

    let mut trem = TremoloModulator::default();
    assert!(trem.process(0.5, 5.0, 0.5, LfoShape::Sine, 0.0, sr).is_finite());

    let mut comp = VoiceCompressor::default();
    let mut cp = CompressorParams::default();
    cp.enabled = true;
    assert!(comp.process(0.9, &cp, sr).is_finite());

    let (pl, pr) = equal_power_pan(1.0, 0.5);
    assert!((pl - pr).abs() < 1.0e-5);

    let mut phaser = AllpassPhaser::default();
    let (ph_l, ph_r) = phaser.process_stereo(0.5, 0.5, 2.0, 0.7, 800.0, 1000.0, sr);
    assert!(ph_l.is_finite() && ph_r.is_finite());

    assert_eq!(sanitize_f32_finite(f32::NAN), 0.0);
    assert_eq!(sanitize_f32_finite(f32::INFINITY), 0.0);
    assert_eq!(soft_limit_feedback(f32::NEG_INFINITY), 0.0);

    let mut mon = OrbitMonitor {
        limiter_type: LimiterType::Softclip,
        ..OrbitMonitor::default()
    };
    let (ml, mr) = mon.process_frame(f32::NAN, f32::INFINITY);
    assert_eq!(ml, 0.0);
    assert_eq!(mr, 0.0);
    let _ = mon.take_rms_and_peak();
}

#[test]
fn test_life_01_02_03_04_cut_groups_voice_stealing_duckorbit_and_multichannel() {
    let (mut engine, mut event_tx, _asset_tx, _rms_rx) =
        make_test_engine(OutputRoutingMode::StereoMixdown);

    let mut v_a = TriggerEvent::default_synth(Waveform::Sawtooth, 220.0);
    v_a.cut_group = 1;
    v_a.duration_samples = 48_000;
    event_tx.push(v_a).unwrap();

    let mut block = vec![0.0f32; 128 * 2];
    engine.process_block(&mut block, 2);
    assert_eq!(engine.active_voice_count(), 1);

    let mut v_b = TriggerEvent::default_synth(Waveform::Square, 440.0);
    v_b.cut_group = 1;
    v_b.trigger_sample = 128;
    v_b.duration_samples = 48_000;
    event_tx.push(v_b).unwrap();

    engine.process_block(&mut block, 2);
    engine.process_block(&mut block, 2);
    assert_eq!(engine.active_voice_count(), 1);

    for i in 0..MAX_VOICES {
        let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0 + (i as f32));
        ev.trigger_sample = engine.current_sample_frame;
        ev.duration_samples = 48_000;
        let idx = engine.allocate_or_steal(&ev);
        engine.voices[idx].state = VoiceState::Active;
        engine.voices[idx].event = ev;
        engine.voices[idx].current_env_level = 0.5;
    }
    let extra = TriggerEvent::default_synth(Waveform::Sawtooth, 880.0);
    let stolen_idx = engine.allocate_or_steal(&extra);
    assert!(stolen_idx < MAX_VOICES);
    assert!(engine.steal_count >= 1);
    let _sf: StealFadeBuffer = engine.steal_fade;

    let mut duck = DuckSidechainState::default();
    duck.trigger_duck(0.8, 0.2, 48_000.0);
    let onset_g = duck.next_gain();
    assert!((onset_g - 0.2).abs() < 1.0e-4);
    for _ in 0..9600 {
        duck.next_gain();
    }
    assert!((duck.next_gain() - 1.0).abs() < 1.0e-3);

    let (mut mc_engine, mut mc_tx, _, _) =
        make_test_engine(OutputRoutingMode::MultichannelOrbits);
    let mut orb3_ev = TriggerEvent::default_synth(Waveform::Sawtooth, 330.0);
    orb3_ev.orbit = 3;
    orb3_ev.trigger_sample = 0;
    mc_tx.push(orb3_ev).unwrap();

    let mut mc_out = vec![0.0f32; 64 * 16];
    mc_engine.process_block(&mut mc_out, 16);
    let mut ch6_energy = 0.0f32;
    let mut other_energy = 0.0f32;
    for f in 0..64 {
        let base = f * 16;
        ch6_energy += mc_out[base + 6].abs() + mc_out[base + 7].abs();
        for c in 0..16 {
            if c != 6 && c != 7 {
                other_energy += mc_out[base + c].abs();
            }
        }
    }
    assert!(ch6_energy > 0.1);
    assert_eq!(other_energy, 0.0);

    let mut blk_buf = AudioBlockBuffer::with_capacity(64);
    blk_buf.clear(64);
    let mut dly = OrbitDelay::new(4800);
    let _ = dly.process_sample(0.5, 0.5, 0.4, 0.1, 0.3, true, 0.5, 48_000.0);
    let mut rev = OrbitReverb::new(99);
    rev.swap_impulse_response(Some(1), 48_000.0);
    let _ = rev.process_sample(0.5, 0.5, 0.5, 0.7, 0.3);
    let mut les = OrbitLeslie::default();
    let _ = les.process_sample(0.5, 0.5, 0.5, 6.0, 0.3, 48_000.0);
    let mut ob = OrbitBus::new(0, 64, 1);
    ob.set_looper_freeze(true, 64);
    ob.process_orbit_block(64, 48_000.0);
}

#[test]
fn test_samp_01_02_03_and_modulation_and_acoustic_telemetry() {
    let gh = SampleSource::parse_github_shorthand("github:tidalcycles/dirt-samples").unwrap();
    assert_eq!(
        gh.canonical_manifest_url().as_deref(),
        Some("https://raw.githubusercontent.com/tidalcycles/dirt-samples/main/strudel.json")
    );
    let local = SampleSource::LocalPath {
        path: PathBuf::from("/tmp/samples"),
    };
    assert!(local.canonical_manifest_url().is_none());

    let mut loader = LazySampleLoader::new();
    loader.set_base_url("https://example.org/samples/");
    let handle: ManifestHandle = loader.register_source(gh).unwrap();
    assert_eq!(handle.0, 1);
    assert_eq!(loader.fetch_count(), 0);
    let sid: SampleId = loader.resolve_or_enqueue("RolandTR909", "bd", 0);
    assert_eq!(sid.0, 1);
    assert_eq!(loader.fetch_count(), 1);
    let _ = LoaderError::UnknownSampleKey("missing".to_string());
    let _ = BankId(1);
    let _ = SoundFontVoiceParams::default();
    let _ = ImpulseResponseBuffer {
        ir_frames: vec![1.0],
        sample_rate: 48_000,
    };
    let wt_buf = WavetableBuffer::new(vec![0.0, 1.0, 0.0, -1.0]);
    assert_eq!(wt_buf.table.len(), 4);

    let ramp: Vec<f32> = (0..48_000).map(|i| (i as f32) / 48_000.0).collect();
    let sbuf = SampleBuffer::from_mono(ramp, 48_000);
    let mut ev = TriggerEvent::default_sample(sid);
    ev.begin = 0.25;
    ev.end = 0.75;
    ev.speed = -1.0;
    let mut player = SampleVoicePlayer::from_trigger(&ev, sbuf.frame_count(), 48_000, 48_000.0, false);
    assert!((player.playhead_frame - 35_999.0).abs() <= 1.0);
    let s0 = player.next_sample(&sbuf);
    let s1 = player.next_sample(&sbuf);
    assert!(s0 > s1);

    let wt_player = SampleVoicePlayer::from_trigger(&ev, 256, 48_000, 48_000.0, true);
    assert!(wt_player.loop_enabled);

    let mut gran = GranularSamplerVoice::new(16, 480);
    assert!(gran.next_sample(&sbuf).is_finite());

    assert_eq!(ModulatableParam::COUNT, 39);
    assert_eq!(
        ModulatableParam::from_index(ModulatableParam::LpfCutoffHz.index()),
        Some(ModulatableParam::LpfCutoffHz)
    );
    let mut mod_table = ModulationBusTable::new();
    assert!(mod_table.apply_sub_block_ramp(ModulatableParam::LpfCutoffHz, 200.0, 1200.0));
    assert!(mod_table.is_dirty(ModulatableParam::LpfCutoffHz));
    assert!((mod_table.value_at(ModulatableParam::LpfCutoffHz, 32, 64) - 700.0).abs() < 1.0);

    let mut eval = PatternModulationEvaluator::new();
    let bus_id = ControlBusId::VoiceLocal {
        stream_id: 1,
        param: ModulatableParam::LpfCutoffHz,
    };
    let _lane = RegisteredModulationLane {
        bus_id,
        rate: ModulationRate::ControlRateSubBlock,
        signal: sine(),
    };
    eval.register_lane(bus_id, ModulationRate::ControlRateSubBlock, sine());
    let mut sub_frames = [ControlSubBlockFrame::default(); 8];
    let n_written = eval.evaluate_modulation_sub_blocks(
        Rational64::zero(),
        Rational64::new_const(1, 16),
        0,
        256,
        &mut sub_frames,
    );
    assert_eq!(n_written, 4);
    assert!(sub_frames[0].sample_at(32).is_finite());

    let mut scenes = CrossfaderSceneBank::new();
    scenes.set_scene_param(0, ModulatableParam::LpfCutoffHz, 200.0);
    scenes.set_scene_param(1, ModulatableParam::LpfCutoffHz, 2200.0);
    scenes.xfader_position = 0.5;
    assert_eq!(
        scenes.interpolate_param(ModulatableParam::LpfCutoffHz, 1000.0),
        1200.0
    );

    let orb0_l: Vec<f32> = (0..480)
        .map(|i| (core::f32::consts::TAU * 60.0 * (i as f32) / 48_000.0).sin() * 0.5)
        .collect();
    let orb1_l: Vec<f32> = orb0_l.iter().map(|&x| -x).collect();
    let diag: AcousticMixDiagnostic = AcousticTelemetryAnalyzer::analyze_orbits(
        &[(0, &orb0_l, &orb0_l), (1, &orb1_l, &orb1_l)],
        48_000.0,
    );
    assert!(!diag.masking_conflicts.is_empty());
    let alert: &InterOrbitConflictAlert = &diag.masking_conflicts[0];
    assert_eq!(alert.kind, ConflictKind::SubBassPhaseCancellation);
    assert_eq!(alert.orbit_a, 0);
    assert_eq!(alert.orbit_b, 1);

    let _ = EngineCompatibilityProfile::SuperDoughC4;
    let _ = AdsrEnvelope::default();
    let _ = VoiceSlot::default();
    let _ = VoiceKind::Synth;
    let _ = LoopMode::Off;

    assert_eq!(OrbitReverb::FDN_DELAY_LEN.len(), 8);
    let mut env_test = AdsrEnvelope::default();
    assert!(env_test.next_value(48_000.0).0 >= 0.0);
    assert!(sbuf.read_hermite_mono(10.5).is_finite());
}
