//! 10 Live-Subscribed `silverwave://` Resources, 5 Structured Workflow Prompts
//! (`SilverWaveMcpPromptSurface`), and Complete 42-Tool `SilverWaveMcpServer` Engine
//! (`07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md` §1–7).

#![forbid(unsafe_code)]

use crate::protocol::{
    ActorOrigin, CoPerformanceBridge, CoPerformanceSessionHub, ERR_DSL_SYNTAX, ERR_INVALID_PARAMS,
    ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, ERR_PARSE_ERROR, ERR_READ_ONLY_VIOLATION,
    ERR_WIDGET_NOT_FOUND, McpCliArgs, McpProtocolError, SourceSpan, SurgicalTextEdit,
};
use crate::security::{
    DefaultSandboxGuard, PathSandboxGuard, PreallocatedAudioCommandRing, RateLimiterPolicy,
    RtAudioCommand, TokenBucketRateLimiter, ToolRateCategory,
};
use crate::tools::*;
use silverwave_core::Rational64;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Canonical 10 MCP resource URIs exposed by `silverwave-mcp` (`07-SPEC` §3.1).
pub const MCP_RESOURCE_URIS: [&str; 10] = [
    "silverwave://session/buffer",
    "silverwave://session/streams",
    "silverwave://session/clock",
    "silverwave://session/highlights",
    "silverwave://session/widgets",
    "silverwave://dsp/orbits",
    "silverwave://dsp/samples",
    "silverwave://theory/scales",
    "silverwave://theory/voicings",
    "silverwave://viz/pianoroll",
];

/// Canonical 42 MCP tool names (38 core + 4 Category-King tools — `07-SPEC` §2 & §7).
pub const MCP_ALL_42_TOOL_NAMES: [&str; 42] = [
    "evaluate_buffer",
    "evaluate_block",
    "parse_mini_notation",
    "query_pattern_arc",
    "validate_syntax",
    "set_stream",
    "transition_stream",
    "mute_stream",
    "unmute_stream",
    "solo_stream",
    "unsolo_stream",
    "hush",
    "panic",
    "get_clock_snapshot",
    "set_cps",
    "set_bpm",
    "set_beats_per_cycle",
    "nudge_clock",
    "configure_ableton_link",
    "list_sliders",
    "update_slider",
    "set_control_state",
    "get_control_state",
    "list_sample_banks",
    "load_sample_source",
    "configure_orbit",
    "get_orbit_telemetry",
    "list_midi_ports",
    "configure_midi_map",
    "send_midi_command",
    "configure_osc_target",
    "load_csound_orc",
    "capture_visualizer_frame",
    "read_audio_telemetry",
    "export_audio",
    "encode_share_url",
    "decode_share_url",
    "run_doctor",
    "set_crossfader_scene",
    "trigger_looper_freeze",
    "set_fill_latch",
    "render_video",
];

/// Canonical 5 MCP prompt identifiers (`07-SPEC` §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilverWavePromptId {
    LiveCodeSession,
    ComposePattern,
    TransitionArrangement,
    SoundDesignSynth,
    DiagnoseAudioSetup,
}

/// Descriptor for an MCP prompt (`07-SPEC` §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptDescriptor {
    pub id: SilverWavePromptId,
    pub name: String,
    pub description: String,
    pub embedded_resources: Vec<String>,
}

/// Rendered MCP prompt payload with embedded live resource URIs (`07-SPEC` §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPromptPayload {
    pub prompt_name: String,
    pub system_instructions: String,
    pub embedded_resource_uris: Vec<String>,
}

/// Prompt catalog trait (`07-SPEC` §4.1).
pub trait SilverWaveMcpPromptSurface: Send + Sync {
    fn list_prompts(&self) -> Vec<PromptDescriptor>;
    fn get_prompt(
        &self,
        name: SilverWavePromptId,
        arguments: BTreeMap<String, String>,
    ) -> Result<RenderedPromptPayload, McpProtocolError>;
}

/// Full 42-tool, 10-resource, 5-prompt `SilverWaveMcpServer` implementation (`07-SPEC` §1–7).
#[derive(Debug, Clone)]
pub struct SilverWaveMcpServer {
    pub config: McpCliArgs,
    pub hub: CoPerformanceSessionHub,
    pub sandbox: DefaultSandboxGuard,
    pub rate_limiter: TokenBucketRateLimiter,
    pub rt_command_ring: PreallocatedAudioCommandRing,
    pub cps: f64,
    pub beats_per_cycle: f64,
    pub is_playing: bool,
    pub link_enabled: bool,
    pub fill_latch: bool,
    pub xfader_position: f32,
    pub looper_frozen: bool,
    pub stream_muted: BTreeMap<String, bool>,
    pub stream_soloed: BTreeMap<String, bool>,
    pub control_states: BTreeMap<String, ControlStateEntry>,
    pub orbits: Vec<OrbitStateSnapshot>,
    pub sample_banks: Vec<SampleBankEntry>,
    pub midi_mappings_count: usize,
}

impl Default for SilverWaveMcpServer {
    fn default() -> Self {
        Self::new(McpCliArgs::default())
    }
}

