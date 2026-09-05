use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSScreen, NSView, NSWindow, NSWindowStyleMask};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use wisp_core::Points;
use wisp_core::geometry::Rect;

/// A window, in the terms this library needs it in.
///
/// Everything here is in **points**, with the origin at the top left of the
/// primary display and y increasing downwards. Cocoa counts upwards from the
/// bottom of that display, and every conversion in this file is that one
/// subtraction written once -- doing it twice, or forgetting it in one place,
/// is a window that lands mirrored about the middle of the screen.
pub struct Overlay {
    window: Retained<NSWindow>,
    click_through: bool,
}

impl Overlay {
    /// Takes over a window the toolkit has already made.
    ///
    /// `None` when this is not an AppKit window, which on macOS means the
    /// caller has been handed something it did not expect rather than that
    /// anything has gone wrong.
    pub fn adopt(window: &impl HasWindowHandle) -> Option<Self> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        // Safety: the toolkit owns this view and keeps it alive for as long as
        // the window it belongs to.
        let view: Retained<NSView> =
            unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>())? };
        let window = view.window()?;
        Some(Self {
            window,
            click_through: false,
        })
    }

    /// Borderless, transparent, shadowless.
    ///
    /// The style mask is set directly rather than asked for at creation. A
    /// borderless window that was made with a title bar keeps a resize border
    /// and a shadow, and a shadow around a transparent window is a grey
    /// rectangle floating over the desktop with nothing in it.
    pub fn make_bare(&self) {
        self.window.setStyleMask(NSWindowStyleMask::Borderless);
        self.window.setOpaque(false);
        self.window.setHasShadow(false);
        // Moving with its content means moving often, and a window that
        // animates itself to each new position lags a frame behind the thing
        // it is following.
        self.window.setMovableByWindowBackground(false);
    }

    /// Above ordinary windows, below the menu bar's own panels.
    ///
    /// 101 is `NSPopUpMenuWindowLevel`. High enough to sit over a full-screen
    /// editor, low enough not to cover a menu somebody has opened.
    pub fn keep_on_top(&self) {
        self.window.setLevel(101);
    }

    /// Whether clicks pass through to whatever is underneath.
    ///
    /// Set from the scene every frame. The window server honours it about a
    /// refresh later, which is why nothing here should read it back and expect
    /// an answer immediately.
    pub fn set_click_through(&mut self, through: bool) {
        if self.click_through != through {
            self.click_through = through;
            self.window.setIgnoresMouseEvents(through);
        }
    }

    pub fn is_click_through(&self) -> bool {
        self.click_through
    }

    /// The window's number, for `screencapture -l` and for asking the window
    /// server what a click at a point would actually hit.
    pub fn number(&self) -> isize {
        self.window.windowNumber()
    }

    /// Where the pointer is, in this library's coordinates.
    ///
    /// Asked of the window server rather than taken from a mouse event.
    /// An event's position is relative to the window, and an overlay's window
    /// moves underneath the gesture -- an offset worked out in one frame and
    /// applied in the next puts the thing being dragged a long way from the
    /// cursor.
    pub fn cursor(&self) -> Option<(Points, Points)> {
        let mtm = MainThreadMarker::new()?;
        let height = primary_height(mtm)?;
        let at = NSEvent::mouseLocation();
        Some((Points(at.x as f32), Points((height - at.y) as f32)))
    }

    /// Where the window is.
    pub fn frame(&self) -> Option<Rect<Points>> {
        let mtm = MainThreadMarker::new()?;
        let height = primary_height(mtm)?;
        let frame = self.window.frame();
        let top = height - (frame.origin.y + frame.size.height);
        Some(Rect::from_edges(
            Points(frame.origin.x as f32),
            Points(top as f32),
            Points((frame.origin.x + frame.size.width) as f32),
            Points((top + frame.size.height) as f32),
        ))
    }

    /// Moves and resizes the window.
    ///
    /// **Never during a gesture.** Changing a window while a button is held
    /// takes the rest of the gesture with it: the press arrives and nothing
    /// after it does, so whatever was picked up sits still while the cursor
    /// walks away from it and is never let go. Whatever size a drag will need,
    /// the window has to already be.
    ///
    /// Redrawn in the same call, because the contents' position *inside* the
    /// window changes by the same amount in the opposite direction on the same
    /// frame; left to redraw on its own schedule the two land in different
    /// frames and the content jumps.
    pub fn set_frame(&self, at: Rect<Points>) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(height) = primary_height(mtm) else {
            return;
        };
        let bottom = height - at.bottom().get() as f64;
        let frame = NSRect::new(
            NSPoint::new(at.left().get() as f64, bottom),
            NSSize::new(
                at.size.width.get().max(1.0) as f64,
                at.size.height.get().max(1.0) as f64,
            ),
        );
        self.window.setFrame_display(frame, true);
    }

    /// The usable area of the display a point is on -- the screen less the
    /// menu bar and the Dock.
    ///
    /// Of the display the point is on, not of "the main screen". `mainScreen`
    /// is the screen holding the key window, so it changes every time somebody
    /// clicks something on the other monitor; a character standing on a floor
    /// derived from it has the floor teleport out from under it.
    pub fn work_area_containing(&self, point: (Points, Points)) -> Option<Rect<Points>> {
        let mtm = MainThreadMarker::new()?;
        let height = primary_height(mtm)?;
        let screen = NSScreen::screens(mtm)
            .iter()
            .find(|screen| {
                let frame = screen.frame();
                let top = height - (frame.origin.y + frame.size.height);
                let bottom = height - frame.origin.y;
                (frame.origin.x..frame.origin.x + frame.size.width)
                    .contains(&(point.0.get() as f64))
                    && (top..bottom).contains(&(point.1.get() as f64))
            })
            .or_else(|| NSScreen::mainScreen(mtm))?;
        let visible = screen.visibleFrame();
        let top = height - (visible.origin.y + visible.size.height);
        Some(Rect::from_edges(
            Points(visible.origin.x as f32),
            Points(top as f32),
            Points((visible.origin.x + visible.size.width) as f32),
            Points((top + visible.size.height) as f32),
        ))
    }

    /// Which window a click at this point would actually reach.
    ///
    /// The only evidence that counts for click-through, because it crosses
    /// application boundaries: it is the window server's own answer rather
    /// than a flag this process set and hopes was honoured.
    pub fn window_under(&self, at: (Points, Points)) -> Option<isize> {
        let mtm = MainThreadMarker::new()?;
        let height = primary_height(mtm)?;
        let point = NSPoint::new(at.0.get() as f64, height - at.1.get() as f64);
        let number = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, 0, mtm);
        (number != 0).then_some(number)
    }

    /// Puts the pointer somewhere, and waits until it is actually there.
    ///
    /// Only for checking the window's own behaviour. A warp is asynchronous --
    /// the position it reports and the position the window server has can
    /// differ for a few milliseconds -- and a fixed sleep instead of this wait
    /// reads a stale position often enough to make a check lie.
    pub fn warp_cursor(&self, to: (Points, Points)) -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(height) = primary_height(mtm) else {
            return false;
        };
        objc2_core_graphics::CGWarpMouseCursorPosition(objc2_core_foundation::CGPoint {
            x: to.0.get() as f64,
            y: to.1.get() as f64,
        });
        for _ in 0..200 {
            let at = NSEvent::mouseLocation();
            let now = (at.x as f32, (height - at.y) as f32);
            if (now.0 - to.0.get()).abs() < 1.0 && (now.1 - to.1.get()).abs() < 1.0 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    /// The window as an opaque pointer, for anything this does not wrap.
    pub fn as_ptr(&self) -> *mut AnyObject {
        Retained::as_ptr(&self.window) as *mut AnyObject
    }
}

