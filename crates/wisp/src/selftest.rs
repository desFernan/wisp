//! Asking the window server whether the overlay actually behaves.
//!
//! Every property an overlay window claims can be checked by reading back the
//! flag this process just set, and every one of those checks is worthless: it
//! proves the call was made, not that anything happened. The question that
//! matters -- *would a click here reach me?* -- can only be answered by the
//! window server, and its answer crosses application boundaries, so it holds
//! whatever else is on screen.
//!
//! The probe points come from the scene rather than from the caller. The
//! library drew the frame, so it knows where there is something and where
//! there is not, and a check that had to be told would be a check of what the
//! caller believed.

use wisp_core::geometry::Point;
use wisp_core::{DevicePixels, Points, Scale, Scene};
use wisp_overlay::{MouseStep, Overlay};

/// A point with something drawn on it and a point with nothing, in window
/// points, or `None` if the frame is all one or the other.
fn probes(scene: &Scene, size: (f32, f32), scale: Scale) -> Option<((f32, f32), (f32, f32))> {
    const STEPS: u32 = 48;
    let mut solid = None;
    let mut clear = None;
    for row in 0..STEPS {
        for column in 0..STEPS {
            let at = (
                size.0 * (column as f32 + 0.5) / STEPS as f32,
                size.1 * (row as f32 + 0.5) / STEPS as f32,
            );
            let device = Point::new(
                DevicePixels(at.0 * scale.factor()),
                DevicePixels(at.1 * scale.factor()),
            );
            if scene.covers(device, wisp_overlay::SOLID) {
                // The middle of the drawn thing rather than its edge: an edge
                // is a pixel wide and a warp lands on whole pixels.
                solid = Some(at);
            } else if clear.is_none() {
                clear = Some(at);
            }
        }
    }
    Some((solid?, clear?))
}

/// The checks, run across frames rather than in one go.
///
/// Whether a click reaches this window depends on a flag the window sets from
/// where the pointer is, and it sets it while drawing. So moving the pointer
/// and asking in the same breath asks about the frame *before* the move: the
/// window still thinks the pointer is where it was. Each step here moves or
/// asks, and then lets some frames go by.
#[derive(Default)]
pub struct Checker {
    step: u32,
    at: u32,
    passed: bool,
    solid: (f32, f32),
    clear: (f32, f32),
}

/// Frames between one step and the next. Six is a tenth of a second on a sixty
/// hertz display, and the window server honours a change to
/// `ignoresMouseEvents` about a refresh after it is made.
const SETTLE: u32 = 6;

impl Checker {
    /// Advances one frame. `Some(passed)` when there is nothing left to check.
    pub fn step(
        &mut self,
        overlay: &Overlay,
        scene: &Scene,
        size: (f32, f32),
        scale: Scale,
    ) -> Option<bool> {
        self.at += 1;
        if self.at < SETTLE {
            return None;
        }
        self.at = 0;

        let frame = overlay.frame()?;
        let on_screen = |at: (f32, f32)| {
            (
                Points(frame.left().get() + at.0),
                Points(frame.top().get() + at.1),
            )
        };
        let reached =
            |at: (f32, f32)| overlay.window_under(on_screen(at)) == Some(overlay.number());

        match self.step {
            0 => {
                println!(
                    "=== wisp overlay selftest (window {}) ===",
                    overlay.number()
                );
                match probes(scene, size, scale) {
                    Some((solid, clear)) => {
                        println!("1. the frame has something and nothing on it -> PASS");
                        self.solid = solid;
                        self.clear = clear;
                        self.passed = true;
                        overlay.warp_cursor(on_screen(solid));
                    }
                    None => {
                        println!(
                            "1. the frame has something and nothing on it -> FAIL (it does not)"
                        );
                        return Some(false);
                    }
                }
            }
            1 => {
                let ok = reached(self.solid);
                self.passed &= ok;
                println!(
                    "2. a click where something is drawn reaches this window -> {} (reached={ok})",
                    if ok { "PASS" } else { "FAIL" }
                );
                overlay.warp_cursor(on_screen(self.clear));
            }
            2 => {
                let through = !reached(self.clear);
                self.passed &= through;
                println!(
                    "3. a click where nothing is drawn passes through       -> {} (reached={})",
                    if through { "PASS" } else { "FAIL" },
                    !through
                );
                if !crate::can_post_events() {
                    println!(
                        "4. a held button is visible without the window -> SKIP (not trusted \n   \
                         for Accessibility, so this process cannot press one to find out)"
                    );
                    return Some(self.finish());
                }
                crate::post_mouse(
                    on_screen(self.clear).0,
                    on_screen(self.clear).1,
                    MouseStep::Down,
                );
            }
            3 => {
                // The press went to whatever is under a see-through pixel, so
                // this window was never told about it. That is the point: a
                // window that moves out from under a gesture still has to know
                // the gesture is running.
                let held = crate::mouse_is_down();
                self.passed &= held;
                println!(
                    "4. a held button is visible without the window        -> {} (held={held})",
                    if held { "PASS" } else { "FAIL" }
                );
                crate::post_mouse(
                    on_screen(self.clear).0,
                    on_screen(self.clear).1,
                    MouseStep::Up,
                );
            }
            4 => {
                let let_go = !crate::mouse_is_down();
                self.passed &= let_go;
                println!(
                    "5. and letting go is visible too                      -> {} (held={})",
                    if let_go { "PASS" } else { "FAIL" },
                    !let_go
                );
                return Some(self.finish());
            }
            _ => return Some(self.passed),
        }
        self.step += 1;
        None
    }

    fn finish(&self) -> bool {
        println!(
            "=== {} ===\n",
            if self.passed {
                "all checks passed"
            } else {
                "SOME CHECKS FAILED"
            }
        );
        self.passed
    }
}