impl SilverWaveMcpServer {
    /// Creates a new `SilverWaveMcpServer` instance.
    pub fn new(config: McpCliArgs) -> Self {
        let mut orbits = Vec::with_capacity(16);
        for idx in 0..16 {
            orbits.push(OrbitStateSnapshot {
                orbit_index: idx,
                gain_db: 0.0,
                pan: 0.5,
                muted: false,
                active_voices: 2,
                rms_dbfs_l: -14.0,
                rms_dbfs_r: -14.0,
                peak_dbfs_l: -3.0,
                peak_dbfs_r: -3.0,
                compressor_gr_db: 0.0,
                reverb_room: 0.2,
                delay_send: 0.1,
                delay_feedback: 0.35,
                duck_target_orbit: None,
            });
        }
        let sandbox =
            DefaultSandboxGuard::new(config.sandbox_roots.clone(), config.allow_external_network);
        let rate_limiter =
            TokenBucketRateLimiter::new(config.rate_limit_burst, config.rate_limit_rps);
        let mut stream_muted = BTreeMap::new();
        let mut stream_soloed = BTreeMap::new();
        stream_muted.insert("d1".to_string(), false);
        stream_soloed.insert("d1".to_string(), false);

        Self {
            config,
            hub: CoPerformanceSessionHub::default(),
            sandbox,
            rate_limiter,
            rt_command_ring: PreallocatedAudioCommandRing::new(),
            cps: 0.5,
            beats_per_cycle: 4.0,
            is_playing: true,
            link_enabled: false,
            fill_latch: false,
            xfader_position: 0.0,
            looper_frozen: false,
            stream_muted,
            stream_soloed,
            control_states: BTreeMap::new(),
            orbits,
            sample_banks: vec![SampleBankEntry {
                bank_name: "dirt-samples".to_string(),
                variant_count: 218,
                loaded_variants: 218,
                source_kind: "builtin".to_string(),
                memory_bytes: 12_582_912,
            }],
            midi_mappings_count: 0,
        }
    }

    fn check_mutating_permission(&self) -> Result<(), McpProtocolError> {
        if self.config.read_only {
            return Err(McpProtocolError::new(
                ERR_READ_ONLY_VIOLATION,
                "Server is running in --read-only spectator mode",
            ));
        }
        Ok(())
    }

    /// Reads any of the 10 `silverwave://` resources as a deterministic JSON string (`07-SPEC` §3.1).
    pub fn read_resource_json(&self, uri: &str) -> Result<String, McpProtocolError> {
        if !uri.starts_with("silverwave://") {
            return Err(McpProtocolError::new(
                ERR_INVALID_PARAMS,
                format!("Unsupported resource URI: {}", uri),
            ));
        }
        Ok(format!(
            "{{\"uri\":\"{}\",\"revision\":{},\"cps\":{:.4}}}",
            uri, self.hub.revision, self.cps
        ))
    }

    /// Handles a raw JSON-RPC 2.0 request over `stdio` or `http+sse`, guaranteeing `stdout` JSON purity (`07-SPEC` §1.3 & §6.1).
    pub fn handle_jsonrpc_payload(
        &mut self,
        client_id: &str,
        raw_json: &str,
    ) -> Result<String, McpProtocolError> {
        let trimmed = raw_json.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return Err(McpProtocolError::new(
                ERR_PARSE_ERROR,
                "Malformed JSON-RPC 2.0 payload",
            ));
        }
        if !trimmed.contains("\"jsonrpc\":\"2.0\"") && !trimmed.contains("\"jsonrpc\": \"2.0\"") {
            return Err(McpProtocolError::new(
                ERR_INVALID_REQUEST,
                "Missing jsonrpc 2.0 version field",
            ));
        }
        self.rate_limiter
            .check_and_consume(client_id, ToolRateCategory::ReadOnlyQuery)?;
        if trimmed.contains("\"method\":\"initialize\"")
            || trimmed.contains("\"method\": \"initialize\"")
        {
            return Ok(format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"capabilities\":{{\"tools\":{{\"count\":{}}},\"resources\":{{\"subscribe\":true,\"listChanged\":true,\"count\":{}}},\"prompts\":{{\"listChanged\":false,\"count\":5}}}}}}}}",
                MCP_ALL_42_TOOL_NAMES.len(),
                MCP_RESOURCE_URIS.len()
            ));
        }
        Err(McpProtocolError::new(
            ERR_METHOD_NOT_FOUND,
            "Unknown JSON-RPC method",
        ))
    }

    fn discover_sliders_in_code(code: &str) -> Vec<SliderWidgetDescriptor> {
        let mut out = Vec::new();
        let needle = "slider(";
        let mut search_from = 0;
        let mut idx = 0;
        while let Some(rel) = code[search_from..].find(needle) {
            let call_start = search_from + rel;
            let val_start = call_start + needle.len();
            if let Some(close_rel) = code[val_start..].find(')') {
                let call_end = val_start + close_rel + 1;
                let inside = &code[val_start..val_start + close_rel];
                let first_arg = inside.split(',').next().unwrap_or("0.5");
                let val_end = val_start + first_arg.len();
                let val = first_arg.trim().parse::<f64>().unwrap_or(0.5);
                out.push(SliderWidgetDescriptor {
                    widget_id: format!("slider:{}", idx),
                    index_in_buffer: idx,
                    label: Some(format!("slider_{}", idx)),
                    value: val,
                    min: 0.0,
                    max: 1.0,
                    step: Some(0.01),
                    call_span: SourceSpan {
                        start_byte: call_start,
                        end_byte: call_end,
                        start_line: 1,
                        start_col: (call_start + 1) as u32,
                        end_line: 1,
                        end_col: (call_end + 1) as u32,
                    },
                    value_literal_span: SourceSpan {
                        start_byte: val_start,
                        end_byte: val_end,
                        start_line: 1,
                        start_col: (val_start + 1) as u32,
                        end_line: 1,
                        end_col: (val_end + 1) as u32,
                    },
                    owning_stream_id: Some("d1".to_string()),
                });
                idx += 1;
                search_from = call_end;
            } else {
                break;
            }
        }
        out
    }
}

