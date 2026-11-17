//! Coverage-focused test suite exercising previously-uncovered paths in
//! `silverwave-dsp`: voice-kind processing arms, serial & ordered FX routing,
//! filter envelopes, lookahead scheduling, voice-stealing tiers, asset
//! ingestion, sample-manipulation helpers, modulation buses, and telemetry.

#![forbid(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use silverwave_core::{Rational64, sine};
use silverwave_dsp::{
    AcousticTelemetryAnalyzer, AdsrEnvelope, AssetMessage, AudioEngine, BitCrusherAndCoarse,
    CompressorParams, ConflictKind, ContinuousModulationEngine, ControlBusId, ControlSubBlockFrame,
    CrossfaderSceneBank, DeferredCollector, DistortionMode, DuckSidechainState, EngineChannels,
    EngineConfig, FmMatrixSynthesizer, FmOpParams, GranularSamplerVoice, ImpulseResponseBuffer,
    InsertFxStage, KarplusStrongParams, KarplusStrongPluck, LazySampleLoader, LfoShape,
    LimiterType, LoopMode, ModulatableParam, ModulationBusTable, ModulationRate, NoiseGenerator,
    OrbitDelay, OrbitMonitor, OrbitReverb, OrbitRmsReport, OrderedVoiceFxChain,
    OrderedVoiceFxExecutor, OutputRoutingMode, PatternModulationEvaluator, PolyBlepOscillator,
    PushError, RingModParams, SampleBuffer, SampleId, SampleRegistry, SampleSource,
    SampleVoicePlayer, SharedAsset, SilverWaveAudioEngine, SpscConsumer, SpscProducer,
    TremoloModulator, TriggerEvent, VoiceCompressor, VoiceKind, VoiceState, VowelFormant,
    VowelFormantFilter, Waveform, WavetableBuffer, ZzfxParams, ZzfxShape, ZzfxSynthesizer,
    poly_blep, process_shape_and_distort, spsc_ring_buffer, vowel_formant_frequencies,
};

const SR: f32 = 48_000.0;

fn make_engine(
    max_voices: usize,
    num_orbits: usize,
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
    let engine = SilverWaveAudioEngine::new(
        EngineConfig {
            max_voices,
            num_orbits,
            routing_mode,
            ..EngineConfig::default()
        },
        EngineChannels {
            event_rx,
            asset_rx,
            rms_tx,
        },
    );
    (engine, event_tx, asset_tx, rms_rx)
}

fn energy(buf: &[f32]) -> f32 {
    buf.iter().map(|x| x * x).sum()
}

fn render_one_block(engine: &mut SilverWaveAudioEngine) -> Vec<f32> {
    let mut out = vec![0.0f32; 512 * 2];
    engine.process_block(&mut out, 2);
    out
}

/// Pushes `ev` at the engine's current sample frame and renders `blocks`
/// stereo blocks, returning the total output energy.
fn render_event(
    engine: &mut SilverWaveAudioEngine,
    tx: &mut SpscProducer<TriggerEvent>,
    mut ev: TriggerEvent,
    blocks: usize,
) -> f32 {
    ev.trigger_sample = engine.current_sample_frame;
    tx.push(ev).unwrap();
    let mut total = 0.0;
    for _ in 0..blocks {
        total += energy(&render_one_block(engine));
    }
    total
}

fn set_slot(
    engine: &mut SilverWaveAudioEngine,
    idx: usize,
    state: VoiceState,
    orbit: u8,
    cut_group: i32,
    trigger_sample: u64,
    env_level: f32,
) {
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.orbit = orbit;
    ev.cut_group = cut_group;
    ev.trigger_sample = trigger_sample;
    let slot = &mut engine.voices[idx];
    slot.state = state;
    slot.event = ev;
    slot.current_env_level = env_level;
}

#[test]
fn spsc_ring_buffer_full_and_empty_behaviour() {
    let (mut tx, mut rx) = spsc_ring_buffer::<u8>(2);
    assert!(tx.push(1).is_ok());
    assert!(tx.push(2).is_ok());
    assert_eq!(tx.push(3), Err(PushError::Full(3)));
    assert_eq!(rx.pop(), Some(1));
    assert_eq!(rx.pop(), Some(2));
    assert_eq!(rx.pop(), None);
    assert!(tx.push(9).is_ok());
    assert_eq!(rx.pop(), Some(9));
}

#[test]
fn engine_synth_voice_kind_arms_render_audio() {
    let (mut engine, mut tx, _asset_tx, _rms_rx) =
        make_engine(16, 4, OutputRoutingMode::StereoMixdown);

    // Additive partials arm.
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.partial_count = 8;
    for (k, partial) in ev.partials.iter_mut().enumerate().take(8) {
        *partial = 1.0 / ((k + 1) as f32);
    }
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.1);

    // 2-operator FM compatibility arm (`fm_index` > 0, matrix inactive).
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.fm_index = 2.5;
    ev.fmh = 3.0;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.1);

    // 8x8 FM matrix arm (`fm_ops` level > 0).
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.fm_ops[0] = FmOpParams {
        ratio: 1.0,
        level: 1.0,
        feedback: 0.5,
        waveform: Waveform::Sine,
    };
    ev.fm_ops[2] = FmOpParams {
        ratio: 3.0,
        level: 0.0,
        feedback: 0.0,
        waveform: Waveform::Sine,
    };
    ev.fm_matrix[2][0] = 2.0; // src > dst reads `prev_outputs`
    ev.fm_matrix[0][1] = 1.5; // src < dst reads `current_out`
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.1);

    // Supersaw unison arm (stereo path keeps `sig_l`/`sig_r` direct).
    let mut ev = TriggerEvent::default_synth(Waveform::Supersaw, 110.0);
    ev.unison = 6;
    ev.detune = 0.6;
    ev.spread = 0.9;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.1);

    // Noise waveforms through the `Synth` voice kind.
    for wf in [
        Waveform::WhiteNoise,
        Waveform::PinkNoise,
        Waveform::BrownNoise,
        Waveform::Crackle,
    ] {
        let ev = TriggerEvent::default_synth(wf, 220.0);
        assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.001);
    }

    // Dedicated `Noise` voice kind.
    let mut ev = TriggerEvent::default_synth(Waveform::WhiteNoise, 220.0);
    ev.voice_kind = VoiceKind::Noise;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.001);

    // ZZFX voice kind.
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.voice_kind = VoiceKind::Zzfx;
    ev.zzfx = ZzfxParams {
        enabled: true,
        shape: ZzfxShape::Sawtooth,
        ..ZzfxParams::default()
    };
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.001);

    // Karplus-Strong plucked string.
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.voice_kind = VoiceKind::KarplusStrong;
    ev.karplus = KarplusStrongParams::default();
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.001);

    // Inline `>>` DSP pipe.
    let mut ev = TriggerEvent::default_synth(Waveform::Sawtooth, 220.0);
    ev.voice_kind = VoiceKind::InlinePipe;
    ev.lpf = 1200.0;
    ev.lpq = 1.0;
    ev.distort = 0.5;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.001);

    // Vibrato + pitch envelope instantaneous-frequency path.
    let mut ev = TriggerEvent::default_synth(Waveform::Sawtooth, 220.0);
    ev.vib = 5.0;
    ev.vibmod = 0.5;
    ev.penv = 4.0;
    ev.pattack = 0.001;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.1);
}

