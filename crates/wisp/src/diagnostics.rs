//! Being able to look at the window from outside the process.
//!
//! A user interface library is judged by what it draws, and the only honest
//! way to check that is a screenshot. macOS can capture one window by its
//! number without bringing it to the front -- `screencapture -l <number>` --
//! which means a window can be looked at while somebody else is using their
//! machine, and while it is behind three other things.
//!
//! Raising the window instead works exactly once, on an idle machine, and
//! wastes everybody's time on any other kind.
//!
//! Set `WISP_WINDOW_ID` and the number is printed on the first frame.

/// Prints the platform's window number, once, if `WISP_WINDOW_ID` is set.
#[cfg(target_os = "macos")]
pub fn announce_window_id(window: &winit::window::Window) {
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if std::env::var_os("WISP_WINDOW_ID").is_none() {
        return;
    }
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // Safety: winit hands out a pointer to the view backing this window and
    // keeps it alive for as long as the window is.
    let view: Retained<NSView> = unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) }
        .expect("winit's view is not null");
    if let Some(native) = view.window() {
        println!("wisp: window {}", native.windowNumber());
    }
}

#[cfg(not(target_os = "macos"))]
pub fn announce_window_id(_window: &winit::window::Window) {}