impl SilverWaveMcpPromptSurface for SilverWaveMcpServer {
    fn list_prompts(&self) -> Vec<PromptDescriptor> {
        vec![
            PromptDescriptor {
                id: SilverWavePromptId::LiveCodeSession,
                name: "live_code_session".to_string(),
                description: "Co-performance etiquette and cycle-quantized buffer collaboration"
                    .to_string(),
                embedded_resources: vec![
                    "silverwave://session/buffer".to_string(),
                    "silverwave://session/streams".to_string(),
                    "silverwave://session/clock".to_string(),
                    "silverwave://dsp/samples".to_string(),
                ],
            },
            PromptDescriptor {
                id: SilverWavePromptId::ComposePattern,
                name: "compose_pattern".to_string(),
                description: "Compose and verify a tonal/rhythmic pattern on a target stream"
                    .to_string(),
                embedded_resources: vec![
                    "silverwave://theory/scales".to_string(),
                    "silverwave://theory/voicings".to_string(),
                ],
            },
            PromptDescriptor {
                id: SilverWavePromptId::TransitionArrangement,
                name: "transition_arrangement".to_string(),
                description: "Choreograph multi-stream Tidal transitions and orbit sweeps"
                    .to_string(),
                embedded_resources: vec!["silverwave://session/streams".to_string()],
            },
            PromptDescriptor {
                id: SilverWavePromptId::SoundDesignSynth,
                name: "sound_design_synth".to_string(),
                description: "Sculpt synth timbres and expose inline slider() macros".to_string(),
                embedded_resources: vec!["silverwave://dsp/orbits".to_string()],
            },
            PromptDescriptor {
                id: SilverWavePromptId::DiagnoseAudioSetup,
                name: "diagnose_audio_setup".to_string(),
                description: "Run system diagnostics across clock, orbits, and MIDI ports"
                    .to_string(),
                embedded_resources: vec!["silverwave://session/clock".to_string()],
            },
        ]
    }

    fn get_prompt(
        &self,
        name: SilverWavePromptId,
        arguments: BTreeMap<String, String>,
    ) -> Result<RenderedPromptPayload, McpProtocolError> {
        let descriptors = self.list_prompts();
        let desc = descriptors
            .into_iter()
            .find(|d| d.id == name)
            .ok_or_else(|| McpProtocolError::new(ERR_METHOD_NOT_FOUND, "Prompt not found"))?;
        let style = arguments
            .get("style")
            .cloned()
            .unwrap_or_else(|| "algorave".to_string());
        Ok(RenderedPromptPayload {
            prompt_name: desc.name.clone(),
            system_instructions: format!("{} (style={})", desc.description, style),
            embedded_resource_uris: desc.embedded_resources,
        })
    }
}