/// One step of a mouse gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseStep {
    Down,
    Move,
    Up,
}

/// Posts one real mouse event at a point on the screen.
///
/// One event per call, and never a whole gesture in one: posting a gesture
/// from the thread that draws blocks the loop for its duration, and every
/// event is then handled in a burst afterwards -- which measures the event
/// queue rather than the window, and reports a drag that moved nothing.
///
/// For checking a window's own behaviour. Nothing an application does in
/// normal use should be posting input to itself.
pub fn post_mouse(x: Points, y: Points, step: MouseStep) -> bool {
    use objc2_core_graphics::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};

    let point = objc2_core_foundation::CGPoint {
        x: x.get() as f64,
        y: y.get() as f64,
    };
    let kind = match step {
        MouseStep::Down => CGEventType::LeftMouseDown,
        MouseStep::Move => CGEventType::LeftMouseDragged,
        MouseStep::Up => CGEventType::LeftMouseUp,
    };
    let Some(event) = CGEvent::new_mouse_event(None, kind, point, CGMouseButton::Left) else {
        return false;
    };
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    true
}

/// Whether this process may post synthetic input at all.
///
/// `CGEventPost` needs Accessibility, and a process without it has its events
/// dropped in silence -- no error, no delivery. A check run in that state
/// fails for the wrong reason, and its opposite passes for the wrong reason.
pub fn can_post_events() -> bool {
    objc2_core_graphics::CGPreflightPostEventAccess()
}

/// The height of the primary display, which every Cocoa coordinate is measured
/// from.
fn primary_height(mtm: MainThreadMarker) -> Option<f64> {
    NSScreen::screens(mtm)
        .iter()
        .next()
        .map(|screen| screen.frame().size.height)
}
