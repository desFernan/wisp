//! Rendering one frame to a file, with no window.
//!
//! A user interface library is judged by what it draws, and the only way to
//! know what it draws is to look. Looking through a window means a machine
//! with a screen, a window that is not covered by something else -- macOS
//! stops updating the contents of a fully occluded window, so a screenshot of
//! one is whatever it last managed to draw -- and somebody's desktop being
//! borrowed for the duration.
//!
//! This renders offscreen instead. It is the same renderer, the same layout
//! and the same fonts, so what comes out is what a window would have shown,
//! and it runs anywhere with a GPU adapter.

use std::path::Path;

use wisp_core::{Rgba, Scale, Scene};
use wisp_gpu::Renderer;
use wisp_ui::{Element, Ui};

/// Rows must be aligned to this in a buffer copy, which is why the image is
/// unpacked a row at a time rather than in one go.
const ROW_ALIGN: u32 = 256;

/// Lays `build` out at `size` points and writes it to `path` as a PNG.
///
/// `Err` when there is no GPU adapter, which is the normal state of a headless
/// runner, so a caller that wants to skip rather than fail can.
pub fn write<F>(
    path: impl AsRef<Path>,
    size: (f32, f32),
    scale: f32,
    clear: Rgba,
    mut build: F,
) -> anyhow::Result<()>
where
    F: FnMut(&mut Ui) -> Element,
{
    let scale = Scale::new(scale).ok_or_else(|| anyhow::anyhow!("scale must be positive"))?;
    let (width, height) = (
        (size.0 * scale.factor()).round() as u32,
        (size.1 * scale.factor()).round() as u32,
    );

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .map_err(|_| anyhow::anyhow!("no GPU adapter this machine can use"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("wisp snapshot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let stride = width * 4;
    let padded = stride.div_ceil(ROW_ALIGN) * ROW_ALIGN;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wisp snapshot"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut renderer = Renderer::new(device, queue, format);
    let mut ui = Ui::new();
    let mut scene = Scene::new();
    let root = build(&mut ui);
    ui.frame(&root, size, scale, &mut scene);

    // The glyphs the frame asked for exist by now, so this is where the atlas
    // and the GPU agree.
    let atlas = ui.text().atlas_mut();
    let dirty = atlas.take_dirty();
    let (side, pixels) = (atlas.side(), atlas.pixels().to_vec());
    renderer.upload_coverage(side, &pixels, dirty);

    let view = target.create_view(&Default::default());
    renderer.draw(&scene, &view, (width, height), clear);

    let mut encoder = renderer
        .device()
        .create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit([encoder.finish()]);

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    renderer.device().poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;

    let mut rows = Vec::with_capacity((stride * height) as usize);
    {
        let mapped = slice
            .get_mapped_range()
            .map_err(|e| anyhow::anyhow!("mapping the snapshot back: {e:?}"))?;
        for row in 0..height {
            let from = (row * padded) as usize;
            rows.extend_from_slice(&mapped[from..from + stride as usize]);
        }
    }
    readback.unmap();

    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // The framebuffer is sRGB, and saying so is the difference between an
    // image that matches the window and one that opens two stops too bright.
    encoder.set_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(&rows)?;
    Ok(())
}