#[test]
fn engine_sample_and_granular_arms() {
    let (mut engine, mut tx, mut asset_tx, _rms_rx) =
        make_engine(8, 4, OutputRoutingMode::StereoMixdown);

    // Zero-output arms on a quiet engine: Granular/Sample without a loaded
    // buffer and SoundFont (which never builds a `SampleVoicePlayer`).
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.voice_kind = VoiceKind::Granular;
    ev.sample_handle = SampleId(999);
    ev.chop = 16;
    assert_eq!(render_event(&mut engine, &mut tx, ev, 1), 0.0);

    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.voice_kind = VoiceKind::SoundFont;
    assert_eq!(render_event(&mut engine, &mut tx, ev, 1), 0.0);

    let ev = TriggerEvent::default_sample(SampleId(700));
    assert_eq!(render_event(&mut engine, &mut tx, ev, 1), 0.0);

    // Asset ingestion: SampleLoaded / WavetableLoaded / ImpulseResponseLoaded /
    // EvictSample, including out-of-range id guards.
    let collector = DeferredCollector::<SampleBuffer>::new(8);
    let buf = collector.share(SampleBuffer::from_mono(
        (0..2048).map(|i| (i as f32 * 0.05).sin()).collect(),
        48_000,
    ));
    assert!(
        asset_tx
            .push(AssetMessage::SampleLoaded {
                id: SampleId(0),
                buffer: buf,
            })
            .is_ok()
    );
    let far = collector.share(SampleBuffer::from_mono(vec![0.1; 64], 48_000));
    assert!(
        asset_tx
            .push(AssetMessage::SampleLoaded {
                id: SampleId(5000),
                buffer: far,
            })
            .is_ok()
    );
    let evict_me = collector.share(SampleBuffer::from_mono(vec![0.2; 64], 48_000));
    assert!(
        asset_tx
            .push(AssetMessage::SampleLoaded {
                id: SampleId(1),
                buffer: evict_me,
            })
            .is_ok()
    );
    let wt_collector = DeferredCollector::<WavetableBuffer>::new(4);
    let wt = wt_collector.share(WavetableBuffer::new(vec![0.0, 1.0, 0.0, -1.0]));
    assert!(
        asset_tx
            .push(AssetMessage::WavetableLoaded {
                id: SampleId(1),
                buffer: wt,
            })
            .is_ok()
    );
    let ir_collector = DeferredCollector::<ImpulseResponseBuffer>::new(4);
    let ir = ir_collector.share(ImpulseResponseBuffer {
        ir_frames: vec![0.5; 256],
        sample_rate: 48_000,
    });
    assert!(
        asset_tx
            .push(AssetMessage::ImpulseResponseLoaded {
                orbit: 2,
                buffer: ir,
            })
            .is_ok()
    );
    assert!(
        asset_tx
            .push(AssetMessage::EvictSample { id: SampleId(9999) })
            .is_ok()
    );
    assert!(
        asset_tx
            .push(AssetMessage::EvictSample { id: SampleId(1) })
            .is_ok()
    );

    render_one_block(&mut engine);
    assert!(engine.samples[0].is_some());
    assert!(engine.samples[1].is_none()); // evicted
    assert!(engine.wavetables[1].is_some());

    // Granular with a loaded buffer.
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    ev.voice_kind = VoiceKind::Granular;
    ev.sample_handle = SampleId(0);
    ev.chop = 12;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.0);

    // `striate`-triggered granular construction on a synth voice.
    let mut ev = TriggerEvent::default_synth(Waveform::Sawtooth, 220.0);
    ev.striate = 10;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.0);

    // Sample playback consumes the 2048-frame buffer and retires the voice.
    // A dedicated quiet engine keeps this assertion isolated from the
    // long-lived granular/striate voices above.
    let (mut engine2, mut tx2, mut asset2, _r2) =
        make_engine(4, 2, OutputRoutingMode::StereoMixdown);
    let collector2 = DeferredCollector::<SampleBuffer>::new(4);
    let buf2 = collector2.share(SampleBuffer::from_mono(
        (0..2048).map(|i| (i as f32 * 0.05).sin()).collect(),
        48_000,
    ));
    assert!(
        asset2
            .push(AssetMessage::SampleLoaded {
                id: SampleId(0),
                buffer: buf2,
            })
            .is_ok()
    );
    let ev = TriggerEvent::default_sample(SampleId(0));
    let played = render_event(&mut engine2, &mut tx2, ev, 6);
    assert!(played > 0.0);
    let silent = render_one_block(&mut engine2);
    assert_eq!(energy(&silent), 0.0);
    assert!(!engine2.voices.iter().any(|v| v.state != VoiceState::Idle));

    // Wavetable kind loops the buffer and keeps producing audio.
    let mut ev = TriggerEvent::default_sample(SampleId(0));
    ev.voice_kind = VoiceKind::Wavetable;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.0);
}

