//! The DSP of Noob Saturator, and the bridge description shared by the
//! plug-in and the standalone.
//!
//! It is a waveshaper whose whole reason to exist is that it does not alias
//! where the device it answers does, and that its dry/wet control sums
//! without combing. Everything here follows from those two, plus the rule
//! that a claim we cannot measure is not made.
//!
//! ## Layout
//!
//! | module | contents |
//! |---|---|
//! | [`curve`] | the six shapes, each with its antiderivative in closed form |
//! | [`adaa`] | first-order antiderivative antialiasing, and the threshold it lives or dies by |
//! | [`oversample`] | the half-band cascade, the tap count that keeps the latency whole, and the matched dry delay |
//! | [`color`] | the pre-shaper equaliser and its exact algebraic inverse |
//! | [`alias`] | the live non-harmonic energy readout |
//! | [`engine`] | the whole chain, the mix, the latency, the transfer curve |
//! | [`source`] | the standalone's demo signals |
//! | this file | [`engine::Settings`], parameter ids and specs, streams, the bridge builder, the [`Processor`] |
//!
//! ## Parameters
//!
//! [`param_specs`] describes every parameter once; the standalone builds its
//! bridge from it directly and the plug-in's nih-plug parameters use the same
//! ids, so the same page drives both. Ids are stable API.
//!
//! Where a range or a default came from the device this one answers, it came
//! from that device's own serialised parameter file rather than from prose,
//! because its manual publishes no numeric range for any control. Where we
//! deliberately differ, the table says so.
//!
//! | id | range / labels | default | ours or theirs |
//! |---|---|---|---|
//! | `drive` | −36 … +36 dB | 0 | theirs, unchanged |
//! | `bias` | −100 … +100 % of the clipping point | 0 | **ours** — they have no asymmetry control |
//! | `curve` | Warm, Round, Soft, Clip, Fold, Gate | Soft | **ours** — our shapes, our equations, printed |
//! | `output` | −36 … +36 dB | 0 | **improved** — theirs only attenuates, so it cannot restore level after a heavy drive |
//! | `mix` | 0 … 100 % | 100 | theirs, with the dry path latency-matched |
//! | `mix_law` | Linear, Equal Power | Linear | **ours** — they added an equal-loudness option to another device and not to this one |
//! | `color_on` | toggle | on | theirs, including the default |
//! | `color_base` | −36 … +36 dB | 0 | theirs |
//! | `color_freq` | 30 … 18500 Hz, log | 1000 | theirs |
//! | `color_q` | 0.1 … 10 Q, log; named "Colour Width" | 0.7 | **improved** — theirs is a unit-free 0…1 whose meaning is unpublished |
//! | `color_depth` | −24 … +24 dB | 0 | theirs |
//! | `dc_block` | toggle | **on** | **improved** — theirs is off by default and, in the current version, hidden in a context menu |
//! | `clip_mode` | Off, Soft, Hard | Off | theirs, including the hard mode they added |
//! | `clip_knee` | 0 … 24 dB below the ceiling | 6 | **ours** — a knee width with a unit on it, in the spirit of the threshold control they added to one curve. One knee shapes the `Clip` curve and the ceiling stage alike, because they are the same shape |
//! | `oversample` | 2x, 4x, 8x, 16x | **16x** | **ours** — a visible control with a published aliasing figure per setting, in place of a hidden quality switch. The default is the lowest factor that reaches the −80 dB target on every curve over a tone sweep |
//! | `src_kind`, `src_level`, `src_freq` | standalone only | | |
//!
//! There is no quality mode, deliberately. The device is always antialiased.
//!
//! ## Streams
//!
//! | id | kind | values | rate | contents |
//! |---|---|---|---|---|
//! | `meter` | meter | 4 | every block | `[in_peak, out_peak, in_rms, out_rms]`, linear amplitude |
//! | `alias` | raw | 4 | every block | `[alias_db, f0_hz, confidence, harmonic_db]`. Measured on the signal actually passing, not on a probe tone. `alias_db` is everything that is not at a harmonic of the fundamental the meter found; `harmonic_db` is the wanted distortion from the second order up. **Both are levels in dB against the fundamental** |
//! | `transfer` | curve, sticky | 256 | on change | the static transfer curve for input swept −1.5 … +1.5 |
//! | `color` | curve, sticky | 129 | on change | the pre-shaper equaliser's magnitude in dB, 20 Hz … 20 kHz log-spaced |
//! | `latency` | raw, sticky | 5 | on change | `[wet_samples, dry_samples, reported_samples, sample_rate, kernel_frac]` — the first three are always equal, which is the claim |
//! | `spec_in`, `spec_out` | spectrum | 2048 | once per window | magnitudes in dB of the transforms the readout already runs, so they cost no extra transform |
//!
//! ## Real-time rules
//!
//! Everything reachable from [`Processor::process`] runs without allocation,
//! locks or input and output. Parameters are read from atomics into a
//! [`engine::Settings`] snapshot once per block; the engine smooths the continuous
//! ones itself.

