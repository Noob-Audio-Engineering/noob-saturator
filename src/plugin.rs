//! The nih-plug plug-in: VST3 and CLAP, stereo in and stereo out. Its editor
//! is the OS web view showing the Vue SPA from `web/dist`, embedded in the
//! binary.
//!
//! How the pieces connect:
//!
//! * The parameters are nih-plug parameters with the same ids as the
//!   standalone's specs ([`crate::dsp::param_specs`]), mirrored into the
//!   bridge by [`NoobVstWebguiFrameworkEditor::with_builder`], so the same
//!   page drives both. The mirroring samples nih-plug's own mapping into a
//!   table, so the page's knob is exactly this plug-in's knob rather than a
//!   second guess at it.
//! * `process` reads a [`Settings`] snapshot from the nih-plug values,
//!   configures the [`Processor`], runs the block and publishes the streams
//!   through the audio handle.
//! * The latency is reported to the host **unconditionally**, at every
//!   sample rate, and re-reported whenever the oversampling factor changes.
//!   That is one of the four things this plug-in exists to get right, so it
//!   is not conditional on anything: a plug-in that delays its output without
//!   saying so desynchronises against every other track, and delay
//!   compensation only works if the plug-in tells the truth.
//! * The page's UI store (presets, window size) is persisted with the
//!   plug-in state by [`NoobSaturatorParams::ui_store`], a `StoreSlot`.
//!
//! One deliberate absence: **there is no quality parameter.** The device is
//! always antialiased. What it has instead is an oversampling factor on the
//! panel, automatable, with a published aliasing figure for every one of its
//! settings — which is the design Ableton themselves chose for Operator and
//! for EQ Eight, and never applied to the device this one answers.

use std::sync::Arc;

use include_dir::{Dir, include_dir};
use nih_plug::prelude::*;
use noob_vst_webgui_framework::Assets;
use noob_vst_webgui_framework_nih::{EditorConfig, PluginHost, StoreSlot, UiStoreParams};

use crate::dsp::{self, Processor, color, engine::Settings};

static UI: Dir = include_dir!("$CARGO_MANIFEST_DIR/web/dist");

fn ui_lookup(path: &str) -> Option<&'static [u8]> {
    UI.get_file(path).map(|f| f.contents())
}

/// The shape. Ours, not theirs, and in order of hardness.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CurveParam {
    #[name = "Warm"]
    Warm,
    #[name = "Round"]
    Round,
    #[name = "Soft"]
    Soft,
    #[name = "Clip"]
    Clip,
    #[name = "Fold"]
    Fold,
    #[name = "Gate"]
    Gate,
}

/// How the dry/wet control crossfades. Ableton added an equal-loudness
/// option to Delay in Live 12 and not to this device; we offer both and
/// default to the linear one, so an A/B against theirs is a fair one.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MixLawParam {
    #[name = "Linear"]
    Linear,
    #[name = "Equal Power"]
    EqualPower,
}

/// The ceiling stage after the shape. Ableton's is a second instance of one
/// of their own curves; Live 12 added a hard mode, so ours has both.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipModeParam {
    #[name = "Off"]
    Off,
    #[name = "Soft"]
    Soft,
    #[name = "Hard"]
    Hard,
}

/// The oversampling factor: a visible, automatable control with a published
/// aliasing figure per setting, in place of a hidden quality switch.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum OversampleParam {
    #[name = "2x"]
    X2,
    #[name = "4x"]
    X4,
    #[name = "8x"]
    X8,
    #[name = "16x"]
    X16,
}

/// Every host parameter. The ids match the standalone's specs and the page.
///
/// The [`Params`] implementation is written out rather than derived, so that
/// the ids and the group names are the ones [`crate::dsp::param_specs`]
/// declares rather than whatever the field names happen to spell, and so
/// that the page's store rides along in the plug-in state.
pub struct NoobSaturatorParams {
    /// Pre-gain into the shape. Symmetric, as theirs is, so the device can
    /// also be used as a 36 dB attenuator into a fixed curve.
    pub drive: FloatParam,
    /// Operating-point offset. Ours: they have no asymmetry control, and
    /// asymmetry is what makes a saturator sound like a valve rather than a
    /// clipper.
    pub bias: FloatParam,
    pub curve: EnumParam<CurveParam>,
    /// Trim after the ceiling. **Bipolar**, where theirs only attenuates: a
    /// device that cannot give back the level it took forces a second one
    /// into the chain.
    pub output: FloatParam,
    pub mix: FloatParam,
    pub mix_law: EnumParam<MixLawParam>,
    pub color_on: BoolParam,
    /// The low shelf's gain: how much the low end reaches the nonlinear part
    /// of the curve, not a tone control.
    pub color_base: FloatParam,
    pub color_freq: FloatParam,
    /// A **stated Q**, where theirs is a unit-free 0…1 whose meaning has
    /// never been published.
    pub color_q: FloatParam,
    pub color_depth: FloatParam,
    /// The direct-current filters, on by default. Theirs ships off and, in
    /// the current version, hidden in a context menu.
    pub dc_block: BoolParam,
    pub clip_mode: EnumParam<ClipModeParam>,
    /// The soft clipper's knee, shared by the `Clip` curve and the ceiling
    /// stage because they are the same shape.
    pub clip_knee: FloatParam,
    pub oversample: EnumParam<OversampleParam>,
    /// The page's presets and window size; not parameters, but saved with
    /// the state.
    pub ui_store: StoreSlot,
}