#[test]
fn engine_filter_routing_and_serial_fx_chain() {
    let (mut engine, mut tx, _asset_tx, mut rms_rx) =
        make_engine(8, 4, OutputRoutingMode::StereoMixdown);
    engine.orbits[0].monitor.limiter_type = LimiterType::Bypass;

    let mut ev = TriggerEvent::default_synth(Waveform::Sawtooth, 220.0);
    ev.duration_samples = 96_000;
    // Filter envelopes: lpf / hpf / bpf with env offsets.
    ev.lpf = 900.0;
    ev.lpq = 1.1;
    ev.lpenv = 0.5;
    ev.hpf = 150.0;
    ev.hpq = 0.8;
    ev.hpenv = 0.3;
    ev.bpf = 1100.0;
    ev.bpq = 1.5;
    ev.bpenv = -0.2;
    ev.vowel = VowelFormant::O;
    // Ordered insert chain.
    let mut chain = OrderedVoiceFxChain::new();
    assert!(chain.push_stage(InsertFxStage::HighPass {
        cutoff_hz: 250.0,
        q: 0.7
    }));
    assert!(chain.push_stage(InsertFxStage::Distort {
        drive: 0.8,
        mode: DistortionMode::HardClip
    }));
    assert!(chain.push_stage(InsertFxStage::RingMod {
        freq_hz: 45.0,
        depth: 0.4
    }));
    ev.ordered_fx = chain;
    // Serial FX: crusher / shape / distort / ring / tremolo / compressor.
    ev.coarse = 3;
    ev.crush = 5.0;
    ev.shape = 0.4;
    ev.distort = 0.6;
    ev.ring_mod = RingModParams {
        ring_wet: 0.5,
        ring_freq_hz: 30.0,
        ring_slide_hz: 5.0,
    };
    ev.tremolo_rate = 6.0;
    ev.tremolo_depth = 0.5;
    ev.tremolo_shape = LfoShape::Tri;
    ev.tremolo_phase = 0.25;
    ev.compressor = CompressorParams {
        enabled: true,
        ..CompressorParams::default()
    };
    ev.phaser = 0.7;
    ev.phaser_depth = 0.6;
    ev.phaser_sweep = 700.0;
    ev.phaser_center = 900.0;
    ev.pan = 0.3;
    ev.postgain = 0.8;
    // Orbit-level FX params (delay lock / reverb / leslie / dry).
    ev.delay = 0.5;
    ev.delay_time = 0.02;
    ev.delay_feedback = 0.4;
    ev.delay_lock = true;
    ev.cps = 0.5;
    ev.room = 0.5;
    ev.size = 0.6;
    ev.dry = 0.9;
    ev.leslie = 0.4;
    ev.lrate = 5.0;
    ev.lsize = 0.4;
    assert!(render_event(&mut engine, &mut tx, ev, 2) > 0.0);

    // Orbit state was updated from the event.
    assert!(engine.orbits[0].delay_send > 0.0);
    assert!(engine.orbits[0].room_send > 0.0);
    assert!(engine.orbits[0].leslie_wet > 0.0);

    let mut reports = 0;
    while rms_rx.pop().is_some() {
        reports += 1;
    }
    assert!(reports > 0);
}

#[test]
fn engine_lookahead_duck_cutgroup_envdone() {
    let (mut engine, mut tx, _asset_tx, _rms_rx) =
        make_engine(8, 4, OutputRoutingMode::StereoMixdown);

    // Zero-frame block early return.
    engine.process_block(&mut [], 2);

    // Lookahead scheduling: event lands inside block [1024, 1536).
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 330.0);
    ev.trigger_sample = 1500;
    ev.duration_samples = 96_000;
    tx.push(ev).unwrap();
    assert_eq!(energy(&render_one_block(&mut engine)), 0.0); // stored, not started
    let _ = render_one_block(&mut engine); // [512, 1024): still future
    let b2 = render_one_block(&mut engine); // activates at offset 476
    assert!(energy(&b2) > 0.0);

    // `duckorbit` targets: valid orbit plus an out-of-range target.
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 440.0);
    ev.duck_targets[0] = 1;
    ev.duck_targets[1] = 250;
    ev.duck_count = 2;
    ev.duck_depth = 0.8;
    ev.duck_attack = 0.05;
    assert!(render_event(&mut engine, &mut tx, ev, 1) > 0.0);
    assert!(engine.orbits[1].duck_envelope.current_gain < 1.0);

    // Cut-group arbitration: an existing higher-priority voice survives the
    // same-timestamp retrigger (early return inside `activate_voice`).
    let mut a = TriggerEvent::default_synth(Waveform::Sine, 220.0);
    a.cut_group = 9;
    a.stream_priority = 5;
    a.hap_ordinal = 9;
    a.duration_samples = 96_000;
    let mut b = a;
    b.stream_priority = 1;
    b.hap_ordinal = 0;
    b.freq_hz = 440.0;
    let ts = engine.current_sample_frame;
    a.trigger_sample = ts;
    b.trigger_sample = ts;
    tx.push(a).unwrap();
    tx.push(b).unwrap();
    render_one_block(&mut engine);
    assert!(
        engine
            .voices
            .iter()
            .any(|v| v.event.cut_group == 9 && v.event.stream_priority == 5)
    );
    assert!(
        !engine
            .voices
            .iter()
            .any(|v| v.event.cut_group == 9 && v.event.stream_priority == 1)
    );

    // A later, higher-priority event chokes the cut-group voice.
    let mut c = a;
    c.stream_priority = 9;
    c.freq_hz = 550.0;
    c.trigger_sample = engine.current_sample_frame;
    tx.push(c).unwrap();
    render_one_block(&mut engine);
    assert!(engine.voices.iter().any(|v| v.event.cut_group == 9
        && v.state == VoiceState::Active
        && (v.event.freq_hz - 550.0).abs() < 0.5));

    // A very short note completes its envelope inside the block and retires.
    let mut ev = TriggerEvent::default_synth(Waveform::Sine, 1234.0);
    ev.duration_samples = 64;
    ev.attack = 0.0001;
    ev.decay = 0.0001;
    ev.sustain = 0.8;
    ev.release = 0.0001;
    let out_e = render_event(&mut engine, &mut tx, ev, 1);
    assert!(out_e > 0.0);
    assert!(
        !engine
            .voices
            .iter()
            .any(|v| (v.event.freq_hz - 1234.0).abs() < 0.5 && v.state != VoiceState::Idle)
    );
}