pub mod adaa;
pub mod alias;
pub mod color;
pub mod curve;
pub mod engine;
pub mod oversample;
pub mod source;

use noob_vst_webgui_framework::{
    AudioHandle, NoobVstWebguiFramework, ParamSpec, StreamKind, StreamSpec,
};
use serde_json::json;

pub use curve::{CURVE_NAMES, Curve};
pub use engine::{
    CLIP_MODE_NAMES, COLOR_MAX_HZ, COLOR_MIN_HZ, COLOR_POINTS, MIX_LAW_NAMES, Saturator,
    TRANSFER_IN, TRANSFER_POINTS,
};
pub use oversample::FACTOR_NAMES;
pub use source::{SOURCE_NAMES, Source};

/// Values in one `meter` frame.
pub const METER_LEN: usize = 4;
/// Values in one `alias` frame.
pub const ALIAS_LEN: usize = 4;
/// Values in one `latency` frame.
pub const LATENCY_LEN: usize = 5;
/// Bins in one spectrum frame: the positive half of the readout's transform.
pub const SPECTRUM_BINS: usize = alias::WINDOW / 2;
/// Top of the band the aliasing readout measures over.
pub const ALIAS_BAND_HZ: f32 = 10_000.0;
/// Ends of the ceiling stage's knee, in decibels below the ceiling.
pub const CLIP_KNEE_MAX_DB: f32 = 24.0;

/// Ends of the drive control, in dB. Symmetric, so the device can be used
/// as a 36 dB attenuator into a fixed curve as well as a saturator.
pub const DRIVE_MIN_DB: f32 = -36.0;
pub const DRIVE_MAX_DB: f32 = 36.0;
/// Ends of the output trim. Theirs stops at 0; ours does not, because a
/// device that cannot restore the level it took away forces a second device
/// into the chain.
pub const OUTPUT_MIN_DB: f32 = -36.0;
pub const OUTPUT_MAX_DB: f32 = 36.0;
/// Ends of the colour controls, from the target device's serialised ranges.
pub const COLOR_BASE_DB: f32 = 36.0;
pub const COLOR_DEPTH_DB: f32 = 24.0;
/// Ends of the bell's quality. Ours; theirs publishes no unit at all.
pub const COLOR_Q_MIN: f32 = 0.1;
pub const COLOR_Q_MAX: f32 = 10.0;

/// Stream indices, in the order [`streams`] declares them.
#[derive(Clone, Copy, Debug)]
pub struct StreamIx {
    pub meter: usize,
    pub alias: usize,
    pub transfer: usize,
    pub color: usize,
    pub latency: usize,
    pub spec_in: usize,
    pub spec_out: usize,
}

/// The fixed stream layout.
pub const STREAM_IX: StreamIx = StreamIx {
    meter: 0,
    alias: 1,
    transfer: 2,
    color: 3,
    latency: 4,
    spec_in: 5,
    spec_out: 6,
};

