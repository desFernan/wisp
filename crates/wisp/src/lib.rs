//! A window, and a frame loop that draws into it.
//!
//! The umbrella crate: it re-exports [`wisp_core`]'s types so that a user of
//! the library needs one dependency, and adds the part that has to touch a
//! platform.

mod window;

pub use window::{Frame, WindowOptions, run};
pub use wisp_core::colour::Rgba;
pub use wisp_core::geometry::{Point, Rect, Size};
pub use wisp_core::scene::{Background, Border, Corners, Quad, Scene, Shadow};
pub use wisp_core::units::{DevicePixels, Points, Scale};
