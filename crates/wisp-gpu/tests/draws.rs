//! Renders scenes to a texture and reads the pixels back.
//!
//! WGSL is compiled by the driver at run time and the layout of a storage
//! buffer is agreed by convention rather than by the compiler, so neither is
//! checked by `cargo build`. A shader with a typo in it, or a struct whose two
//! halves have drifted apart, builds perfectly and draws the wrong thing. The
//! only way to know is to draw and look, which is what this does.

use wisp_core::geometry::Rect;
use wisp_core::scene::{Background, Border, Corners, Masked, Quad, Shadow};
use wisp_core::{DevicePixels, Rgba, Scene};
use wisp_gpu::Renderer;

const SIZE: u32 = 64;
/// The texture is read back as raw bytes and wgpu wants each row aligned.
const ROW_ALIGN: u32 = 256;

pub struct Harness {
    renderer: Renderer,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
}

/// `None` when this machine has no adapter wgpu can use, which is the normal
/// state of a headless CI runner. Skipping is honest; failing would report a
/// missing GPU as a broken shader.
fn harness() -> Option<Harness> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (ROW_ALIGN * SIZE) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    Some(Harness {
        renderer: Renderer::new(device, queue, format),
        target,
        readback,
    })
}

impl Harness {
    /// Draws the scene and returns the frame as rows of RGBA bytes.
    fn draw(&mut self, scene: &Scene) -> Vec<[u8; 4]> {
        let view = self.target.create_view(&Default::default());
        self.renderer
            .draw(scene, &view, (SIZE, SIZE), Rgba::TRANSPARENT);

        let device = self.renderer.device();
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_ALIGN),
                    rows_per_image: Some(SIZE),
                },
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
        self.renderer.queue().submit([encoder.finish()]);

        let slice = self.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("the queue drains");

        // The view has to be dropped before the unmap, which is what the
        // block is for -- `BufferSlice` is Copy, so dropping that does nothing.
        let bytes = {
            let view = slice.get_mapped_range().expect("the buffer maps");
            view.to_vec()
        };
        self.readback.unmap();

        let mut pixels = Vec::with_capacity((SIZE * SIZE) as usize);
        for y in 0..SIZE {
            let row = (y * ROW_ALIGN) as usize;
            for x in 0..SIZE {
                let at = row + (x * 4) as usize;
                pixels.push([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            }
        }
        pixels
    }
}

fn at(pixels: &[[u8; 4]], x: u32, y: u32) -> [u8; 4] {
    pixels[(y * SIZE + x) as usize]
}

fn px(v: f32) -> DevicePixels {
    DevicePixels(v)
}

fn box_at(l: f32, t: f32, r: f32, b: f32) -> Rect<DevicePixels> {
    Rect::from_edges(px(l), px(t), px(r), px(b))
}

/// Within a few steps, which is what a colour survives a round trip through an
/// sRGB texture with.
fn close(got: [u8; 4], want: [u8; 4]) -> bool {
    got.iter()
        .zip(want)
        .all(|(a, b)| (*a as i16 - b as i16).abs() <= 4)
}

macro_rules! gpu_test {
    ($name:ident, $harness:ident, $body:block) => {
        #[test]
        fn $name() {
            let Some(mut $harness) = harness() else {
                eprintln!("no usable GPU adapter; skipping");
                return;
            };
            $body
        }
    };
}

gpu_test!(a_solid_quad_lands_where_it_was_asked_to, h, {
    let mut scene = Scene::new();
    scene.push(Quad::new(
        box_at(16.0, 16.0, 48.0, 48.0),
        Background::Solid(Rgba::hex(0xff0000)),
    ));
    let pixels = h.draw(&scene);

    assert!(
        close(at(&pixels, 32, 32), [255, 0, 0, 255]),
        "middle: {:?}",
        at(&pixels, 32, 32)
    );
    // Outside it, the clear colour shows through. This is what a transparent
    // window depends on and it is the easiest thing to lose to a blend mode.
    assert_eq!(
        at(&pixels, 4, 4)[3],
        0,
        "outside the quad should stay clear"
    );
    // Just inside each edge, so a quad drawn a pixel off is caught.
    assert!(close(at(&pixels, 17, 32), [255, 0, 0, 255]), "left edge");
    assert!(close(at(&pixels, 46, 32), [255, 0, 0, 255]), "right edge");
    assert_eq!(at(&pixels, 14, 32)[3], 0, "one pixel outside the left edge");
});