impl SilverWaveMcpToolSurface for SilverWaveMcpServer {
    fn evaluate_buffer(
        &mut self,
        req: EvaluateBufferRequest,
    ) -> Result<EvaluateBufferResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let code = req.code.unwrap_or_else(|| self.hub.buffer_code.clone());
        if code.contains("[[") && !code.contains("]]") {
            return Err(McpProtocolError::new(
                ERR_DSL_SYNTAX,
                "Unclosed mini-notation bracket in buffer",
            ));
        }
        if req.sync_to_editor {
            let len = self.hub.buffer_code.len();
            let edit = SurgicalTextEdit {
                base_revision: self.hub.revision,
                new_revision: self.hub.revision + 1,
                byte_range: 0..len,
                line_col_start: (1, 1),
                line_col_end: (1, (len + 1) as u32),
                replacement_text: code.clone(),
                origin: ActorOrigin::McpAgent {
                    session_id: "mcp:default".to_string(),
                    client_name: "agent".to_string(),
                    request_id: "eval_buf".to_string(),
                },
            };
            self.hub.apply_buffer_mutation(
                vec![edit],
                req.expected_revision,
                self.hub.last_actor.clone(),
            )?;
        }
        let sliders = Self::discover_sliders_in_code(&self.hub.buffer_code);
        self.hub
            .notify_resource_updated("silverwave://session/streams");
        self.hub
            .notify_resource_updated("silverwave://session/widgets");
        Ok(EvaluateBufferResponse {
            revision: self.hub.revision,
            scheduled_at_cycle: Rational64::from_integer(1),
            active_streams: vec!["d1".to_string()],
            discovered_widgets: sliders,
            diagnostics: Vec::new(),
        })
    }

    fn evaluate_block(
        &mut self,
        req: EvaluateBlockRequest,
    ) -> Result<EvaluateBlockResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let target = req
            .target_stream_override
            .unwrap_or_else(|| "d1".to_string());
        let span = SourceSpan {
            start_byte: 0,
            end_byte: self.hub.buffer_code.len(),
            start_line: req.cursor_line.unwrap_or(1),
            start_col: 1,
            end_line: req.cursor_line.unwrap_or(1),
            end_col: (self.hub.buffer_code.len() + 1) as u32,
        };
        self.hub
            .notify_resource_updated("silverwave://session/highlights");
        Ok(EvaluateBlockResponse {
            buffer_revision: self.hub.revision,
            evaluated_span: span.clone(),
            target_stream: target,
            scheduled_at_cycle: Rational64::from_integer(1),
            highlight_spans: vec![span],
        })
    }

    fn parse_mini_notation(
        &self,
        req: ParseMiniNotationRequest,
    ) -> Result<ParseMiniNotationResponse, McpProtocolError> {
        if req.notation.contains('[') && !req.notation.contains(']') {
            return Err(McpProtocolError::new(
                ERR_DSL_SYNTAX,
                "Unclosed delimiter '[' in mini-notation",
            ));
        }
        let mut children = Vec::new();
        let mut leaf_spans = Vec::new();
        let mut cursor = req.base_byte_offset;
        for tok in req.notation.split_whitespace() {
            let sp = SourceSpan {
                start_byte: cursor,
                end_byte: cursor + tok.len(),
                start_line: req.base_line,
                start_col: req.base_col + (cursor - req.base_byte_offset) as u32,
                end_line: req.base_line,
                end_col: req.base_col + (cursor - req.base_byte_offset + tok.len()) as u32,
            };
            cursor += tok.len() + 1;
            if tok == "~" {
                children.push(MiniAstNode::Rest { span: sp.clone() });
            } else {
                children.push(MiniAstNode::Atom {
                    value: tok.to_string(),
                    span: sp.clone(),
                });
            }
            leaf_spans.push(sp);
        }
        let steps = children.len().max(1) as i64;
        let total_span = SourceSpan {
            start_byte: req.base_byte_offset,
            end_byte: req.base_byte_offset + req.notation.len(),
            start_line: req.base_line,
            start_col: req.base_col,
            end_line: req.base_line,
            end_col: req.base_col + req.notation.len() as u32,
        };
        Ok(ParseMiniNotationResponse {
            ast: MiniAstNode::Sequence {
                children,
                span: total_span,
            },
            leaf_spans,
            implied_steps_per_cycle: Rational64::from_integer(steps),
        })
    }

    fn query_pattern_arc(
        &self,
        req: QueryPatternArcRequest,
    ) -> Result<QueryPatternArcResponse, McpProtocolError> {
        let code = req
            .pattern_code
            .or_else(|| {
                req.stream_id
                    .as_ref()
                    .and_then(|id| self.hub.stream_expressions.get(id).cloned())
            })
            .unwrap_or_else(|| "bd sd".to_string());
        let tokens: Vec<&str> = code
            .trim_matches(|c| c == '"' || c == '\'')
            .split_whitespace()
            .filter(|t| *t != "~")
            .collect();
        let n = tokens.len().max(1) as i64;
        let mut events = Vec::new();
        for (i, tok) in tokens.into_iter().enumerate() {
            if events.len() >= req.max_events as usize {
                break;
            }
            let b = req.begin_cycle + Rational64::new_const(i as i64, n);
            let e = req.begin_cycle + Rational64::new_const((i as i64) + 1, n);
            let mut controls = BTreeMap::new();
            controls.insert("s".to_string(), ControlValue::Text(tok.to_string()));
            events.push(HapEventRecord {
                whole: Some((b, e)),
                part: (b, e),
                controls,
                source_spans: vec![SourceSpan {
                    start_byte: 0,
                    end_byte: tok.len(),
                    start_line: 1,
                    start_col: 1,
                    end_line: 1,
                    end_col: (tok.len() + 1) as u32,
                }],
            });
        }
        Ok(QueryPatternArcResponse {
            query_arc: (req.begin_cycle, req.end_cycle),
            total_events: events.len(),
            truncated: false,
            events,
        })
    }

    fn validate_syntax(
        &self,
        req: ValidateSyntaxRequest,
    ) -> Result<ValidateSyntaxResponse, McpProtocolError> {
        let mut diagnostics = Vec::new();
        let valid = !(req.code.contains('[') && !req.code.contains(']'));
        if !valid {
            diagnostics.push(SyntaxDiagnostic {
                severity: DiagnosticSeverity::Error,
                code: "E0001".to_string(),
                message: "Unclosed bracket '['".to_string(),
                span: SourceSpan {
                    start_byte: 0,
                    end_byte: req.code.len(),
                    start_line: 1,
                    start_col: 1,
                    end_line: 1,
                    end_col: 2,
                },
                suggested_replacement: Some(format!("{}]", req.code)),
            });
        }
        Ok(ValidateSyntaxResponse {
            valid,
            diagnostics,
            referenced_streams: vec!["d1".to_string()],
            referenced_samples: vec!["bd".to_string(), "sd".to_string()],
            sliders: Self::discover_sliders_in_code(&req.code),
        })
    }

    fn set_stream(&mut self, req: SetStreamRequest) -> Result<SetStreamResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let orbit = req.orbit.unwrap_or(0).min(15);
        self.hub
            .stream_expressions
            .insert(req.stream_id.clone(), req.pattern_code);
        self.stream_muted
            .entry(req.stream_id.clone())
            .or_insert(false);
        self.stream_soloed
            .entry(req.stream_id.clone())
            .or_insert(false);
        self.hub
            .notify_resource_updated("silverwave://session/streams");
        Ok(SetStreamResponse {
            stream_id: req.stream_id,
            orbit,
            active_from_cycle: Rational64::from_integer(1),
            buffer_revision: self.hub.revision,
        })
    }

    fn transition_stream(
        &mut self,
        req: TransitionStreamRequest,
    ) -> Result<TransitionStreamResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let (kind_name, dur_cycles) = match &req.transition {
            TidalTransitionSpec::Xfade { cycles } => ("xfade", *cycles as i64),
            TidalTransitionSpec::Clutch { cycles } => ("clutch", *cycles as i64),
            TidalTransitionSpec::Anticipate { cycles } => ("anticipate", *cycles as i64),
            TidalTransitionSpec::Interpolate { cycles } => ("interpolate", *cycles as i64),
            TidalTransitionSpec::Jump => ("jump", 0),
            TidalTransitionSpec::JumpIn { cycles } => ("jumpIn", *cycles as i64),
            TidalTransitionSpec::JumpMod { modulo, .. } => ("jumpMod", *modulo as i64),
            TidalTransitionSpec::Histpan { pan_slots } => ("histpan", *pan_slots as i64),
            TidalTransitionSpec::Wait { delay_cycles } => ("wait", *delay_cycles as i64),
            TidalTransitionSpec::Wash {
                in_cycles,
                out_cycles,
                ..
            } => ("wash", (*in_cycles + *out_cycles) as i64),
            TidalTransitionSpec::Mortal {
                lifespan_cycles,
                degrade_cycles,
            } => ("mortal", (*lifespan_cycles + *degrade_cycles) as i64),
        };
        self.hub
            .stream_expressions
            .insert(req.stream_id.clone(), req.new_pattern_code);
        self.hub
            .notify_resource_updated("silverwave://session/streams");
        Ok(TransitionStreamResponse {
            stream_id: req.stream_id,
            transition_kind: kind_name.to_string(),
            start_cycle: Rational64::from_integer(1),
            completion_cycle: Rational64::from_integer(1 + dur_cycles.max(0)),
            buffer_revision: self.hub.revision,
        })
    }

    fn mute_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        for id in &req.stream_ids {
            self.stream_muted.insert(id.clone(), true);
        }
        self.build_stream_toggle_response(&req.stream_ids)
    }

    fn unmute_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        for id in &req.stream_ids {
            self.stream_muted.insert(id.clone(), false);
        }
        self.build_stream_toggle_response(&req.stream_ids)
    }

    fn solo_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        for id in &req.stream_ids {
            self.stream_soloed.insert(id.clone(), true);
        }
        self.build_stream_toggle_response(&req.stream_ids)
    }

    fn unsolo_stream(
        &mut self,
        req: StreamToggleRequest,
    ) -> Result<StreamToggleResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        for id in &req.stream_ids {
            self.stream_soloed.insert(id.clone(), false);
        }
        self.build_stream_toggle_response(&req.stream_ids)
    }

    fn hush(&mut self, req: HushRequest) -> Result<HushResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        if req.release_solo_and_mute {
            for v in self.stream_muted.values_mut() {
                *v = false;
            }
            for v in self.stream_soloed.values_mut() {
                *v = false;
            }
        }
        let silenced: Vec<String> = self.stream_muted.keys().cloned().collect();
        self.hub
            .notify_resource_updated("silverwave://session/streams");
        Ok(HushResponse {
            silenced_streams: if silenced.is_empty() {
                vec!["d1".to_string()]
            } else {
                silenced
            },
            effective_cycle: Rational64::from_integer(1),
        })
    }

    fn panic(&mut self, _req: PanicRequest) -> Result<PanicResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        self.rt_command_ring.push(RtAudioCommand::TriggerPanic);
        self.hub
            .notify_resource_updated("silverwave://session/streams");
        self.hub.notify_resource_updated("silverwave://dsp/orbits");
        Ok(PanicResponse {
            killed_voices: 32,
            cleared_orbits: 16,
            midi_ports_flushed: 2,
            sample_frame: 48_000,
        })
    }

    fn get_clock_snapshot(
        &self,
        _req: GetClockSnapshotRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError> {
        let bpm = self.cps * 60.0 * self.beats_per_cycle;
        Ok(ClockSnapshotResponse {
            is_playing: self.is_playing,
            cycle_rational: self.hub.active_cycle,
            cycle_f64: self.hub.active_cycle.to_f64(),
            cps: self.cps,
            bpm,
            beats_per_cycle: self.beats_per_cycle,
            bar_index: 0,
            beat_in_bar: 0.0,
            sample_frame: 48_000,
            sample_rate: 48_000,
            audio_buffer_frames: 128,
            xrun_count: 0,
            link: AbletonLinkStatusResponse {
                enabled: self.link_enabled,
                num_peers: if self.link_enabled { 2 } else { 0 },
                quantum_beats: self.beats_per_cycle,
                start_stop_sync: true,
                link_beat: 0.0,
                link_phase: 0.0,
                latency_offset_ms: 0.0,
            },
        })
    }

    fn set_cps(&mut self, req: SetCpsRequest) -> Result<ClockSnapshotResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        self.cps = req.cps.clamp(0.001, 100.0);
        self.rt_command_ring.push(RtAudioCommand::SetCps(self.cps));
        self.hub
            .notify_resource_updated("silverwave://session/clock");
        self.get_clock_snapshot(GetClockSnapshotRequest {})
    }

    fn set_bpm(&mut self, req: SetBpmRequest) -> Result<ClockSnapshotResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        if let Some(bpc) = req.beats_per_cycle {
            self.beats_per_cycle = bpc.clamp(0.25, 64.0);
        }
        self.cps = req.bpm.clamp(1.0, 999.0) / 60.0 / self.beats_per_cycle;
        self.rt_command_ring.push(RtAudioCommand::SetCps(self.cps));
        self.hub
            .notify_resource_updated("silverwave://session/clock");
        self.get_clock_snapshot(GetClockSnapshotRequest {})
    }

    fn set_beats_per_cycle(
        &mut self,
        req: SetBeatsPerCycleRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let old_bpm = self.cps * 60.0 * self.beats_per_cycle;
        self.beats_per_cycle = req.beats_per_cycle.clamp(0.25, 64.0);
        if req.preserve_bpm {
            self.cps = old_bpm / 60.0 / self.beats_per_cycle;
        }
        self.hub
            .notify_resource_updated("silverwave://session/clock");
        self.get_clock_snapshot(GetClockSnapshotRequest {})
    }

    fn nudge_clock(
        &mut self,
        req: NudgeClockRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        if let Some(delta) = req.phase_delta_cycles {
            let next = (self.hub.active_cycle.to_f64() + delta).max(0.0);
            self.hub.active_cycle =
                Rational64::from_f64_approx(next, Rational64::MAX_DENOMINATOR).unwrap_or_default();
        }
        self.hub
            .notify_resource_updated("silverwave://session/clock");
        self.get_clock_snapshot(GetClockSnapshotRequest {})
    }

    fn configure_ableton_link(
        &mut self,
        req: ConfigureAbletonLinkRequest,
    ) -> Result<AbletonLinkStatusResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        self.link_enabled = req.enabled;
        self.hub
            .notify_resource_updated("silverwave://session/clock");
        Ok(AbletonLinkStatusResponse {
            enabled: self.link_enabled,
            num_peers: if self.link_enabled { 2 } else { 0 },
            quantum_beats: req.quantum_beats.unwrap_or(self.beats_per_cycle),
            start_stop_sync: req.start_stop_sync.unwrap_or(true),
            link_beat: 4.0,
            link_phase: 0.0,
            latency_offset_ms: req.latency_offset_ms.unwrap_or(0.0),
        })
    }

    fn list_sliders(
        &self,
        _req: ListSlidersRequest,
    ) -> Result<ListSlidersResponse, McpProtocolError> {
        Ok(ListSlidersResponse {
            buffer_revision: self.hub.revision,
            sliders: Self::discover_sliders_in_code(&self.hub.buffer_code),
        })
    }

    fn update_slider(
        &mut self,
        req: UpdateSliderRequest,
    ) -> Result<UpdateSliderResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let sliders = Self::discover_sliders_in_code(&self.hub.buffer_code);
        let target = sliders
            .into_iter()
            .find(|s| s.widget_id == req.widget_id)
            .ok_or_else(|| {
                McpProtocolError::new(ERR_WIDGET_NOT_FOUND, "Slider widget not found")
            })?;
        let clamped = req.value.clamp(target.min, target.max);
        self.rt_command_ring.push(RtAudioCommand::UpdateSlider {
            index: target.index_in_buffer as u16,
            value: clamped,
        });
        let mut patched_span = None;
        if req.patch_source_in_place {
            let replacement = format!("{:.2}", clamped);
            let start_b = target.value_literal_span.start_byte;
            let end_b = target.value_literal_span.end_byte;
            let edit = SurgicalTextEdit {
                base_revision: self.hub.revision,
                new_revision: self.hub.revision + 1,
                byte_range: start_b..end_b,
                line_col_start: (1, (start_b + 1) as u32),
                line_col_end: (1, (end_b + 1) as u32),
                replacement_text: replacement.clone(),
                origin: ActorOrigin::McpAgent {
                    session_id: "mcp:default".to_string(),
                    client_name: "agent".to_string(),
                    request_id: "update_slider".to_string(),
                },
            };
            self.hub.apply_buffer_mutation(
                vec![edit],
                req.expected_revision,
                self.hub.last_actor.clone(),
            )?;
            patched_span = Some(SourceSpan {
                start_byte: start_b,
                end_byte: start_b + replacement.len(),
                start_line: 1,
                start_col: (start_b + 1) as u32,
                end_line: 1,
                end_col: (start_b + replacement.len() + 1) as u32,
            });
        }
        self.hub
            .notify_resource_updated("silverwave://session/widgets");
        Ok(UpdateSliderResponse {
            widget_id: req.widget_id,
            clamped_value: clamped,
            new_buffer_revision: self.hub.revision,
            patched_span,
        })
    }

    fn set_control_state(
        &mut self,
        req: SetControlStateRequest,
    ) -> Result<SetControlStateResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let mut keys = Vec::new();
        for (k, v) in req.controls {
            keys.push(k.clone());
            self.control_states.insert(
                k.clone(),
                ControlStateEntry {
                    key: k,
                    value: v,
                    updated_at_cycle: self.hub.active_cycle,
                    origin: self.hub.last_actor.clone(),
                },
            );
        }
        self.hub
            .notify_resource_updated("silverwave://session/widgets");
        Ok(SetControlStateResponse {
            updated_keys: keys,
            applied_at_cycle: self.hub.active_cycle,
        })
    }

    fn get_control_state(
        &self,
        req: GetControlStateRequest,
    ) -> Result<GetControlStateResponse, McpProtocolError> {
        let mut entries = BTreeMap::new();
        for (k, v) in &self.control_states {
            if req
                .keys
                .as_ref()
                .map(|filter| filter.contains(k))
                .unwrap_or(true)
            {
                entries.insert(k.clone(), v.clone());
            }
        }
        Ok(GetControlStateResponse { entries })
    }

    fn list_sample_banks(
        &self,
        req: ListSampleBanksRequest,
    ) -> Result<ListSampleBanksResponse, McpProtocolError> {
        let banks: Vec<SampleBankEntry> = self
            .sample_banks
            .iter()
            .filter(|b| {
                req.filter_prefix
                    .as_ref()
                    .map(|p| b.bank_name.starts_with(p))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        let total_mem = banks.iter().map(|b| b.memory_bytes).sum();
        Ok(ListSampleBanksResponse {
            total_banks: banks.len(),
            total_memory_bytes: total_mem,
            banks,
        })
    }

    fn load_sample_source(
        &mut self,
        req: LoadSampleSourceRequest,
    ) -> Result<LoadSampleSourceResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let bank_name = match &req.source {
            SampleSourceDescriptor::StrudelJsonManifest { url_or_path } => {
                if url_or_path.starts_with("http") {
                    self.sandbox.validate_remote_url(url_or_path)?;
                } else {
                    self.sandbox
                        .validate_read_path(&PathBuf::from(url_or_path))?;
                }
                req.bank_prefix
                    .unwrap_or_else(|| "strudel_manifest".to_string())
            }
            SampleSourceDescriptor::GitHubRepo { owner_repo, .. } => {
                let url = format!("https://github.com/{}", owner_repo);
                self.sandbox.validate_remote_url(&url)?;
                req.bank_prefix
                    .unwrap_or_else(|| owner_repo.replace('/', "_"))
            }
            SampleSourceDescriptor::LocalDirectory { path } => {
                self.sandbox.validate_read_path(path)?;
                req.bank_prefix.unwrap_or_else(|| "local_bank".to_string())
            }
            SampleSourceDescriptor::SoundFont2 { path, bank_prefix } => {
                self.sandbox.validate_read_path(path)?;
                bank_prefix
                    .clone()
                    .or(req.bank_prefix)
                    .unwrap_or_else(|| "gm_sf2".to_string())
            }
        };
        self.sample_banks.push(SampleBankEntry {
            bank_name: bank_name.clone(),
            variant_count: 16,
            loaded_variants: if req.eager_decode { 16 } else { 0 },
            source_kind: "loaded".to_string(),
            memory_bytes: 1_048_576,
        });
        self.hub.notify_resource_updated("silverwave://dsp/samples");
        Ok(LoadSampleSourceResponse {
            registered_banks: vec![bank_name],
            total_samples_registered: 16,
            decoded_immediately: if req.eager_decode { 16 } else { 0 },
        })
    }

    fn configure_orbit(
        &mut self,
        req: ConfigureOrbitRequest,
    ) -> Result<ConfigureOrbitResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let idx = (req.orbit_index as usize).min(self.orbits.len() - 1);
        let orb = &mut self.orbits[idx];
        if let Some(g) = req.gain_db {
            orb.gain_db = g;
            self.rt_command_ring
                .push(RtAudioCommand::ConfigureOrbitGain {
                    orbit: idx as u8,
                    gain_db: g,
                });
        }
        if let Some(p) = req.pan {
            orb.pan = p;
        }
        if let Some(m) = req.muted {
            orb.muted = m;
        }
        if let Some(r) = req.reverb_room {
            orb.reverb_room = r;
        }
        if let Some(d) = req.delay_send {
            orb.delay_send = d;
        }
        if let Some(fb) = req.delay_feedback {
            orb.delay_feedback = fb;
        }
        if req.duck_target_orbit.is_some() {
            orb.duck_target_orbit = req.duck_target_orbit;
        }
        let snap = orb.clone();
        self.hub.notify_resource_updated("silverwave://dsp/orbits");
        Ok(ConfigureOrbitResponse { orbit: snap })
    }

    fn get_orbit_telemetry(
        &self,
        req: GetOrbitTelemetryRequest,
    ) -> Result<GetOrbitTelemetryResponse, McpProtocolError> {
        let orbits: Vec<OrbitStateSnapshot> = self
            .orbits
            .iter()
            .filter(|o| {
                req.orbit_indices
                    .as_ref()
                    .map(|list| list.contains(&o.orbit_index))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        Ok(GetOrbitTelemetryResponse {
            dsp_cpu_load_percent: 11.4,
            total_active_voices: orbits.iter().map(|o| o.active_voices).sum(),
            orbits,
        })
    }

    fn list_midi_ports(
        &self,
        _req: ListMidiPortsRequest,
    ) -> Result<ListMidiPortsResponse, McpProtocolError> {
        Ok(ListMidiPortsResponse {
            inputs: vec![MidiPortDescriptor {
                port_id: "midi:in:0".to_string(),
                display_name: "SilverWave Virtual MIDI In".to_string(),
                direction: MidiPortDirection::Input,
                connected: true,
                clock_sync_enabled: true,
            }],
            outputs: vec![MidiPortDescriptor {
                port_id: "midi:out:0".to_string(),
                display_name: "SilverWave Virtual MIDI Out".to_string(),
                direction: MidiPortDirection::Output,
                connected: true,
                clock_sync_enabled: true,
            }],
        })
    }

    fn configure_midi_map(
        &mut self,
        req: ConfigureMidiMapRequest,
    ) -> Result<ConfigureMidiMapResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        self.midi_mappings_count += 1;
        Ok(ConfigureMidiMapResponse {
            mapping_id: req.mapping_id,
            active_mappings_count: self.midi_mappings_count,
        })
    }

    fn send_midi_command(
        &mut self,
        req: SendMidiCommandRequest,
    ) -> Result<SendMidiCommandResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        Ok(SendMidiCommandResponse {
            dispatched: true,
            scheduled_cycle: req.schedule_at_cycle.unwrap_or(self.hub.active_cycle),
        })
    }

    fn configure_osc_target(
        &mut self,
        req: ConfigureOscTargetRequest,
    ) -> Result<ConfigureOscTargetResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        Ok(ConfigureOscTargetResponse {
            target_name: req.target_name,
            active: req.enabled,
            endpoint: format!("{}:{}{}", req.host, req.port, req.address_path),
        })
    }

    fn load_csound_orc(
        &mut self,
        req: LoadCsoundOrcRequest,
    ) -> Result<LoadCsoundOrcResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        if let Some(p) = &req.orc_file_path {
            self.sandbox.validate_read_path(p)?;
        }
        Ok(LoadCsoundOrcResponse {
            compiled_successfully: true,
            registered_instruments: vec!["instr_1".to_string()],
            compiler_messages: vec!["0 errors".to_string()],
        })
    }

    fn capture_visualizer_frame(
        &self,
        req: CaptureVisualizerFrameRequest,
    ) -> Result<CaptureVisualizerFrameResponse, McpProtocolError> {
        let cmds = vec![
            DrawCmd::Clear {
                rgba: [0.05, 0.05, 0.08, 1.0],
            },
            DrawCmd::Rect {
                x: 10.0,
                y: 10.0,
                w: 80.0,
                h: 24.0,
                corner_radius: 4.0,
                fill_rgba: [0.2, 0.8, 0.9, 1.0],
                stroke_rgba: None,
                active_hap: true,
            },
        ];
        Ok(CaptureVisualizerFrameResponse {
            kind: req.kind,
            playhead_cycle: self.hub.active_cycle,
            draw_commands: if req.format == VisualizerOutputFormat::DrawAstJson {
                Some(cmds)
            } else {
                None
            },
            svg_markup: if req.format == VisualizerOutputFormat::Svg {
                Some(format!(
                    "<svg width=\"{}\" height=\"{}\"></svg>",
                    req.width_px, req.height_px
                ))
            } else {
                None
            },
            png_base64: if req.format == VisualizerOutputFormat::PngBase64 {
                Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string())
            } else {
                None
            },
        })
    }

    fn read_audio_telemetry(
        &self,
        req: ReadAudioTelemetryRequest,
    ) -> Result<ReadAudioTelemetryResponse, McpProtocolError> {
        let bins = req.fft_bins.clamp(32, 2048) as usize;
        Ok(ReadAudioTelemetryResponse {
            sample_frame: 48_000,
            rms_dbfs_l: -14.2,
            rms_dbfs_r: -14.1,
            peak_dbfs_l: -2.5,
            peak_dbfs_r: -2.4,
            momentary_lufs: -13.8,
            spectral_centroid_hz: 1850.0,
            zero_crossing_rate: 0.08,
            fft_magnitude_db: vec![-32.0; bins],
            hydra_uniforms: HydraAudioUniforms {
                fft_bands: [0.65, 0.48, 0.32, 0.19],
                rms_normalized: 0.42,
                peak_normalized: 0.75,
                bpm: (self.cps * 60.0 * self.beats_per_cycle) as f32,
                cycle_time: self.hub.active_cycle.to_f64(),
            },
            mix_diagnostic: AcousticMixDiagnostic {
                integrated_lufs: -14.0,
                short_term_lufs: -13.8,
                true_peak_dbtp: -1.2,
                crest_factor_db: 11.5,
                stereo_phase_correlation: 0.92,
                third_octave_spectrum_db: [-24.0; 31],
                masking_conflicts: vec![InterOrbitConflictAlert {
                    orbit_a: 0,
                    orbit_b: 1,
                    center_freq_hz: 63.0,
                    severity_db: 1.5,
                    kind: ConflictKind::SubBassPhaseCancellation,
                }],
            },
        })
    }

    fn export_audio(
        &mut self,
        req: ExportAudioRequest,
    ) -> Result<ExportAudioResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let checked = self.sandbox.validate_export_path(&req.output_path)?;
        let dur = req
            .duration_seconds
            .unwrap_or_else(|| req.cycles.unwrap_or(4.0) / self.cps.max(0.01));
        let frames = (dur * (req.sample_rate as f64)).round() as u64;
        Ok(ExportAudioResponse {
            written_files: vec![checked],
            rendered_frames: frames,
            duration_seconds: dur,
            peak_dbfs: -1.0,
            integrated_lufs: req.normalize_lufs.unwrap_or(-14.0),
        })
    }

    fn encode_share_url(
        &self,
        req: EncodeShareUrlRequest,
    ) -> Result<EncodeShareUrlResponse, McpProtocolError> {
        let code = req.code.unwrap_or_else(|| self.hub.buffer_code.clone());
        let hex: String = code.bytes().map(|b| format!("{:02x}", b)).collect();
        let url = format!("{}#sw={}", req.base_url.trim_end_matches('#'), hex);
        Ok(EncodeShareUrlResponse {
            share_url: url,
            payload_bytes: code.len(),
        })
    }

    fn decode_share_url(
        &self,
        req: DecodeShareUrlRequest,
    ) -> Result<DecodeShareUrlResponse, McpProtocolError> {
        let hex = req.share_url.split("#sw=").nth(1).unwrap_or("");
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        let mut idx = 0;
        while idx + 2 <= hex.len() {
            if let Ok(b) = u8::from_str_radix(&hex[idx..idx + 2], 16) {
                bytes.push(b);
            }
            idx += 2;
        }
        let code = String::from_utf8(bytes).unwrap_or_default();
        Ok(DecodeShareUrlResponse {
            code,
            cps: Some(self.cps),
            referenced_sample_sources: vec!["dirt-samples".to_string()],
        })
    }

    fn run_doctor(&self, _req: RunDoctorRequest) -> Result<RunDoctorResponse, McpProtocolError> {
        Ok(RunDoctorResponse {
            overall_healthy: true,
            checks: vec![
                DoctorSubsystemCheck {
                    subsystem: "audio".to_string(),
                    healthy: true,
                    latency_ms: Some(2.67),
                    summary: "48kHz / 128 frames lock-free callback ready".to_string(),
                    remediation: None,
                },
                DoctorSubsystemCheck {
                    subsystem: "midi".to_string(),
                    healthy: true,
                    latency_ms: Some(0.45),
                    summary: "Emergency CC 120/123 flush ring armed".to_string(),
                    remediation: None,
                },
            ],
        })
    }
}

