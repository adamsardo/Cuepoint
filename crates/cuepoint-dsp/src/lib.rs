//! DSP building blocks: time-stretch, key shift, filters and effects.
//!
//! Everything here is real-time safe: no allocation, locking or I/O once
//! constructed.

mod biquad;
mod curves;
mod interp;
mod smooth;

pub use biquad::{Biquad, BiquadCoeffs};
pub use curves::{constant_power_crossfade, db_to_gain, fader_gain};
pub use interp::hermite;
pub use smooth::Ramp;
