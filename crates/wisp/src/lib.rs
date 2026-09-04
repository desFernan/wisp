//! A window, and a frame loop that draws into it.
//!
//! The umbrella crate: it re-exports [`wisp_core`]'s types so that a user of
//! the library needs one dependency, and adds the part that has to touch a
//! platform.

mod diagnostics;
#[cfg(target_os = "macos")]
mod selftest;
#[cfg(feature = "snapshot")]
pub mod snapshot;
mod window;

pub use window::{Frame, WindowOptions, run};
pub use wisp_core::colour::Rgba;
pub use wisp_core::geometry::{Point, Rect, Size};
pub use wisp_core::scene::{Background, Border, Corners, Masked, Quad, Scene, Shadow};
pub use wisp_core::units::{DevicePixels, Points, Scale};
pub use wisp_text::{Align, Font, TextSystem, Weight};
pub use wisp_ui::element::{
    Axis, Edges, Element, Place, Sizing, column, div, picture, row, spacer, text,
};
pub use wisp_ui::ui::{Edited, OnEnter};
pub use wisp_ui::{Editor, Elevation, Interactions, Picture, Pointer, Role, Theme, Ui};
