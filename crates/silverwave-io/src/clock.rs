//! Phase-Continuous Clock, Tempo & Ableton Link Multi-Peer Synchronization
//! (`04-SPEC` Section 5, `LNK-01`..`LNK-03`).

use crate::types::{CycleTime, IoError};
use std::sync::{Arc, Mutex};

/// Clock synchronization source (`Section 5.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockSource {
    Monotonic,
    AbletonLink,
    AudioSampleClock,
    DawHostClock,
}

/// Instantaneous clock & Link session snapshot (`Section 5.2`).
#[derive(Clone, Debug, PartialEq)]
pub struct ClockSnapshot {
    pub wall_secs: f64,
    pub cycle: f64,
    pub cps: f64,
    pub bpm: f64,
    pub beats_per_cycle: f64,
    pub quantum: f64,
    pub link_peers: usize,
    pub is_playing: bool,
}

/// Core clock & Ableton Link engine trait (`Section 5.2`, `LNK-01`..`LNK-03`).
pub trait ClockEngine: Send + Sync {
    fn source(&self) -> ClockSource;
    fn snapshot(&self) -> ClockSnapshot;
    fn cycle_to_wall_secs(&self, cycle: CycleTime) -> f64;
    fn wall_secs_to_cycle(&self, wall_secs: f64) -> f64;
    fn set_cps(&self, cps: f64) -> Result<(), IoError>;
    fn set_bpm(&self, bpm: f64) -> Result<(), IoError>;
    fn set_beats_per_cycle(&self, bpc: f64) -> Result<(), IoError>;
    fn nudge_secs(&self, delta_secs: f64);
    fn enable_link(&self, enabled: bool) -> Result<(), IoError>;
    fn enable_start_stop_sync(&self, enabled: bool) -> Result<(), IoError>;
    fn link_num_peers(&self) -> usize;
    fn set_playing(&self, playing: bool);
}

/// Shared LAN/in-process Ableton Link session mesh for multi-peer phase-lock (`LNK-02`, `LNK-03`).
#[derive(Default)]
pub struct LinkSessionMesh {
    inner: Mutex<LinkMeshState>,
}

struct LinkMeshState {
    origin_secs: f64,
    origin_cycle: f64,
    cps: f64,
    is_playing: bool,
    peer_count: usize,
}

impl Default for LinkMeshState {
    fn default() -> Self {
        Self {
            origin_secs: 0.0,
            origin_cycle: 0.0,
            cps: 0.5,
            is_playing: true,
            peer_count: 0,
        }
    }
}

impl LinkSessionMesh {
    /// Creates a new shared `LinkSessionMesh`.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

struct ClockInternal {
    source: ClockSource,
    now_secs: f64,
    origin_secs: f64,
    origin_cycle: f64,
    cps: f64,
    beats_per_cycle: f64,
    link_enabled: bool,
    start_stop_sync: bool,
    is_playing: bool,
    mesh: Option<Arc<LinkSessionMesh>>,
}

/// Phase-continuous `LinkClock` implementing `ClockEngine` (`LNK-01`, `LNK-02`, `LNK-03`).
pub struct LinkClock {
    state: Mutex<ClockInternal>,
}

impl Default for LinkClock {
    fn default() -> Self {
        Self::new(0.5, 4.0)
    }
}

impl LinkClock {
    /// Creates a new standalone `LinkClock` with `cps` and `beats_per_cycle`.
    pub fn new(cps: f64, beats_per_cycle: f64) -> Self {
        Self {
            state: Mutex::new(ClockInternal {
                source: ClockSource::Monotonic,
                now_secs: 0.0,
                origin_secs: 0.0,
                origin_cycle: 0.0,
                cps: cps.max(0.001),
                beats_per_cycle: beats_per_cycle.max(1.0),
                link_enabled: false,
                start_stop_sync: false,
                is_playing: true,
                mesh: None,
            }),
        }
    }

    /// Joins a shared `LinkSessionMesh` (`LNK-02`, `LNK-03`).
    pub fn join_mesh(mesh: Arc<LinkSessionMesh>, initial_cps: f64) -> Self {
        {
            // `inner` is private and no user code runs under the guard, so
            // the mesh mutex can never be poisoned.
            let mut m = mesh.inner.lock().unwrap();
            m.peer_count += 1;
            m.cps = initial_cps.max(0.001);
        }
        Self {
            state: Mutex::new(ClockInternal {
                source: ClockSource::AbletonLink,
                now_secs: 0.0,
                origin_secs: 0.0,
                origin_cycle: 0.0,
                cps: initial_cps.max(0.001),
                beats_per_cycle: 4.0,
                link_enabled: true,
                start_stop_sync: true,
                is_playing: true,
                mesh: Some(mesh),
            }),
        }
    }

    /// Advances the clock's reference wall-clock timestamp `now_secs` (`LNK-01`, `LNK-02`).
    pub fn set_wall_time_secs(&self, now_secs: f64) {
        // `state`/`inner` are private and no user code runs under either
        // guard, so neither mutex can ever be poisoned.
        let mut g = self.state.lock().unwrap();
        g.now_secs = now_secs;
        if g.link_enabled
            && let Some(mesh) = g.mesh.clone()
        {
            let m = mesh.inner.lock().unwrap();
            g.origin_secs = m.origin_secs;
            g.origin_cycle = m.origin_cycle;
            g.cps = m.cps;
            if g.start_stop_sync {
                g.is_playing = m.is_playing;
            }
        }
    }
}

impl ClockEngine for LinkClock {
    fn source(&self) -> ClockSource {
        self.state
            .lock()
            .map(|g| g.source)
            .unwrap_or(ClockSource::Monotonic)
    }

