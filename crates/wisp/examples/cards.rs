//! Everything the renderer can do, in one window.
//!
//! Run with `cargo run --example cards`.

use wisp::{
    Background, Border, Corners, DevicePixels, Points, Quad, Rect, Rgba, Shadow, WindowOptions, run,
};

const INK: u32 = 0x0f0f14;
const CARD: u32 = 0x1c1c24;
const EDGE: u32 = 0x33333d;
const ACCENT: u32 = 0xed8c33;
const MINT: u32 = 0x3fb950;

fn main() -> Result<(), winit::error::EventLoopError> {
    run(
        WindowOptions {
            title: "wisp — cards".into(),
            size: (880.0, 560.0),
            clear: Rgba::hex(INK),
            transparent: false,
            ..Default::default()
        },
        |scene, frame| {
            let scale = frame.scale;
            let px = |v: f32| Points(v).to_device(scale);
            let rect = |x: f32, y: f32, w: f32, h: f32| {
                Rect::from_edges(px(x), px(y), px(x + w), px(y + h))
            };
            let shadow = |blur: f32| Shadow {
                offset: (DevicePixels::ZERO, px(6.0)),
                blur: px(blur),
                spread: DevicePixels::ZERO,
                colour: Rgba::hex(0x000000).with_alpha(0.55),
            };
            let edge = Border {
                width: px(1.0),
                colour: Rgba::hex(EDGE),
            };

            // A plain card, to have something to compare the rest against.
            scene.push(
                Quad::new(
                    rect(40.0, 40.0, 240.0, 150.0),
                    Background::Solid(Rgba::hex(CARD)),
                )
                .with_corners(Corners::all(px(12.0)))
                .with_border(edge)
                .with_shadow(shadow(24.0)),
            );

            // A gradient, mixed in Oklab. The two ends are far apart in hue,
            // which is where a channel-wise blend goes muddy in the middle.
            scene.push(
                Quad::new(
                    rect(310.0, 40.0, 240.0, 150.0),
                    Background::LinearGradient {
                        angle: 0.9,
                        stops: vec![
                            (0.0, Rgba::hex(ACCENT)),
                            (0.5, Rgba::hex(0x8c5cff)),
                            (1.0, Rgba::hex(MINT)),
                        ],
                    },
                )
                .with_corners(Corners::all(px(12.0)))
                .with_shadow(shadow(24.0)),
            );

            // Corner radii larger than the box, scaled down together rather
            // than clamped one at a time -- a pill, not a pinched shape.
            scene.push(
                Quad::new(
                    rect(580.0, 40.0, 240.0, 150.0),
                    Background::Solid(Rgba::hex(CARD)),
                )
                .with_corners(Corners::all(px(999.0)))
                .with_border(Border {
                    width: px(2.0),
                    colour: Rgba::hex(ACCENT),
                })
                .with_shadow(shadow(24.0)),
            );

            // A radius per corner.
            scene.push(
                Quad::new(
                    rect(40.0, 220.0, 240.0, 150.0),
                    Background::LinearGradient {
                        angle: std::f32::consts::FRAC_PI_2,
                        stops: vec![(0.0, Rgba::hex(0x2a2a36)), (1.0, Rgba::hex(0x14141b))],
                    },
                )
                .with_corners(Corners {
                    top_left: px(36.0),
                    top_right: px(4.0),
                    bottom_right: px(36.0),
                    bottom_left: px(4.0),
                })
                .with_border(edge),
            );

            // Something moving, drawn at fractional pixels. The point of the
            // library: this slides rather than stepping, because nothing
            // rounds its position on the way to the GPU.
            let sweep = (frame.elapsed * 0.7).sin() * 0.5 + 0.5;
            let x = 310.0 + sweep * 470.0;
            scene.push(
                Quad::new(
                    rect(x, 250.0, 40.0, 40.0),
                    Background::Solid(Rgba::hex(ACCENT)),
                )
                .with_corners(Corners::all(px(20.0)))
                .with_shadow(Shadow {
                    offset: (DevicePixels::ZERO, px(4.0)),
                    blur: px(18.0),
                    spread: DevicePixels::ZERO,
                    colour: Rgba::hex(ACCENT).with_alpha(0.45),
                }),
            );

            // A row of swatches down the bottom: the same colour at falling
            // alpha, over the background, to show the blend is premultiplied.
            for step in 0..12 {
                let t = step as f32 / 11.0;
                scene.push(
                    Quad::new(
                        rect(40.0 + step as f32 * 64.0, 420.0, 56.0, 90.0),
                        Background::Solid(Rgba::hex(MINT).with_alpha(1.0 - t * 0.92)),
                    )
                    .with_corners(Corners::all(px(8.0))),
                );
            }
        },
    )
}