/// The streams (see the module docs for the layouts).
pub fn streams(sr: f32) -> Vec<StreamSpec> {
    vec![
        StreamSpec::new("meter", METER_LEN)
            .name("Meter")
            .kind(StreamKind::Meter)
            .meta(json!({
                "layout": "in_peak,out_peak,in_rms,out_rms",
                "unit": "linear",
                "sample_rate": sr
            })),
        StreamSpec::new("alias", ALIAS_LEN)
            .name("Aliasing")
            .kind(StreamKind::Raw)
            .meta(json!({
                "layout": "alias_db,f0_hz,confidence,harmonic_db",
                "units": ["dB", "Hz", "ratio", "dB"],
                "band_hz": ALIAS_BAND_HZ,
                "floor_db": alias::FLOOR_DB,
                "window": alias::WINDOW,
                "mode": "signal",
                "note": "measured on the signal actually passing rather than on a probe tone: f0_hz is the fundamental the meter found. Everything not at a harmonic of it is aliasing; harmonic_db is the wanted distortion from the second order up, against the fundamental. Only meaningful while confidence is near 1, and harmonic_db has nothing to report for a fundamental whose harmonics all lie above Nyquist"
            })),
        StreamSpec::new("transfer", TRANSFER_POINTS)
            .name("Transfer curve")
            .kind(StreamKind::Curve)
            .sticky()
            .meta(json!({
                "in_range": [-TRANSFER_IN, TRANSFER_IN],
                "points": TRANSFER_POINTS,
                "includes": "drive,bias,curve,clip_mode,clip_knee,output",
                "excludes": "colour,mix"
            })),
        StreamSpec::new("color", COLOR_POINTS)
            .name("Colour curve")
            .kind(StreamKind::Curve)
            .sticky()
            .meta(json!({
                "hz_range": [COLOR_MIN_HZ, COLOR_MAX_HZ],
                "points": COLOR_POINTS,
                "unit": "dB",
                "shelf_hz": color::SHELF_HZ,
                "note": "the pre-shaper emphasis; the post-shaper filter is its exact inverse"
            })),
        StreamSpec::new("latency", LATENCY_LEN)
            .name("Dry/wet alignment")
            .kind(StreamKind::Raw)
            .sticky()
            .meta(json!({
                "layout": "wet_samples,dry_samples,reported_samples,sample_rate,kernel_frac",
                "units": ["samples", "samples", "samples", "Hz", "samples"],
                "note": "the first three are always equal, which is the claim; kernel_frac is the antialiasing kernel's own half-sample at the oversampled rate, deliberately excluded from the reported figure because it is fractional and no integer delay can carry it"
            })),
        StreamSpec::new("spec_in", SPECTRUM_BINS)
            .name("Input spectrum")
            .kind(StreamKind::Spectrum)
            .meta(json!({
                "sample_rate": sr,
                "fft_size": alias::WINDOW,
                "db": true,
                "window": "blackman-harris-4"
            })),
        StreamSpec::new("spec_out", SPECTRUM_BINS)
            .name("Output spectrum")
            .kind(StreamKind::Spectrum)
            .meta(json!({
                "sample_rate": sr,
                "fft_size": alias::WINDOW,
                "db": true,
                "window": "blackman-harris-4"
            })),
    ]
}

/// Every parameter (see the module docs). `with_source` adds the
/// standalone's demo-source parameters, which are not automatable.
pub fn param_specs(with_source: bool) -> Vec<ParamSpec> {
    let d = engine::Settings::default();
    let mut v = vec![
        ParamSpec::new("drive", "Drive")
            .range(DRIVE_MIN_DB, DRIVE_MAX_DB)
            .default(d.drive_db)
            .unit("dB")
            .group("shape"),
        // A percentage of the clipping point rather than a unit-free number,
        // for the same reason the colour bell publishes a Q.
        ParamSpec::new("bias", "Bias")
            .range(-100.0, 100.0)
            .default(d.bias * 100.0)
            .unit("%")
            .group("shape"),
        ParamSpec::new("curve", "Curve")
            .labels(CURVE_NAMES)
            .default(d.curve as f32)
            .group("shape"),
        ParamSpec::new("output", "Output")
            .range(OUTPUT_MIN_DB, OUTPUT_MAX_DB)
            .default(d.output_db)
            .unit("dB")
            .group("output"),
        ParamSpec::new("mix", "Dry/Wet")
            .range(0.0, 100.0)
            .default(d.mix * 100.0)
            .unit("%")
            .group("output"),
        ParamSpec::new("mix_law", "Mix Law")
            .labels(MIX_LAW_NAMES)
            .default(d.mix_law as f32)
            .group("output"),
        ParamSpec::new("color_on", "Colour")
            .toggle()
            .default(if d.color.on { 1.0 } else { 0.0 })
            .group("colour"),
        ParamSpec::new("color_base", "Colour Base")
            .range(-COLOR_BASE_DB, COLOR_BASE_DB)
            .default(d.color.base_db)
            .unit("dB")
            .group("colour"),
        ParamSpec::new("color_freq", "Colour Freq")
            .range(color::FREQ_MIN_HZ, color::FREQ_MAX_HZ)
            .log()
            .default(d.color.freq_hz)
            .unit("Hz")
            .group("colour"),
        // Named for the control it answers and united for what it actually
        // is, so the improvement survives outside our own editor: a host's
        // generic parameter view reads "0.70 Q" where theirs reads "0.3".
        ParamSpec::new("color_q", "Colour Width")
            .range(COLOR_Q_MIN, COLOR_Q_MAX)
            .log()
            .default(d.color.q)
            .unit("Q")
            .group("colour"),
        ParamSpec::new("color_depth", "Colour Depth")
            .range(-COLOR_DEPTH_DB, COLOR_DEPTH_DB)
            .default(d.color.depth_db)
            .unit("dB")
            .group("colour"),
        ParamSpec::new("dc_block", "Pre DC Filter")
            .toggle()
            .default(if d.dc_block { 1.0 } else { 0.0 })
            .group("shape"),
        ParamSpec::new("clip_mode", "Post Clip")
            .labels(CLIP_MODE_NAMES)
            .default(d.clip_mode as f32)
            .group("output"),
        ParamSpec::new("clip_knee", "Clip Knee")
            .range(0.0, CLIP_KNEE_MAX_DB)
            .default(d.clip_knee_db)
            .unit("dB")
            .group("output"),
        ParamSpec::new("oversample", "Oversample")
            .labels(FACTOR_NAMES)
            .default(d.oversample as f32)
            .group("quality"),
    ];
    if with_source {
        v.push(
            ParamSpec::new("src_kind", "Source")
                .labels(SOURCE_NAMES)
                .default(0.0)
                .not_automatable()
                .group("source"),
        );
        v.push(
            ParamSpec::new("src_level", "Source Level")
                .range(0.0, 1.0)
                .default(0.5)
                .not_automatable()
                .group("source"),
        );
        v.push(
            ParamSpec::new("src_freq", "Source Freq")
                .range(20.0, 20_000.0)
                .log()
                .default(1000.0)
                .unit("Hz")
                .not_automatable()
                .group("source"),
        );
    }
    v
}