gpu_test!(a_mid_tone_comes_out_the_colour_it_was_asked_for, h, {
    // The test the first eight did not make. They were all written with pure
    // colours -- 0x00 and 0xff -- which are the fixed points of the sRGB
    // transfer function, so they pass whether or not the colours are
    // linearised on the way to the GPU. Everything in between does not: the
    // window rendered every mid-tone about twice as light as it should, and
    // the whole suite stayed green.
    let mut scene = Scene::new();
    scene.push(Quad::new(
        box_at(8.0, 8.0, 56.0, 56.0),
        Background::Solid(Rgba::hex(0x808080)),
    ));
    let pixels = h.draw(&scene);
    assert!(
        close(at(&pixels, 32, 32), [128, 128, 128, 255]),
        "{:?}",
        at(&pixels, 32, 32)
    );
});

gpu_test!(a_rounded_corner_is_cut_away_and_the_middle_is_not, h, {
    let mut scene = Scene::new();
    scene.push(
        Quad::new(
            box_at(8.0, 8.0, 56.0, 56.0),
            Background::Solid(Rgba::hex(0x00ff00)),
        )
        .with_corners(Corners::all(px(16.0))),
    );
    let pixels = h.draw(&scene);

    assert!(
        close(at(&pixels, 32, 32), [0, 255, 0, 255]),
        "the middle is filled"
    );
    // The very corner of the bounding box is outside a 16px radius.
    assert_eq!(at(&pixels, 9, 9)[3], 0, "the corner is rounded away");
    // Halfway along an edge is not.
    assert!(
        at(&pixels, 32, 9)[3] > 200,
        "the middle of an edge is not cut"
    );
});

gpu_test!(a_radius_larger_than_the_box_does_not_pinch_it, h, {
    // `Corners::fitted_to` scales overlapping radii down together. If the
    // shader got the raw values it would draw a shape with a waist.
    let mut scene = Scene::new();
    scene.push(
        Quad::new(
            box_at(16.0, 16.0, 48.0, 48.0),
            Background::Solid(Rgba::hex(0xffffff)),
        )
        .with_corners(Corners::all(px(999.0))),
    );
    let pixels = h.draw(&scene);
    // A circle inscribed in the square: the centre is filled and the corners
    // are not, but the middle of each edge is still on the shape.
    assert!(
        at(&pixels, 32, 32)[3] > 200,
        "the middle of a circle is filled"
    );
    assert!(
        at(&pixels, 32, 17)[3] > 128,
        "the top of the circle is on the shape"
    );
    assert_eq!(at(&pixels, 17, 17)[3], 0, "the corner is not");
});

gpu_test!(a_border_is_drawn_inside_the_shape_it_belongs_to, h, {
    let mut scene = Scene::new();
    scene.push(
        Quad::new(
            box_at(16.0, 16.0, 48.0, 48.0),
            Background::Solid(Rgba::hex(0x000000)),
        )
        .with_border(Border {
            width: px(4.0),
            colour: Rgba::hex(0xff0000),
        }),
    );
    let pixels = h.draw(&scene);

    assert!(
        close(at(&pixels, 18, 32), [255, 0, 0, 255]),
        "on the border: {:?}",
        at(&pixels, 18, 32)
    );
    assert!(
        close(at(&pixels, 32, 32), [0, 0, 0, 255]),
        "inside the border"
    );
    // A border must not grow the quad: outside its bounds is still clear.
    assert_eq!(
        at(&pixels, 14, 32)[3],
        0,
        "the border stayed inside the bounds"
    );
});

