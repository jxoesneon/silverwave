//! Native CLAP (`org.silverwave.live`) & VST3 (`SilverWave.vst3`) Plugin Descriptor,
//! 16-Stereo-Bus (32-Channel) Orbit Audio I/O Layout, and Bundle Manifest Generator
//! (`00-MASTER-ROADMAP` §2 `v0.6.0`, `04-SPEC` §2.4 & §8.5, `08-SPEC` §8.2–8.3, `PP-12`).

#![forbid(unsafe_code)]

/// Canonical CLAP plugin identifier (`08-SPEC` §8.2.3).
pub const CLAP_PLUGIN_ID: &str = "org.silverwave.live";

/// Canonical 16-byte VST3 Class UID for `SilverWave` (`08-SPEC` §8.2.3).
pub const VST3_CLASS_UID: [u8; 16] = *b"SilverWaveLive01";

/// Number of stereo orbit output buses exposed to the DAW mixer (`16` stereo = `32` channels — `PP-12`).
pub const NUM_PLUGIN_STEREO_ORBIT_BUSES: usize = 16;

/// Total output audio channels across all 16 stereo orbit buses (`32` channels — `PP-12`).
pub const TOTAL_PLUGIN_OUTPUT_CHANNELS: usize = NUM_PLUGIN_STEREO_ORBIT_BUSES * 2;

/// Number of DAW-automatable macro slider parameters (`slider(0..63)` — `08-SPEC` §8.2.3).
pub const NUM_DAW_MACRO_PARAMS: usize = 64;

/// Native CLAP / VST3 DAW plugin descriptor contract (`08-SPEC` §8.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DawPluginConfig {
    pub plugin_id: &'static str,
    pub stereo_orbit_buses: u8,
    pub exposed_macro_params: u8,
    pub sync_to_host_transport: bool,
    pub emit_sample_accurate_midi: bool,
    pub enable_mcp_and_lsp_bridge: bool,
}

impl Default for DawPluginConfig {
    fn default() -> Self {
        Self {
            plugin_id: CLAP_PLUGIN_ID,
            stereo_orbit_buses: NUM_PLUGIN_STEREO_ORBIT_BUSES as u8,
            exposed_macro_params: NUM_DAW_MACRO_PARAMS as u8,
            sync_to_host_transport: true,
            emit_sample_accurate_midi: true,
            enable_mcp_and_lsp_bridge: true,
        }
    }
}

/// Bus role inside the DAW mixer (`Main` for Orbit 0, `Aux` for Orbits 1..15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrbitBusRole {
    MainStereo,
    AuxiliaryStereo,
}

/// Descriptor for a single stereo output bus (`Orbit 0..15`) exposed to the DAW host (`08-SPEC` §8.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StereoOrbitBusDescriptor {
    pub orbit_index: u8,
    pub name: String,
    pub role: OrbitBusRole,
    pub left_channel_index: usize,
    pub right_channel_index: usize,
}

/// Complete 16-Stereo-Bus (32-Channel) + Stereo Sidechain Input layout for CLAP & VST3 (`PP-12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiBusAudioLayout {
    pub has_stereo_sidechain_input: bool,
    pub output_buses: Vec<StereoOrbitBusDescriptor>,
}

impl Default for MultiBusAudioLayout {
    fn default() -> Self {
        Self::new_16_orbit_layout()
    }
}

impl MultiBusAudioLayout {
    /// Constructs the canonical 16-stereo-bus (32-channel) layout (`Orbit 0 Main` + `Orbit 1..15 Aux`).
    pub fn new_16_orbit_layout() -> Self {
        let mut buses = Vec::with_capacity(NUM_PLUGIN_STEREO_ORBIT_BUSES);
        for idx in 0..NUM_PLUGIN_STEREO_ORBIT_BUSES {
            let role = if idx == 0 {
                OrbitBusRole::MainStereo
            } else {
                OrbitBusRole::AuxiliaryStereo
            };
            let name = if idx == 0 {
                "Orbit 0 (Main)".to_string()
            } else {
                format!("Orbit {} (Aux)", idx)
            };
            buses.push(StereoOrbitBusDescriptor {
                orbit_index: idx as u8,
                name,
                role,
                left_channel_index: idx * 2,
                right_channel_index: idx * 2 + 1,
            });
        }
        Self {
            has_stereo_sidechain_input: true,
            output_buses: buses,
        }
    }

    /// Total number of output audio channels (`32`).
    pub fn total_output_channels(&self) -> usize {
        self.output_buses.len() * 2
    }

    /// Looks up the stereo channel indices `(left, right)` for `orbit_index` (`0..16`).
    pub fn channel_pair_for_orbit(&self, orbit_index: usize) -> Option<(usize, usize)> {
        let bus = self.output_buses.get(orbit_index)?;
        Some((bus.left_channel_index, bus.right_channel_index))
    }
}

/// Plugin binary format (`CLAP` vs `VST3` — `00-MASTER-ROADMAP` §2 `v0.6.0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginFormat {
    Clap,
    Vst3,
}

/// Metadata manifest for `clap-validator` and `pluginval` inspection (`00-MASTER-ROADMAP` §4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginBundleManifest {
    pub format: PluginFormat,
    pub bundle_name: String,
    pub plugin_id: String,
    pub vendor: String,
    pub version: String,
    pub features: Vec<String>,
}

impl PluginBundleManifest {
    /// Generates the canonical CLAP or VST3 bundle manifest.
    pub fn for_format(format: PluginFormat) -> Self {
        let bundle_name = match format {
            PluginFormat::Clap => "SilverWave.clap".to_string(),
            PluginFormat::Vst3 => "SilverWave.vst3".to_string(),
        };
        Self {
            format,
            bundle_name,
            plugin_id: CLAP_PLUGIN_ID.to_string(),
            vendor: "SilverWave Audio".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            features: vec![
                "instrument".to_string(),
                "audio-effect".to_string(),
                "note-effect".to_string(),
                "stereo".to_string(),
                "multi-output-32ch".to_string(),
                "polyphonic-modulation".to_string(),
            ],
        }
    }
}