#[test]
fn engine_allocate_or_steal_three_tiers() {
    // Tier 1: Release / CutFadeOut victims sorted by envelope level.
    let (mut engine, _tx, _a, _r) = make_engine(4, 4, OutputRoutingMode::StereoMixdown);
    set_slot(&mut engine, 0, VoiceState::Release, 0, 0, 0, 0.9);
    set_slot(&mut engine, 1, VoiceState::Release, 1, 0, 100, 0.2);
    set_slot(&mut engine, 2, VoiceState::CutFadeOut, 2, 0, 50, 0.5);
    set_slot(&mut engine, 3, VoiceState::Active, 3, 0, 10, 0.05);
    let incoming = TriggerEvent::default_synth(Waveform::Sawtooth, 880.0);
    assert_eq!(engine.allocate_or_steal(&incoming), 1);
    assert_eq!(engine.steal_count, 1);
    // The steal crossfade buffer is mixed into orbit 0 on the next block.
    assert_eq!(engine.steal_fade.active_frames, 64);
    let mut buf = vec![0.0f32; 512 * 2];
    engine.process_block(&mut buf, 2);
    assert_eq!(engine.steal_fade.active_frames, 0);

    // Tier 2: same cut_group / orbit match, oldest `trigger_sample` wins.
    let (mut engine, _tx, _a, _r) = make_engine(4, 4, OutputRoutingMode::StereoMixdown);
    set_slot(&mut engine, 0, VoiceState::Active, 1, 5, 100, 0.5);
    set_slot(&mut engine, 1, VoiceState::Active, 0, 0, 50, 0.5);
    set_slot(&mut engine, 2, VoiceState::Active, 0, 0, 200, 0.9);
    set_slot(&mut engine, 3, VoiceState::Active, 3, 7, 10, 0.9);
    let mut incoming = TriggerEvent::default_synth(Waveform::Sawtooth, 880.0);
    incoming.orbit = 0;
    incoming.cut_group = 5;
    assert_eq!(engine.allocate_or_steal(&incoming), 1);
    assert_eq!(engine.steal_count, 1);
    let mut buf = vec![0.0f32; 512 * 2];
    engine.process_block(&mut buf, 2);

    // Tier 3: global quietest, ties broken by oldest `trigger_sample`.
    let (mut engine, _tx, _a, _r) = make_engine(4, 4, OutputRoutingMode::StereoMixdown);
    set_slot(&mut engine, 0, VoiceState::Active, 0, 0, 100, 0.5);
    set_slot(&mut engine, 1, VoiceState::Active, 1, 0, 80, 0.3);
    set_slot(&mut engine, 2, VoiceState::Active, 2, 0, 30, 0.3);
    set_slot(&mut engine, 3, VoiceState::Active, 3, 0, 200, 0.3);
    let mut incoming = TriggerEvent::default_synth(Waveform::Sawtooth, 880.0);
    incoming.orbit = 9;
    incoming.cut_group = 0;
    assert_eq!(engine.allocate_or_steal(&incoming), 2);
    assert_eq!(engine.steal_count, 1);
}

#[test]
fn engine_multichannel_orbit_routing() {
    let (mut engine, mut tx, _a, _r) = make_engine(4, 16, OutputRoutingMode::MultichannelOrbits);
    let mut ev = TriggerEvent::default_synth(Waveform::Sawtooth, 300.0);
    ev.orbit = 3;
    ev.trigger_sample = 0;
    ev.duration_samples = 96_000;
    tx.push(ev).unwrap();

    let mut out = vec![0.0f32; 256 * 8];
    engine.process_block(&mut out, 8);
    let mut orbit3_energy = 0.0f32;
    let mut other_energy = 0.0f32;
    for f in 0..256 {
        let base = f * 8;
        orbit3_energy += out[base + 6].abs() + out[base + 7].abs();
        for c in 0..6 {
            other_energy += out[base + c].abs();
        }
    }
    assert!(orbit3_energy > 0.01);
    assert_eq!(other_energy, 0.0);
}

#[test]
fn events_vowel_alias_parsing_and_fx_chain_bound() {
    let cases: &[(&str, VowelFormant)] = &[
        ("a", VowelFormant::A),
        ("E", VowelFormant::E),
        ("i", VowelFormant::I),
        ("o", VowelFormant::O),
        ("u", VowelFormant::U),
        ("ae", VowelFormant::Ae),
        ("Æ", VowelFormant::Ae),
        ("aa", VowelFormant::Aa),
        ("Å", VowelFormant::Aa),
        ("ɑ", VowelFormant::Aa),
        ("oe", VowelFormant::Oe),
        ("ø", VowelFormant::Oe),
        ("ö", VowelFormant::Oe),
        ("ue", VowelFormant::Ue),
        ("Ü", VowelFormant::Ue),
        ("y", VowelFormant::Y),
        ("ı", VowelFormant::Y),
        ("uh", VowelFormant::Uh),
        ("un", VowelFormant::Un),
        ("en", VowelFormant::En),
        ("an", VowelFormant::An),
        ("on", VowelFormant::On),
        ("zzz", VowelFormant::None),
        ("", VowelFormant::None),
        ("  a  ", VowelFormant::A),
    ];
    for (s, want) in cases {
        assert_eq!(VowelFormant::from_str_alias(s), *want, "input {s:?}");
    }

    // The ordered insert chain accepts at most 8 stages.
    let mut chain = OrderedVoiceFxChain::new();
    for _ in 0..8 {
        assert!(chain.push_stage(InsertFxStage::Bitcrush {
            bits: 4.0,
            downsample: 2.0
        }));
    }
    assert!(!chain.push_stage(InsertFxStage::Notch {
        cutoff_hz: 800.0,
        q: 1.0
    }));
    assert_eq!(chain.stage_count, 8);
}