gpu_test!(a_gradient_runs_the_way_it_was_pointed, h, {
    let mut scene = Scene::new();
    scene.push(Quad::new(
        box_at(0.0, 0.0, 64.0, 64.0),
        Background::LinearGradient {
            angle: 0.0, // left to right
            stops: vec![(0.0, Rgba::hex(0x000000)), (1.0, Rgba::hex(0xffffff))],
        },
    ));
    let pixels = h.draw(&scene);

    let left = at(&pixels, 1, 32)[0];
    let middle = at(&pixels, 32, 32)[0];
    let right = at(&pixels, 62, 32)[0];
    assert!(
        left < middle && middle < right,
        "not a ramp: {left} {middle} {right}"
    );
    // And it is a ramp across, not down.
    assert_eq!(
        at(&pixels, 32, 4)[0],
        at(&pixels, 32, 60)[0],
        "the gradient tilted"
    );
});

gpu_test!(a_shadow_falls_outside_the_thing_casting_it, h, {
    let mut scene = Scene::new();
    scene.push(
        Quad::new(
            box_at(20.0, 20.0, 44.0, 44.0),
            Background::Solid(Rgba::hex(0xffffff)),
        )
        .with_shadow(Shadow {
            offset: (px(0.0), px(0.0)),
            blur: px(6.0),
            spread: px(0.0),
            colour: Rgba::hex(0x000000).with_alpha(1.0),
        }),
    );
    let pixels = h.draw(&scene);

    // Just outside the quad there is shadow, and it fades with distance.
    let near = at(&pixels, 32, 17)[3];
    let far = at(&pixels, 32, 10)[3];
    assert!(near > 0, "no shadow next to the quad");
    assert!(
        near > far,
        "the shadow does not fade: near {near}, far {far}"
    );
    // The quad itself is not darkened by its own shadow.
    assert!(
        close(at(&pixels, 32, 32), [255, 255, 255, 255]),
        "{:?}",
        at(&pixels, 32, 32)
    );
});

gpu_test!(an_empty_scene_draws_nothing_at_all, h, {
    let pixels = h.draw(&Scene::new());
    assert!(
        pixels.iter().all(|p| p[3] == 0),
        "something was drawn into an empty frame"
    );
});

gpu_test!(quads_are_drawn_in_the_order_they_were_added, h, {
    let mut scene = Scene::new();
    scene.push(Quad::new(
        box_at(8.0, 8.0, 56.0, 56.0),
        Background::Solid(Rgba::hex(0xff0000)),
    ));
    scene.push(Quad::new(
        box_at(8.0, 8.0, 56.0, 56.0),
        Background::Solid(Rgba::hex(0x0000ff)),
    ));
    let pixels = h.draw(&scene);
    assert!(
        close(at(&pixels, 32, 32), [0, 0, 255, 255]),
        "the later quad should be on top"
    );
});

gpu_test!(a_mask_is_drawn_in_the_colour_it_was_given, h, {
    // The coverage atlas holds a shape, not a picture: one channel saying how
    // much of each pixel the shape covers. The colour comes from the item, so
    // one cached glyph can serve text in every colour in a window.
    //
    // A four by four atlas, opaque on the left half and empty on the right.
    let side = 4u32;
    let mut atlas = vec![0u8; (side * side) as usize];
    for row in 0..side {
        for column in 0..side / 2 {
            atlas[(row * side + column) as usize] = 255;
        }
    }
    h.renderer
        .upload_coverage(side, &atlas, Some((0, 0, side, side)));

    let mut scene = Scene::new();
    scene.push_masked(Masked {
        clip: None,
        bounds: box_at(0.0, 0.0, 64.0, 64.0),
        uv: Rect::from_edges(0.0, 0.0, 1.0, 1.0),
        colour: Rgba::hex(0xff0000),
    });
    let pixels = h.draw(&scene);

    assert!(
        close(at(&pixels, 8, 32), [255, 0, 0, 255]),
        "covered: {:?}",
        at(&pixels, 8, 32)
    );
    assert_eq!(
        at(&pixels, 56, 32)[3],
        0,
        "the empty half of the mask drew something"
    );
});

