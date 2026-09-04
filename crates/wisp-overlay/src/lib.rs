//! Windows that are not rectangles somebody gave you.
//!
//! The case this library is named for: a window that is transparent, sits over
//! everything, lets clicks through every pixel its content is not drawn on,
//! and moves with that content. A desktop character, a heads-up display over a
//! game, a floating palette, a notification that is not a grey box.
//!
//! Every toolkit can be talked into the first two. The third is what none of
//! them do, because it needs the toolkit to know what it drew: "is there any
//! of me under the pointer" is a question about this frame's scene, asked
//! sixty times a second, and answered by [`wisp_core::Scene::covers`].
//!
//! What is here is the platform underneath that. It is macOS only so far, and
//! the rest of the library does not depend on it.

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
pub use mac::Overlay;

/// How solid a pixel has to be before it catches a click.
///
/// Not zero. Antialiasing leaves a fringe of nearly-transparent pixels around
/// everything, and a window that caught clicks on those has a halo of dead
/// space around every letter that nobody can see and everybody notices.
pub const SOLID: f32 = 0.15;

#[cfg(not(target_os = "macos"))]
compile_error!(
    "wisp-overlay has only been written for macOS. \
     The rest of wisp builds everywhere; this crate is opt-in."
);