#[test]
fn filters_adsr_regions_and_vowel_tables() {
    // Attack (4f) -> decay (4f) -> sustain until note_off (32f) -> release (4f).
    let mut env = AdsrEnvelope::new(0.0001, 0.0001, 0.5, 0.0001, 32);
    let mut saw_sustain = false;
    let mut saw_release_ramp = false;
    let mut saw_done = false;
    for i in 0..64 {
        let (v, done) = env.next_value(SR);
        if (8..32).contains(&i) && (v - 0.5).abs() < 1.0e-6 {
            saw_sustain = true;
        }
        if (33..36).contains(&i) && v < 0.5 {
            saw_release_ramp = true;
        }
        saw_done |= done;
    }
    assert!(saw_sustain);
    assert!(saw_release_ramp);
    assert!(saw_done);

    // Full vowel formant table, including `None`.
    let all = [
        VowelFormant::None,
        VowelFormant::A,
        VowelFormant::E,
        VowelFormant::I,
        VowelFormant::O,
        VowelFormant::U,
        VowelFormant::Ae,
        VowelFormant::Aa,
        VowelFormant::Oe,
        VowelFormant::Ue,
        VowelFormant::Y,
        VowelFormant::Uh,
        VowelFormant::Un,
        VowelFormant::En,
        VowelFormant::An,
        VowelFormant::On,
    ];
    for v in all {
        let freqs = vowel_formant_frequencies(v);
        assert!(freqs.iter().all(|f| *f > 0.0));
    }

    // `process_vowel` passthrough for `VowelFormant::None`.
    let mut vf = VowelFormantFilter::new();
    assert_eq!(vf.process_vowel(0.7, VowelFormant::None, SR), 0.7);
    for v in all.iter().copied().skip(1) {
        assert!(vf.process_vowel(0.5, v, SR).is_finite());
    }
}

#[test]
fn fx_units_distortion_tremolo_compressor_ordered_stages() {
    // `coarse` zero-order hold + `crush` quantizer.
    let mut bc = BitCrusherAndCoarse::default();
    for _ in 0..12 {
        assert!(bc.process(0.9, 4, 6.0).is_finite());
    }

    // Distortion modes not otherwise exercised.
    assert_eq!(
        process_shape_and_distort(0.5, 0.0, 1.0, DistortionMode::HardClip),
        1.0
    );
    let pos = process_shape_and_distort(0.5, 0.0, 1.0, DistortionMode::AsymmetricTube);
    let neg = process_shape_and_distort(-0.5, 0.0, 1.0, DistortionMode::AsymmetricTube);
    assert!(pos > 0.0 && neg < 0.0 && pos > neg.abs());

    // Tremolo LFO shapes + initial-phase initialization.
    let mut trem = TremoloModulator::default();
    for shape in [
        LfoShape::Sine,
        LfoShape::Tri,
        LfoShape::Saw,
        LfoShape::Square,
    ] {
        assert!(trem.process(0.5, 5.0, 0.8, shape, 0.25, SR).is_finite());
    }
    let mut trem_sq = TremoloModulator::default();
    let y = trem_sq.process(0.5, 5.0, 0.8, LfoShape::Square, 0.6, SR);
    assert!(y <= 0.5 + 1.0e-6); // phase >= 0.5 -> lfo = 0 -> attenuated
    let mut trem_off = TremoloModulator::default();
    assert_eq!(
        trem_off.process(0.5, 0.0, 0.8, LfoShape::Sine, 0.0, SR),
        0.5
    );

    // Compressor envelope attack + release branches and gain reduction.
    let mut comp = VoiceCompressor::default();
    let cp = CompressorParams {
        enabled: true,
        threshold_db: -20.0,
        ratio: 8.0,
        ..CompressorParams::default()
    };
    let mut last = 0.0f32;
    for _ in 0..2048 {
        last = comp.process(0.95, &cp, SR);
    }
    assert!(last < 0.95); // gain reduction above threshold
    assert!(comp.env_lin > 0.0);
    let rel = comp.process(0.000_001, &cp, SR);
    assert!(rel.is_finite());

    // Ordered executor covering every insert stage kind plus a `None` hole.
    let mut chain = OrderedVoiceFxChain::new();
    assert!(chain.push_stage(InsertFxStage::HighPass {
        cutoff_hz: 200.0,
        q: 0.8
    }));
    assert!(chain.push_stage(InsertFxStage::BandPass {
        cutoff_hz: 900.0,
        q: 1.0
    }));
    assert!(chain.push_stage(InsertFxStage::Notch {
        cutoff_hz: 1200.0,
        q: 1.0
    }));
    assert!(chain.push_stage(InsertFxStage::VowelFormant { morph: 2.5 }));
    assert!(chain.push_stage(InsertFxStage::Distort {
        drive: 0.5,
        mode: DistortionMode::WavefoldSaturate
    }));
    assert!(chain.push_stage(InsertFxStage::Bitcrush {
        bits: 6.0,
        downsample: 2.0
    }));
    assert!(chain.push_stage(InsertFxStage::RingMod {
        freq_hz: 55.0,
        depth: 0.5
    }));
    assert!(chain.push_stage(InsertFxStage::Compressor {
        threshold_db: -12.0,
        ratio: 3.0,
        attack_ms: 5.0,
        release_ms: 60.0
    }));
    assert_eq!(chain.stage_count, 8);
    chain.stages[0] = None; // exercise the `continue` on a missing stage
    let mut exec = OrderedVoiceFxExecutor::new();
    let y = exec.process(0.6, &chain, SR);
    assert!(y.is_finite());
    // Re-run so the HighPass slot is also visited with a real stage.
    chain.stages[0] = Some(InsertFxStage::HighPass {
        cutoff_hz: 200.0,
        q: 0.8,
    });
    assert!(exec.process(0.6, &chain, SR).is_finite());
}