/// Parameter indices, resolved once so the audio thread never looks an id up
/// by string.
#[derive(Clone, Copy, Debug)]
pub struct ParamIx {
    pub drive: usize,
    pub bias: usize,
    pub curve: usize,
    pub output: usize,
    pub mix: usize,
    pub mix_law: usize,
    pub color_on: usize,
    pub color_base: usize,
    pub color_freq: usize,
    pub color_q: usize,
    pub color_depth: usize,
    pub dc_block: usize,
    pub clip_mode: usize,
    pub clip_knee: usize,
    pub oversample: usize,
    pub src_kind: Option<usize>,
    pub src_level: Option<usize>,
    pub src_freq: Option<usize>,
}

/// Resolve the parameter indices by id. Works for the plug-in's mirror,
/// which has no source parameters, as well as the standalone.
pub fn param_index(s: &NoobVstWebguiFramework) -> ParamIx {
    let ix = |id: &str| s.index_of(id).expect(id);
    ParamIx {
        drive: ix("drive"),
        bias: ix("bias"),
        curve: ix("curve"),
        output: ix("output"),
        mix: ix("mix"),
        mix_law: ix("mix_law"),
        color_on: ix("color_on"),
        color_base: ix("color_base"),
        color_freq: ix("color_freq"),
        color_q: ix("color_q"),
        color_depth: ix("color_depth"),
        dc_block: ix("dc_block"),
        clip_mode: ix("clip_mode"),
        clip_knee: ix("clip_knee"),
        oversample: ix("oversample"),
        src_kind: s.index_of("src_kind"),
        src_level: s.index_of("src_level"),
        src_freq: s.index_of("src_freq"),
    }
}

/// Build the standalone's bridge and resolve its parameter indices.
pub fn build_bridge(name: &str, sr: f32) -> (NoobVstWebguiFramework, ParamIx) {
    let mut b = NoobVstWebguiFramework::builder(name)
        .meta(json!({
            "vendor": "Noob Audio Engineering",
            "version": env!("CARGO_PKG_VERSION"),
            "sample_rate": sr,
            "standalone": true,
            "transfer_points": TRANSFER_POINTS,
            "color_points": COLOR_POINTS,
            "dc_corner_hz": engine::DC_HZ,
            "color_shelf_hz": color::SHELF_HZ,
            "alias_band_hz": ALIAS_BAND_HZ,
            "oversample_taps": oversample::TAPS,
        }))
        .params(param_specs(true));
    for s in streams(sr) {
        b = b.stream(s);
    }
    let s = b.build();
    let ix = param_index(&s);
    (s, ix)
}