impl Default for NoobSaturatorParams {
    fn default() -> Self {
        let d = Settings::default();
        let db = |name: &str, default: f32, min: f32, max: f32| {
            FloatParam::new(name, default, FloatRange::Linear { min, max })
                .with_unit(" dB")
                .with_step_size(0.1)
        };
        NoobSaturatorParams {
            drive: db("Drive", d.drive_db, dsp::DRIVE_MIN_DB, dsp::DRIVE_MAX_DB),
            bias: FloatParam::new(
                "Bias",
                d.bias,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_step_size(0.001),
            curve: EnumParam::new("Curve", CurveParam::Soft),
            output: db(
                "Output",
                d.output_db,
                dsp::OUTPUT_MIN_DB,
                dsp::OUTPUT_MAX_DB,
            ),
            mix: FloatParam::new(
                "Dry/Wet",
                d.mix * 100.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_step_size(0.1),
            mix_law: EnumParam::new("Mix Law", MixLawParam::Linear),
            color_on: BoolParam::new("Colour", d.color.on),
            color_base: db(
                "Colour Base",
                d.color.base_db,
                -dsp::COLOR_BASE_DB,
                dsp::COLOR_BASE_DB,
            ),
            // Skewed rather than logarithmic, because nih-plug has no
            // logarithmic range; the factor puts the geometric middle of the
            // span near the middle of the travel. The endpoints and the
            // default are the target device's own, and the page reads this
            // mapping from a table rather than guessing it, so the knob the
            // user turns is this one.
            color_freq: FloatParam::new(
                "Colour Freq",
                d.color.freq_hz,
                FloatRange::Skewed {
                    min: color::FREQ_MIN_HZ,
                    max: color::FREQ_MAX_HZ,
                    factor: FloatRange::skew_factor(-2.2),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            // Named for the control it answers and united for what it is, so
            // the improvement survives outside our own editor: a host's
            // generic parameter view reads "0.70 Q" where theirs reads "0.3".
            color_q: FloatParam::new(
                "Colour Width",
                d.color.q,
                FloatRange::Skewed {
                    min: dsp::COLOR_Q_MIN,
                    max: dsp::COLOR_Q_MAX,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" Q")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            color_depth: db(
                "Colour Depth",
                d.color.depth_db,
                -dsp::COLOR_DEPTH_DB,
                dsp::COLOR_DEPTH_DB,
            ),
            dc_block: BoolParam::new("Pre DC Filter", d.dc_block),
            clip_mode: EnumParam::new("Post Clip", ClipModeParam::Off),
            // Decibels below the ceiling, not a fraction: zero is a hard
            // corner and the number says where the knee starts.
            clip_knee: FloatParam::new(
                "Clip Knee",
                d.clip_knee_db,
                FloatRange::Linear {
                    min: 0.0,
                    max: dsp::CLIP_KNEE_MAX_DB,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1),
            oversample: EnumParam::new("Oversample", OversampleParam::X16),
            ui_store: StoreSlot::new(),
        }
    }
}

// SAFETY: every pointer comes from a field of `self`, which nih-plug keeps
// alive in an `Arc` for the plug-in's whole life. Written by hand so the ids
// and the groups match the standalone and the page.
unsafe impl Params for NoobSaturatorParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        let g = |s: &str| s.to_string();
        vec![
            (g("drive"), self.drive.as_ptr(), g("shape")),
            (g("bias"), self.bias.as_ptr(), g("shape")),
            (g("curve"), self.curve.as_ptr(), g("shape")),
            (g("dc_block"), self.dc_block.as_ptr(), g("shape")),
            (g("color_on"), self.color_on.as_ptr(), g("colour")),
            (g("color_base"), self.color_base.as_ptr(), g("colour")),
            (g("color_freq"), self.color_freq.as_ptr(), g("colour")),
            (g("color_q"), self.color_q.as_ptr(), g("colour")),
            (g("color_depth"), self.color_depth.as_ptr(), g("colour")),
            (g("clip_mode"), self.clip_mode.as_ptr(), g("output")),
            (g("clip_knee"), self.clip_knee.as_ptr(), g("output")),
            (g("output"), self.output.as_ptr(), g("output")),
            (g("mix"), self.mix.as_ptr(), g("output")),
            (g("mix_law"), self.mix_law.as_ptr(), g("output")),
            (g("oversample"), self.oversample.as_ptr(), g("quality")),
        ]
    }

    noob_vst_webgui_framework_nih::ui_store_fields!(ui_store);
}

impl UiStoreParams for NoobSaturatorParams {
    fn ui_store(&self) -> &StoreSlot {
        &self.ui_store
    }
}

impl NoobSaturatorParams {
    /// One block's worth of settings, read from the host's values.
    fn settings(&self) -> Settings {
        Settings {
            drive_db: self.drive.value(),
            bias: self.bias.value() / 100.0,
            curve: self.curve.value() as usize,
            output_db: self.output.value(),
            mix: (self.mix.value() / 100.0).clamp(0.0, 1.0),
            mix_law: self.mix_law.value() as usize,
            color: color::Settings {
                on: self.color_on.value(),
                base_db: self.color_base.value(),
                freq_hz: self.color_freq.value(),
                q: self.color_q.value(),
                depth_db: self.color_depth.value(),
            },
            dc_block: self.dc_block.value(),
            clip_mode: self.clip_mode.value() as usize,
            clip_knee_db: self.clip_knee.value(),
            oversample: self.oversample.value() as usize,
        }
    }
}

/// The plug-in.
pub struct NoobSaturator {
    params: Arc<NoobSaturatorParams>,
    host: PluginHost,
    processor: Processor,
    last_latency: usize,
}

impl Default for NoobSaturator {
    fn default() -> Self {
        let params = Arc::new(NoobSaturatorParams::default());
        let host = PluginHost::new(
            "noob-saturator",
            &params,
            dsp::streams(48_000.0),
            EditorConfig::new(1000, 620)
                .size_limits((900, 520), (7680, 4320))
                .assets(Assets::Lookup(ui_lookup)),
            |b| {
                b.meta(serde_json::json!({
                    "vendor": "Noob Audio Engineering",
                    "version": env!("CARGO_PKG_VERSION"),
                    "sample_rate": 48_000.0,
                    "standalone": false,
                    "transfer_points": dsp::TRANSFER_POINTS,
                    "color_points": dsp::COLOR_POINTS,
                    "dc_corner_hz": dsp::engine::DC_HZ,
                    "color_shelf_hz": color::SHELF_HZ,
                    "alias_band_hz": dsp::ALIAS_BAND_HZ,
                    "oversample_taps": dsp::oversample::TAPS,
                }))
            },
        );
        NoobSaturator {
            params,
            host,
            processor: Processor::new(48_000.0),
            last_latency: usize::MAX,
        }
    }
}

impl Plugin for NoobSaturator {
    const NAME: &'static str = "Noob Saturator";
    noob_vst_webgui_framework_nih::noob_identity!();

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] =
        noob_vst_webgui_framework_nih::stereo_or_mono_io!();

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(self.host.editor())
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.processor.set_sample_rate(buffer_config.sample_rate);
        self.processor.configure(&self.params.settings());
        self.last_latency = self.processor.latency();
        // Unconditional, at the host's rate, whatever the settings are.
        context.set_latency_samples(self.last_latency as u32);
        self.host.bridge().send_json(
            "sample_rate",
            serde_json::json!({ "sample_rate": buffer_config.sample_rate }),
        );
        true
    }

    fn reset(&mut self) {
        self.processor.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.processor.configure(&self.params.settings());
        let latency = self.processor.latency();
        if latency != self.last_latency {
            self.last_latency = latency;
            context.set_latency_samples(latency as u32);
        }
        let channels = buffer.channels();
        let slices = buffer.as_slice();
        if channels >= 2 {
            let (a, b) = slices.split_at_mut(1);
            self.processor.process(&mut *a[0], &mut *b[0]);
        } else if channels == 1 {
            // Mono: process the one channel against a copy of itself and
            // keep only the left result.
            let l = &mut *slices[0];
            let mut r = [0.0f32; 4096];
            let n = l.len().min(r.len());
            r[..n].copy_from_slice(&l[..n]);
            self.processor.process(&mut l[..n], &mut r[..n]);
        }
        if let Some(audio) = self.host.audio() {
            self.processor.publish(audio);
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for NoobSaturator {
    const VST3_CLASS_ID: [u8; 16] = *b"NoobSaturatorV3W";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

impl ClapPlugin for NoobSaturator {
    const CLAP_ID: &'static str = "io.github.noob-audio-engineering.noob-saturator";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "An antialiased waveshaper with a latency-matched dry path and a live aliasing readout, with a web-view editor over noob-vst-webgui-framework",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}