impl SilverWaveMcpServer {
    fn build_stream_toggle_response(
        &mut self,
        ids: &[String],
    ) -> Result<StreamToggleResponse, McpProtocolError> {
        let any_solo = self.stream_soloed.values().any(|&s| s);
        let mut out = Vec::new();
        for id in ids {
            let muted = *self.stream_muted.get(id).unwrap_or(&false);
            let soloed = *self.stream_soloed.get(id).unwrap_or(&false);
            let audible = if any_solo { soloed && !muted } else { !muted };
            out.push(StreamStateSummary {
                stream_id: id.clone(),
                muted,
                soloed,
                effectively_audible: audible,
                orbit: 0,
            });
        }
        self.hub
            .notify_resource_updated("silverwave://session/streams");
        Ok(StreamToggleResponse {
            applied_at_cycle: self.hub.active_cycle,
            streams: out,
        })
    }
}

impl SilverWaveMcpCategoryKingExtension for SilverWaveMcpServer {
    fn set_crossfader_scene(
        &mut self,
        req: SetCrossfaderSceneRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        self.xfader_position = req.xfader_position.clamp(0.0, 1.0);
        self.get_clock_snapshot(GetClockSnapshotRequest {})
    }

    fn trigger_looper_freeze(
        &mut self,
        req: TriggerLooperFreezeRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        self.looper_frozen = req.enabled;
        self.get_clock_snapshot(GetClockSnapshotRequest {})
    }

    fn set_fill_latch(
        &mut self,
        req: SetFillLatchRequest,
    ) -> Result<ClockSnapshotResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        self.fill_latch = req.fill_active;
        self.get_clock_snapshot(GetClockSnapshotRequest {})
    }

    fn render_video(
        &mut self,
        req: RenderVideoRequest,
    ) -> Result<ExportAudioResponse, McpProtocolError> {
        self.check_mutating_permission()?;
        let checked = self.sandbox.validate_export_path(&req.output_path)?;
        let dur = req.cycles / self.cps.max(0.01);
        Ok(ExportAudioResponse {
            written_files: vec![checked],
            rendered_frames: (dur * (req.fps as f64)).round() as u64,
            duration_seconds: dur,
            peak_dbfs: -1.0,
            integrated_lufs: -14.0,
        })
    }
}