gpu_test!(a_mask_is_drawn_over_the_quads_under_it, h, {
    // Text sits on the box it is written in. Two passes, quads then masks,
    // rather than one sorted list.
    let side = 2u32;
    h.renderer
        .upload_coverage(side, &[255; 4], Some((0, 0, side, side)));

    let mut scene = Scene::new();
    scene.push(Quad::new(
        box_at(0.0, 0.0, 64.0, 64.0),
        Background::Solid(Rgba::hex(0x0000ff)),
    ));
    scene.push_masked(Masked {
        clip: None,
        bounds: box_at(16.0, 16.0, 48.0, 48.0),
        uv: Rect::from_edges(0.0, 0.0, 1.0, 1.0),
        colour: Rgba::hex(0x00ff00),
    });
    let pixels = h.draw(&scene);

    assert!(
        close(at(&pixels, 32, 32), [0, 255, 0, 255]),
        "the mask should be on top"
    );
    assert!(
        close(at(&pixels, 4, 4), [0, 0, 255, 255]),
        "the quad shows where the mask is not"
    );
});

gpu_test!(a_half_covered_pixel_is_half_there, h, {
    // The whole reason coverage is a value rather than a flag: without it
    // every glyph edge is a staircase.
    let side = 2u32;
    h.renderer
        .upload_coverage(side, &[128; 4], Some((0, 0, side, side)));

    let mut scene = Scene::new();
    scene.push_masked(Masked {
        clip: None,
        bounds: box_at(0.0, 0.0, 64.0, 64.0),
        uv: Rect::from_edges(0.0, 0.0, 1.0, 1.0),
        colour: Rgba::hex(0xffffff),
    });
    let pixels = h.draw(&scene);
    let alpha = at(&pixels, 32, 32)[3];
    assert!(
        (100..=155).contains(&alpha),
        "half coverage came out as {alpha}"
    );
});

gpu_test!(nothing_is_drawn_outside_a_clip, h, {
    // What makes scrolling possible: a box reaching past the end of the thing
    // holding it is cut at the edge rather than drawn over what is above and
    // below it.
    let mut scene = Scene::new();
    scene.push(
        Quad::new(
            box_at(0.0, 0.0, 64.0, 64.0),
            Background::Solid(Rgba::hex(0xff0000)),
        )
        .clipped_to(Some(box_at(0.0, 0.0, 64.0, 32.0))),
    );
    let pixels = h.draw(&scene);

    assert!(
        close(at(&pixels, 32, 16), [255, 0, 0, 255]),
        "inside the clip"
    );
    assert_eq!(at(&pixels, 32, 48)[3], 0, "below the clip should be empty");
    // And the boundary belongs to the half that is drawn.
    assert!(at(&pixels, 32, 31)[3] > 0);
    assert_eq!(at(&pixels, 32, 32)[3], 0);
});

gpu_test!(a_clip_cuts_a_mask_too, h, {
    let side = 2u32;
    h.renderer
        .upload_coverage(side, &[255; 4], Some((0, 0, side, side)));

    let mut scene = Scene::new();
    scene.push_masked(Masked {
        clip: Some(box_at(0.0, 0.0, 32.0, 64.0)),
        bounds: box_at(0.0, 0.0, 64.0, 64.0),
        uv: Rect::from_edges(0.0, 0.0, 1.0, 1.0),
        colour: Rgba::hex(0xffffff),
    });
    let pixels = h.draw(&scene);
    assert!(at(&pixels, 16, 32)[3] > 0, "inside the clip");
    assert_eq!(at(&pixels, 48, 32)[3], 0, "outside it");
});