#[test]
fn modulation_index_bus_table_and_evaluator() {
    // `ModulatableParam::from_index` full 0..=38 mapping plus `None`.
    for i in 0..ModulatableParam::COUNT {
        let p = ModulatableParam::from_index(i).expect("index in range");
        assert_eq!(p.index(), i);
    }
    assert_eq!(ModulatableParam::from_index(ModulatableParam::COUNT), None);
    assert_eq!(ModulatableParam::from_index(99), None);

    // `ControlSubBlockFrame::sample_at` degenerate length.
    let frame = ControlSubBlockFrame {
        sub_block_len: 1,
        start_value: 1.0,
        end_value: 3.0,
        ..ControlSubBlockFrame::default()
    };
    assert_eq!(frame.sample_at(0), 3.0);
    assert_eq!(frame.sample_at(7), 3.0);
    let ramp = ControlSubBlockFrame {
        sub_block_len: 64,
        start_value: 0.0,
        end_value: 1.0,
        ..ControlSubBlockFrame::default()
    };
    assert!((ramp.sample_at(32) - 0.5).abs() < 1.0e-6);

    // `ModulationBusTable` dirty-bit behaviour.
    let mut table = ModulationBusTable::default();
    assert!(table.apply_sub_block_ramp(ModulatableParam::Gain, 0.5, 0.5));
    assert!(table.is_dirty(ModulatableParam::Gain));
    // Same start but a changed end still marks the parameter dirty.
    assert!(table.apply_sub_block_ramp(ModulatableParam::Gain, 0.5, 0.9));
    // Identical ramp clears the dirty bit and returns `false`.
    assert!(!table.apply_sub_block_ramp(ModulatableParam::Gain, 0.5, 0.9));
    assert!(!table.is_dirty(ModulatableParam::Gain));
    assert_eq!(table.value_at(ModulatableParam::Gain, 4, 1), 0.9);

    // `PatternModulationEvaluator` early-outs and sub-block evaluation.
    let eval_empty = PatternModulationEvaluator::new();
    let mut frames = [ControlSubBlockFrame::default(); 8];
    assert_eq!(
        eval_empty.evaluate_modulation_sub_blocks(
            Rational64::zero(),
            Rational64::new_const(1, 8),
            0,
            128,
            &mut frames,
        ),
        0
    );

    let mut eval = PatternModulationEvaluator::new();
    let bus_a = ControlBusId::VoiceLocal {
        stream_id: 1,
        param: ModulatableParam::Pan,
    };
    let bus_b = ControlBusId::NamedSharedBus { bus_index: 3 };
    eval.register_lane(bus_a, ModulationRate::ControlRateSubBlock, sine());
    eval.register_lane(bus_b, ModulationRate::AudioRateSampleExact, sine());

    // `frames_in_block == 0` early-out.
    assert_eq!(
        eval.evaluate_modulation_sub_blocks(
            Rational64::zero(),
            Rational64::new_const(1, 8),
            0,
            0,
            &mut frames,
        ),
        0
    );
    // Empty `out_frames` early-out.
    let none_out: &mut [ControlSubBlockFrame] = &mut [];
    assert_eq!(
        eval.evaluate_modulation_sub_blocks(
            Rational64::zero(),
            Rational64::new_const(1, 8),
            0,
            128,
            none_out,
        ),
        0
    );
    // `out_frames` capacity stops mid-block.
    let mut one = [ControlSubBlockFrame::default(); 1];
    assert_eq!(
        eval.evaluate_modulation_sub_blocks(
            Rational64::zero(),
            Rational64::new_const(1, 8),
            0,
            128,
            &mut one,
        ),
        1
    );
    // 128 frames = 2 sub-blocks x 2 lanes = 4 frames written.
    let mut four = [ControlSubBlockFrame::default(); 4];
    let written = eval.evaluate_modulation_sub_blocks(
        Rational64::zero(),
        Rational64::new_const(1, 8),
        0,
        128,
        &mut four,
    );
    assert_eq!(written, 4);
    assert_eq!(four[0].sub_block_len, 64);
    assert_eq!(four[0].bus_id, bus_a);
    assert_eq!(four[1].bus_id, bus_b);

    // `CrossfaderSceneBank::default` uses `Self::new()`.
    let bank = CrossfaderSceneBank::default();
    assert_eq!(bank.xfader_position, 0.0);
    assert_eq!(bank.scene_a_id, 0);
}

#[test]
fn orbit_duck_delay_reverb_monitor_edges() {
    // Deeper duck holds; a shallower retrigger must not raise the floor.
    let mut duck = DuckSidechainState::default();
    duck.trigger_duck(0.9, 0.1, SR);
    assert!(duck.current_gain <= 0.1 + 1.0e-6);
    duck.trigger_duck(0.2, 0.1, SR);
    assert!(duck.current_gain <= 0.1 + 1.0e-6);
    assert!(duck.next_gain() < 1.0);

    // `OrbitDelay`: zero send early-out and the `lock` (tempo-synced) branch.
    let mut dly = OrbitDelay::new(4096);
    assert_eq!(
        dly.process_sample(0.5, 0.5, 0.0, 0.1, 0.3, false, 0.5, SR),
        (0.0, 0.0)
    );
    let (wl, wr) = dly.process_sample(0.5, 0.5, 0.6, 0.02, 0.5, true, 0.5, SR);
    assert!(wl.is_finite() && wr.is_finite());

    // `OrbitReverb`: zero send early-out and IR crossfade countdown.
    let mut rev = OrbitReverb::new(7);
    assert_eq!(rev.process_sample(0.4, 0.4, 0.0, 0.5, 0.3), (0.0, 0.0));
    rev.swap_impulse_response(Some(3), SR);
    let mut reverb_energy = 0.0f32;
    for _ in 0..4000 {
        let (l, r) = rev.process_sample(0.4, -0.3, 0.7, 0.8, 0.3);
        reverb_energy += l * l + r * r;
    }
    assert!(reverb_energy > 0.0);

    // `OrbitMonitor`: Bypass limiter and empty `take_rms_and_peak`.
    let mut mon = OrbitMonitor {
        limiter_type: LimiterType::Bypass,
        ..OrbitMonitor::default()
    };
    assert_eq!(mon.process_frame(3.0, -3.0), (3.0, -3.0));
    let mut fresh = OrbitMonitor::default();
    assert_eq!(fresh.take_rms_and_peak(), (0.0, 0.0, 0.0));
}

