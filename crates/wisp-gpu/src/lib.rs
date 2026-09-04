//! Drawing a [`Scene`] with wgpu.
//!
//! One pipeline and one primitive. A rounded rectangle with a border and a
//! shadow is a single signed-distance field evaluated once, not three things
//! drawn on top of each other -- which is where the seam between a box and its
//! border comes from, and why a shadow drawn as its own quad has a visible
//! edge where it meets the thing casting it.
//!
//! Gradients are not re-implemented here. Each one in a frame is baked into a
//! row of a lookup texture by [`wisp_core::Background::sample`], which mixes in
//! Oklab and is tested; the shader reads that row. Colour arithmetic living in
//! two places is colour arithmetic that disagrees in one of them.

mod renderer;

pub use renderer::{Renderer, Surface};

/// How many samples across each gradient is baked at.
///
/// A gradient is a smooth ramp and the texture is filtered, so this is about
/// banding rather than about resolution. 256 is one byte's worth of steps per
/// channel, which is the point past which more samples cannot show.
pub(crate) const GRADIENT_SAMPLES: u32 = 256;