    fn snapshot(&self) -> ClockSnapshot {
        let g = self.state.lock().unwrap();
        let (cps, orig_s, orig_c, playing, peers) = if g.link_enabled
            && let Some(ref mesh) = g.mesh
        {
            // `inner` is private and no user code runs under the guard, so
            // the mesh mutex can never be poisoned.
            let m = mesh.inner.lock().unwrap();
            let pl = if g.start_stop_sync {
                m.is_playing
            } else {
                g.is_playing
            };
            (
                m.cps,
                m.origin_secs,
                m.origin_cycle,
                pl,
                m.peer_count.saturating_sub(1),
            )
        } else {
            (g.cps, g.origin_secs, g.origin_cycle, g.is_playing, 0)
        };
        let cycle = orig_c + (g.now_secs - orig_s) * cps;
        ClockSnapshot {
            wall_secs: g.now_secs,
            cycle,
            cps,
            bpm: cps * 60.0 * g.beats_per_cycle,
            beats_per_cycle: g.beats_per_cycle,
            quantum: g.beats_per_cycle,
            link_peers: peers,
            is_playing: playing,
        }
    }

    fn cycle_to_wall_secs(&self, cycle: CycleTime) -> f64 {
        let g = self.state.lock().unwrap();
        let c_f64 = cycle.to_f64();
        g.origin_secs + (c_f64 - g.origin_cycle) / g.cps.max(1.0e-9)
    }

    fn wall_secs_to_cycle(&self, wall_secs: f64) -> f64 {
        let g = self.state.lock().unwrap();
        if g.link_enabled
            && let Some(ref mesh) = g.mesh
        {
            // `inner` is private and no user code runs under the guard, so
            // the mesh mutex can never be poisoned.
            let m = mesh.inner.lock().unwrap();
            return m.origin_cycle + (wall_secs - m.origin_secs) * m.cps;
        }
        g.origin_cycle + (wall_secs - g.origin_secs) * g.cps
    }

    fn set_cps(&self, cps: f64) -> Result<(), IoError> {
        if cps <= 0.0 || !cps.is_finite() {
            return Err(IoError::Config(
                "cps must be positive and finite".to_string(),
            ));
        }
        // `state`/`inner` are private and no user code runs under either
        // guard, so neither mutex can ever be poisoned.
        let mut g = self.state.lock().unwrap();
        // Preserve exact phase continuity at `g.now_secs` (`LNK-01`)
        let cur_cycle = g.origin_cycle + (g.now_secs - g.origin_secs) * g.cps;
        g.origin_secs = g.now_secs;
        g.origin_cycle = cur_cycle;
        g.cps = cps;
        if g.link_enabled
            && let Some(ref mesh) = g.mesh
        {
            let mut m = mesh.inner.lock().unwrap();
            m.origin_secs = g.origin_secs;
            m.origin_cycle = g.origin_cycle;
            m.cps = cps;
        }
        Ok(())
    }

    fn set_bpm(&self, bpm: f64) -> Result<(), IoError> {
        let bpc = self.state.lock().map(|g| g.beats_per_cycle).unwrap_or(4.0);
        self.set_cps(bpm / 60.0 / bpc)
    }

    fn set_beats_per_cycle(&self, bpc: f64) -> Result<(), IoError> {
        if bpc <= 0.0 {
            return Err(IoError::Config("beats_per_cycle must be > 0".to_string()));
        }
        // `state` is private and no user code runs under the guard, so the
        // mutex can never be poisoned.
        self.state.lock().unwrap().beats_per_cycle = bpc;
        Ok(())
    }

    fn nudge_secs(&self, delta_secs: f64) {
        // `state` is private and no user code runs under the guard, so the
        // mutex can never be poisoned.
        self.state.lock().unwrap().origin_secs += delta_secs;
    }

    fn enable_link(&self, enabled: bool) -> Result<(), IoError> {
        // `state` is private and no user code runs under the guard, so the
        // mutex can never be poisoned.
        let mut g = self.state.lock().unwrap();
        g.link_enabled = enabled;
        g.source = if enabled {
            ClockSource::AbletonLink
        } else {
            ClockSource::Monotonic
        };
        Ok(())
    }

    fn enable_start_stop_sync(&self, enabled: bool) -> Result<(), IoError> {
        // `state` is private and no user code runs under the guard, so the
        // mutex can never be poisoned.
        self.state.lock().unwrap().start_stop_sync = enabled;
        Ok(())
    }

    fn link_num_peers(&self) -> usize {
        self.snapshot().link_peers
    }

    fn set_playing(&self, playing: bool) {
        // `state`/`inner` are private and no user code runs under either
        // guard, so neither mutex can ever be poisoned.
        let mut g = self.state.lock().unwrap();
        g.is_playing = playing;
        if g.link_enabled
            && g.start_stop_sync
            && let Some(ref mesh) = g.mesh
        {
            let mut m = mesh.inner.lock().unwrap();
            m.is_playing = playing;
        }
    }
}