#[test]
fn samples_shared_asset_loader_player_granular() {
    // `SharedAsset` clone + deferred reclamation through the graveyard.
    let collector = DeferredCollector::<SampleBuffer>::new(4);
    let shared = collector.share(SampleBuffer::from_mono(vec![0.25; 128], 48_000));
    let clone: SharedAsset<SampleBuffer> = shared.clone();
    assert_eq!(clone.frame_count(), 128);
    drop(shared);
    assert_eq!(collector.pending_count(), 0); // still shared by `clone`
    drop(clone);
    assert_eq!(collector.pending_count(), 1);
    assert_eq!(collector.collect(), 1);
    assert_eq!(collector.collect(), 0);

    // A poisoned graveyard mutex makes `collect` fall back to `0`.
    struct PanicOnDrop;
    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!("intentional drop panic to poison the graveyard");
        }
    }
    let poisoned = DeferredCollector::<PanicOnDrop>::new(2);
    let doomed = poisoned.share(PanicOnDrop);
    drop(doomed);
    let caught = catch_unwind(AssertUnwindSafe(|| poisoned.collect()));
    assert!(caught.is_err());
    assert_eq!(poisoned.collect(), 0);
    assert_eq!(poisoned.pending_count(), 0);

    // `SampleBuffer` degenerate shapes.
    let ch_zero = SampleBuffer {
        frames: vec![0.5; 8],
        channels: 0,
        sample_rate: 48_000,
        is_mmap_streamed: false,
    };
    assert_eq!(ch_zero.frame_count(), 0);
    let empty = SampleBuffer::from_mono(vec![], 48_000);
    assert_eq!(empty.frame_count(), 0);
    assert_eq!(empty.read_hermite_mono(2.5), 0.0);

    // `SampleSource` parsing and manifest URL resolution.
    assert!(SampleSource::parse_github_shorthand("github:").is_none());
    assert!(SampleSource::parse_github_shorthand("https://x").is_none());
    let gh = SampleSource::parse_github_shorthand("github:alice").unwrap();
    match &gh {
        SampleSource::GithubRepo { user, repo, branch } => {
            assert_eq!(user, "alice");
            assert_eq!(repo, "samples");
            assert_eq!(branch, "main");
        }
        _ => panic!("expected GithubRepo"),
    }
    let url_src = SampleSource::StrudelJsonUrl {
        url: "https://example.org/m.json".to_string(),
    };
    assert_eq!(
        url_src.canonical_manifest_url().as_deref(),
        Some("https://example.org/m.json")
    );

    // `LazySampleLoader` interning + empty-bank name formatting.
    let mut loader = LazySampleLoader::new();
    let id1 = loader.resolve_or_enqueue("bank", "sd", 0);
    let id2 = loader.resolve_or_enqueue("bank", "sd", 0);
    assert_eq!(id1, id2);
    assert_eq!(loader.fetch_count(), 1);
    let id3 = loader.resolve_or_enqueue("", "hh", 2);
    assert_ne!(id3, id1);
    assert!(
        loader
            .enqueued_fetches
            .iter()
            .any(|(_, u)| u.ends_with("hh/2.wav"))
    );

    // `SampleVoicePlayer::from_trigger` manipulation paths.
    let mut base = TriggerEvent::default_sample(SampleId(0));
    base.duration_samples = 1024;

    let mut ev = base;
    ev.slice_count = 4;
    ev.slice_idx = 1;
    let p = SampleVoicePlayer::from_trigger(&ev, 4096, 48_000, SR, false);
    assert_eq!(p.start_frame, 1024.0);
    assert_eq!(p.end_frame, 2048.0);

    let mut ev = base;
    ev.splice_mode = true;
    let p = SampleVoicePlayer::from_trigger(&ev, 4096, 48_000, SR, false);
    assert!((p.step_per_sample - 4.0).abs() < 1.0e-4);

    let mut ev = base;
    ev.fit = 0.5;
    let p = SampleVoicePlayer::from_trigger(&ev, 4096, 48_000, SR, false);
    assert!((p.step_per_sample - 2.0).abs() < 1.0e-4);

    for mode in [LoopMode::On, LoopMode::WavetableDefault] {
        let mut ev = base;
        ev.loop_mode = mode;
        assert!(SampleVoicePlayer::from_trigger(&ev, 4096, 48_000, SR, false).loop_enabled);
    }

    let mut ev = base;
    ev.scrub = 0.5;
    let p = SampleVoicePlayer::from_trigger(&ev, 4096, 48_000, SR, false);
    assert!((p.playhead_frame - 2048.0).abs() < 1.0);

    let mut ev = base;
    ev.speed = -1.0;
    let p = SampleVoicePlayer::from_trigger(&ev, 4096, 48_000, SR, false);
    assert!(p.step_per_sample < 0.0);
    assert!((p.playhead_frame - 4095.0).abs() < 1.0);

    // `next_sample` edges: forward finish, forward loop wrap, reverse finish,
    // reverse loop wrap, and the `finished` early-out.
    let small = SampleBuffer::from_mono((0..64).map(|i| (i as f32) / 64.0).collect(), 48_000);

    let mut ev = base;
    ev.loop_mode = LoopMode::Off;
    let mut p = SampleVoicePlayer::from_trigger(&ev, 64, 48_000, SR, false);
    for _ in 0..64 {
        assert!(p.next_sample(&small).is_finite());
    }
    assert!(p.finished);
    assert_eq!(p.next_sample(&small), 0.0);

    let mut ev = base;
    ev.loop_mode = LoopMode::On;
    let mut pl = SampleVoicePlayer::from_trigger(&ev, 64, 48_000, SR, false);
    for _ in 0..200 {
        assert!(pl.next_sample(&small).is_finite());
    }
    assert!(!pl.finished);

    let mut ev = base;
    ev.speed = -1.0;
    ev.loop_mode = LoopMode::Off;
    let mut pr = SampleVoicePlayer::from_trigger(&ev, 64, 48_000, SR, false);
    for _ in 0..64 {
        pr.next_sample(&small);
    }
    assert!(pr.finished);

    let mut ev = base;
    ev.speed = -1.0;
    ev.loop_mode = LoopMode::On;
    let mut prl = SampleVoicePlayer::from_trigger(&ev, 64, 48_000, SR, false);
    for _ in 0..200 {
        assert!(prl.next_sample(&small).is_finite());
    }
    assert!(!prl.finished);

    // `GranularSamplerVoice` degenerate constructors and exhausted grains.
    let g = GranularSamplerVoice::new(0, 0);
    assert_eq!(g.grain_count, 1);
    assert_eq!(g.total_duration_frames, 4800);
    let mut g_empty = GranularSamplerVoice::new(4, 64);
    assert_eq!(g_empty.next_sample(&empty), 0.0);
    let mut g2 = GranularSamplerVoice::new(4, 16);
    let mut rendered = 0.0f32;
    for _ in 0..32 {
        rendered += g2.next_sample(&small).abs();
    }
    assert!(rendered >= 0.0);
    assert_eq!(g2.next_sample(&small), 0.0); // past total_duration_frames
}