gpu_test!(a_picture_is_drawn_from_its_own_colours, h, {
    // A two by two atlas: red, green on the top row; blue, white below.
    let side = 2u32;
    let pixels: Vec<u8> = [
        [255u8, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ]
    .concat();
    h.renderer
        .upload_pictures(side, &pixels, Some((0, 0, side, side)));

    let mut scene = Scene::new();
    scene.push_textured(
        wisp_core::Textured::new(
            box_at(0.0, 0.0, 64.0, 64.0),
            Rect::from_edges(0.0, 0.0, 1.0, 1.0),
        )
        .tinted(Rgba::hex(0xffffff)),
    );
    let pixels = h.draw(&scene);

    assert!(
        close(at(&pixels, 8, 8), [255, 0, 0, 255]),
        "top left: {:?}",
        at(&pixels, 8, 8)
    );
    assert!(close(at(&pixels, 56, 8), [0, 255, 0, 255]), "top right");
    assert!(close(at(&pixels, 8, 56), [0, 0, 255, 255]), "bottom left");
});

gpu_test!(a_tint_multiplies_into_the_picture, h, {
    // What makes one white icon serve every colour it is needed in.
    let side = 1u32;
    h.renderer
        .upload_pictures(side, &[255, 255, 255, 255], Some((0, 0, side, side)));

    let mut scene = Scene::new();
    scene.push_textured(
        wisp_core::Textured::new(
            box_at(0.0, 0.0, 64.0, 64.0),
            Rect::from_edges(0.0, 0.0, 1.0, 1.0),
        )
        .tinted(Rgba::hex(0xff0000)),
    );
    let pixels = h.draw(&scene);
    assert!(
        close(at(&pixels, 32, 32), [255, 0, 0, 255]),
        "{:?}",
        at(&pixels, 32, 32)
    );
});

gpu_test!(a_transparent_pixel_in_a_picture_stays_transparent, h, {
    // A sprite is drawn on a transparent canvas, and the canvas has to stay
    // transparent or the character is a rectangle.
    let side = 2u32;
    let pixels: Vec<u8> = [
        [255u8, 255, 255, 255],
        [0, 0, 0, 0],
        [0, 0, 0, 0],
        [0, 0, 0, 0],
    ]
    .concat();
    h.renderer
        .upload_pictures(side, &pixels, Some((0, 0, side, side)));

    let mut scene = Scene::new();
    scene.push_textured(
        wisp_core::Textured::new(
            box_at(0.0, 0.0, 64.0, 64.0),
            Rect::from_edges(0.0, 0.0, 1.0, 1.0),
        )
        .tinted(Rgba::hex(0xffffff)),
    );
    let pixels = h.draw(&scene);
    assert!(at(&pixels, 8, 8)[3] > 200, "the opaque quarter");
    assert_eq!(at(&pixels, 56, 56)[3], 0, "the empty quarter");
});

gpu_test!(a_turned_picture_turns_about_the_pivot_it_was_given, h, {
    // A one pixel picture stretched into a tall thin bar, stood on the bottom
    // edge of the frame and leaned. Turned about its middle it would swing
    // both ends; turned about its foot only the top moves.
    let side = 1u32;
    h.renderer
        .upload_pictures(side, &[255, 255, 255, 255], Some((0, 0, side, side)));
    let uv = Rect::from_edges(0.0, 0.0, 1.0, 1.0);
    let bar = box_at(30.0, 4.0, 34.0, 60.0);

    let mut upright = Scene::new();
    upright.push_textured(wisp_core::Textured::new(bar, uv));
    let before = h.draw(&upright);

    let mut leaned = Scene::new();
    leaned.push_textured(wisp_core::Textured::new(bar, uv).turned(0.35, (0.5, 1.0)));
    let after = h.draw(&leaned);

    // The foot has not moved.
    assert!(
        before[(58 * SIZE + 32) as usize][3] > 100,
        "the foot was drawn"
    );
    assert!(
        after[(58 * SIZE + 32) as usize][3] > 100,
        "the foot stayed put"
    );
    // The top has.
    assert!(
        before[(6 * SIZE + 32) as usize][3] > 100,
        "the top was drawn"
    );
    assert_eq!(
        after[(6 * SIZE + 32) as usize][3],
        0,
        "the top should have leaned away"
    );
});

gpu_test!(a_rotation_of_nothing_changes_nothing, h, {
    let side = 1u32;
    h.renderer
        .upload_pictures(side, &[255, 255, 255, 255], Some((0, 0, side, side)));
    let uv = Rect::from_edges(0.0, 0.0, 1.0, 1.0);
    let bar = box_at(16.0, 16.0, 48.0, 48.0);

    let mut plain = Scene::new();
    plain.push_textured(wisp_core::Textured::new(bar, uv));
    let a = h.draw(&plain);

    let mut turned = Scene::new();
    turned.push_textured(wisp_core::Textured::new(bar, uv).turned(0.0, (0.5, 1.0)));
    let b = h.draw(&turned);
    assert_eq!(a, b);
});
