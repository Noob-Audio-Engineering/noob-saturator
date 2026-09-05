//! The plug-in outside a DAW, so the real editor can be watched locally.
//! Not shipped; see `noob-resonator/src/bin/host.rs`.

fn main() {
    nih_plug::prelude::nih_export_standalone::<noob_saturator::plugin::NoobSaturator>();
}
