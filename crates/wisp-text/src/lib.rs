//! Turning a string into positioned glyphs, and glyphs into an atlas.
//!
//! Nothing here is a shaper. Shaping is the part of text that looks simple
//! from the outside and is not -- ligatures, marks, Indic reordering, bidi,
//! fallback for a character the chosen font has never heard of -- and every
//! toolkit that has tried to write its own has either abandoned it or become a
//! text library that also draws rectangles. `cosmic-text` is used for that,
//! and for finding fonts on the system, and `swash` underneath it for turning
//! an outline into coverage.
//!
//! What is here is the part specific to drawing: an atlas that glyphs are
//! packed into once and read from every frame, and the arithmetic that puts
//! them in the right place at fractional positions.

mod atlas;
mod layout;

pub use atlas::{Atlas, AtlasSlot};
pub use layout::{Align, Font, TextSystem, Weight};
