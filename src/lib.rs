//! Noob Saturator: a waveshaper that does not alias, built on
//! noob-vst-webgui-framework with a browser-rendered front panel.
//!
//! It is an affectionate spoof of Ableton Live's Saturator, and it is not a
//! parity replacement or a clone. It stands on four things, all of them
//! demonstrable here rather than matters of taste: **aliasing measured and
//! stated on the face**, a **dry/wet control that sums without combing**
//! because its dry path is delayed to match the wet one, **quality always on**
//! rather than hidden behind a switch, and **latency reported honestly** to
//! the host and compensated internally. All four are properties of this
//! device, measured on this device; none of them is a comparison, because
//! there is no measurement of anyone else's to compare against.
//!
//! The curves are ours. We do not copy theirs — they are unverifiable, they
//! are not modelled on anything, and "sounds like Saturator" is a taste
//! claim rather than a measurement. Every curve here ships with its transfer
//! function and its antiderivative printed, which no Ableton document does
//! for any device.
//!
//! ## What may and may not be claimed
//!
//! **Nobody has measured Ableton's Saturator** — not us, not the survey
//! behind this build, not any third party we could find. So this plug-in
//! claims that it meets **its own** aliasing target, cites Ableton's manual
//! admitting the defect in their own words, and cites their factory preset
//! library shipping the mitigation disabled. It does not claim a margin over
//! them in decibels, anywhere, and the reason is simply that no such
//! measurement exists to support one.
//!
//! On the preset question the fairest reading is the evidenced one and it is
//! not negligence: silently holding a legacy device to its old behaviour is
//! a defensible engineering choice, and Ableton's own manual records doing
//! exactly that for an equaliser's pre-Live-9 shelf shape, "to ensure that
//! older Sets sound exactly the same". What none of the readings changes is
//! the outcome, and that is the only part asserted here: reach for the
//! factory library, pick a preset, and the aliasing device runs with nothing
//! on the panel saying so.
//!
//! ## Layers
//!
//! | layer | path | role |
//! |---|---|---|
//! | DSP | [`dsp`] | the curves, the antialiasing, the resampler, the colour section, the readout |
//! | plug-in | `plugin` (feature `plugin`) | nih-plug VST3 / CLAP effect whose editor is the OS web view |
//! | standalone | `src/bin/standalone.rs` | a dev server with a fake audio thread and demo sources |
//! | benchmark | `src/bin/benchmark.rs` | regenerates `docs/BENCHMARK.md` |
//! | page | `web/` | the Vue and Tailwind front panel |
//!
//! Where the framework ends: everything here is specific to this device. The
//! bridge, server, parameter mirroring, host adapter, browser client,
//! gestures and charts come from noob-vst-webgui-framework, which holds
//! generics only and no look of its own.

// The oversampled inner loops index several buffers by the same sample
// index; iterator chains would hide the arithmetic the comments describe.
#![allow(clippy::needless_range_loop)]

pub mod dsp;

#[cfg(feature = "plugin")]
pub mod plugin;

// The VST3 and CLAP entry points. nih-plug generates the C ABI exports from
// the `Plugin` / `Vst3Plugin` / `ClapPlugin` impls in `plugin.rs`.
#[cfg(feature = "plugin")]
nih_plug::nih_export_vst3!(plugin::NoobSaturator);
#[cfg(feature = "plugin")]
nih_plug::nih_export_clap!(plugin::NoobSaturator);