/// One block's worth of parameter values, read from the atomics.
pub fn read_settings(audio: &AudioHandle, ix: &ParamIx) -> engine::Settings {
    engine::Settings {
        drive_db: audio.param(ix.drive),
        bias: audio.param(ix.bias) / 100.0,
        curve: audio.param(ix.curve).round().clamp(0.0, 5.0) as usize,
        output_db: audio.param(ix.output),
        mix: (audio.param(ix.mix) / 100.0).clamp(0.0, 1.0),
        mix_law: audio.param(ix.mix_law).round().clamp(0.0, 1.0) as usize,
        color: color::Settings {
            on: audio.param(ix.color_on) >= 0.5,
            base_db: audio.param(ix.color_base),
            freq_hz: audio.param(ix.color_freq),
            q: audio.param(ix.color_q),
            depth_db: audio.param(ix.color_depth),
        },
        dc_block: audio.param(ix.dc_block) >= 0.5,
        clip_mode: audio.param(ix.clip_mode).round().clamp(0.0, 2.0) as usize,
        clip_knee_db: audio.param(ix.clip_knee),
        oversample: audio.param(ix.oversample).round().clamp(0.0, 3.0) as usize,
    }
}

/// The engine plus the block-rate telemetry. The plug-in and the standalone
/// drive it the same way: [`configure`](Self::configure) with a fresh
/// snapshot, [`process`](Self::process) the block,
/// [`publish`](Self::publish) the streams.
pub struct Processor {
    engine: Saturator,
    transfer: [f32; TRANSFER_POINTS],
    color_curve: [f32; COLOR_POINTS],
    /// The settings the sticky curves were last drawn for, so a knob that is
    /// not moving does not redraw them.
    drawn: Option<engine::Settings>,
    blocks: u64,
    curves_due: bool,
    /// The alignment frame is sticky and only moves when the factor or the
    /// sample rate does, so it is published on change rather than per block.
    latency_due: bool,
}

impl Processor {
    pub fn new(sr: f32) -> Self {
        Processor {
            engine: Saturator::new(sr),
            transfer: [0.0; TRANSFER_POINTS],
            color_curve: [0.0; COLOR_POINTS],
            drawn: None,
            blocks: 0,
            curves_due: true,
            latency_due: true,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.engine.set_sample_rate(sr);
        self.curves_due = true;
        self.latency_due = true;
    }

    pub fn reset(&mut self) {
        self.engine.reset();
        self.curves_due = true;
        self.latency_due = true;
    }

    pub fn configure(&mut self, s: &engine::Settings) {
        if self.drawn.as_ref() != Some(s) {
            if self.drawn.map(|d| d.oversample) != Some(s.oversample) {
                self.latency_due = true;
            }
            self.drawn = Some(*s);
            self.curves_due = true;
        }
        self.engine.configure(s);
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        self.engine.process(l, r);
    }

    pub fn latency(&self) -> usize {
        self.engine.latency()
    }

    pub fn engine(&self) -> &Saturator {
        &self.engine
    }

    /// Publish the streams after [`process`](Self::process). Real-time safe.
    ///
    /// The two curves are sticky and only redrawn when something they depend
    /// on has moved, and then only on the fourth block, so a swept knob does
    /// not flood the wire.
    pub fn publish(&mut self, audio: &mut AudioHandle) {
        audio.publish_slice(STREAM_IX.meter, &self.engine.meter());
        audio.publish_slice(STREAM_IX.alias, &self.engine.alias_frame());
        if self.latency_due {
            audio.publish_slice(STREAM_IX.latency, &self.engine.latency_frame());
            self.latency_due = false;
        }
        // The spectra are the readout's own transforms read back, so this
        // publishes once per analysis window rather than once per block.
        let (fresh, a, b) = self.engine.take_spectra();
        if fresh {
            audio.publish_slice(STREAM_IX.spec_in, a);
            audio.publish_slice(STREAM_IX.spec_out, b);
        }
        self.blocks += 1;
        if self.curves_due && self.blocks.is_multiple_of(4) {
            let mut t = self.transfer;
            self.engine.transfer(&mut t);
            self.transfer = t;
            let mut c = self.color_curve;
            self.engine.color_curve(&mut c);
            self.color_curve = c;
            audio.publish_slice(STREAM_IX.transfer, &self.transfer);
            audio.publish_slice(STREAM_IX.color, &self.color_curve);
            self.curves_due = false;
        }
    }
}

#[cfg(test)]
mod tests;