#[test]
fn synths_polyblep_noise_fm_zzfx_karplus_edges() {
    // `poly_blep` degenerate `dt`.
    assert_eq!(poly_blep(0.5, 0.0), 0.0);
    assert_eq!(poly_blep(0.5, -1.0), 0.0);

    // `PolyBlepOscillator` fallback arm for non-basic waveforms.
    let mut osc = PolyBlepOscillator::new(0.0);
    let y = osc.next_sample(Waveform::Supersaw, 440.0, SR);
    assert!(y.is_finite());

    // `NoiseGenerator` fallback arm.
    let mut ng = NoiseGenerator::new(123);
    assert!(ng.next_sample(Waveform::Sawtooth).is_finite());

    // FM matrix: src>dst uses `prev_outputs`, op `feedback` adds phase mod.
    let mut fm = FmMatrixSynthesizer::default();
    let mut ops = [FmOpParams::default(); 8];
    ops[0] = FmOpParams {
        ratio: 1.0,
        level: 1.0,
        feedback: 0.5,
        waveform: Waveform::Sine,
    };
    ops[2] = FmOpParams {
        ratio: 3.0,
        level: 0.0,
        feedback: 0.0,
        waveform: Waveform::Sine,
    };
    let mut matrix = [[0.0f32; 8]; 8];
    matrix[2][0] = 2.0;
    matrix[0][1] = 1.5;
    let mut fm_energy = 0.0f32;
    for _ in 0..256 {
        fm_energy += fm
            .next_sample(220.0, SR, 0.0, 1.0, 1.0, &ops, &matrix)
            .abs();
    }
    assert!(fm_energy > 0.0);

    // ZZFX: every shape, all four envelope regions, pitch jump, and tremolo.
    for shape in [
        ZzfxShape::Sine,
        ZzfxShape::Triangle,
        ZzfxShape::Sawtooth,
        ZzfxShape::Tan,
        ZzfxShape::Noise,
        ZzfxShape::Square,
    ] {
        let params = ZzfxParams {
            enabled: true,
            shape,
            attack: 0.005,
            decay: 0.01,
            sustain_time: 0.02,
            release: 0.01,
            pitch_jump: 40.0,
            pitch_jump_time: 0.001,
            tremolo: 0.5,
            lfo: 0.01,
            ..ZzfxParams::default()
        };
        let mut z = ZzfxSynthesizer::default();
        let mut sum = 0.0f32;
        for _ in 0..2400 {
            sum += z.next_sample(&params, SR).abs();
        }
        assert!(sum > 0.0, "shape {shape:?} produced silence");
    }

    // `KarplusStrongPluck::default` delegates to `new`, and the first
    // `next_sample` excites the delay line.
    let mut ks = KarplusStrongPluck::default();
    let kp = KarplusStrongParams::default();
    let mut ks_energy = 0.0f32;
    for _ in 0..64 {
        ks_energy += ks.next_sample(&kp, SR).abs();
    }
    assert!(ks_energy > 0.0);
}

#[test]
fn telemetry_analyzer_edge_cases() {
    // Empty input -> degenerate statistics.
    let empty = AcousticTelemetryAnalyzer::analyze_orbits(&[], SR);
    assert!(empty.integrated_lufs.is_finite());
    assert!(empty.masking_conflicts.is_empty());
    assert_eq!(empty.stereo_phase_correlation, 1.0);

    // Anti-correlated orbits produce a sub-bass cancellation alert; a
    // zero-length orbit exercises the `n == 0` skip.
    let n = 480usize;
    let a: Vec<f32> = (0..n).map(|i| (i as f32 * 0.3).sin() * 0.7).collect();
    let b: Vec<f32> = a.iter().map(|x| -x).collect();
    let e: Vec<f32> = Vec::new();
    let diag = AcousticTelemetryAnalyzer::analyze_orbits(
        &[
            (0, a.as_slice(), a.as_slice()),
            (1, b.as_slice(), b.as_slice()),
            (2, e.as_slice(), e.as_slice()),
        ],
        SR,
    );
    assert!(
        diag.masking_conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::SubBassPhaseCancellation
                && c.orbit_a == 0
                && c.orbit_b == 1)
    );

    // Signal above 0 dBTP -> true-peak clipping risk alert.
    let loud = vec![2.0f32; 64];
    let diag2 =
        AcousticTelemetryAnalyzer::analyze_orbits(&[(0, loud.as_slice(), loud.as_slice())], SR);
    assert!(
        diag2
            .masking_conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::TruePeakClippingRisk)
    );
    assert!(diag2.true_peak_dbtp > 0.0);
}
