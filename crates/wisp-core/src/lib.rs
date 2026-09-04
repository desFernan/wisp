//! What wisp draws, described without reference to how.
//!
//! This crate has no GPU in it and no platform: it is geometry, colour and a
//! display list, and it is where nearly all of the library's tests live.
//! Everything below it is a renderer for one of these, and everything above it
//! builds one.

pub mod colour;
pub mod geometry;
pub mod scene;
pub mod units;

pub use colour::Rgba;
pub use geometry::{Point, Rect, Size};
pub use scene::{Background, Border, Corners, Masked, Quad, Scene, Shadow, Textured};
pub use units::{DevicePixels, Points, Scale};
