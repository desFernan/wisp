use bytemuck::{Pod, Zeroable};
use wisp_core::geometry::Rect;
use wisp_core::scene::{Background, Quad as SceneQuad};
use wisp_core::{DevicePixels, Rgba, Scene};

use crate::GRADIENT_SAMPLES;

/// Everything the shader needs about the frame as a whole.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    viewport: [f32; 2],
    _pad: [f32; 2],
}

/// One quad, laid out to match `Quad` in `quad.wgsl`.
///
/// `repr(C)` and every field a multiple of sixteen bytes: a storage buffer
/// struct whose Rust and WGSL layouts disagree does not fail to compile, it
/// draws something wrong somewhere else in the frame.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuQuad {
    bounds: [f32; 4],
    painted: [f32; 4],
    radii: [f32; 4],
    background: [f32; 4],
    border_colour: [f32; 4],
    shadow_colour: [f32; 4],
    /// border width, shadow blur, shadow spread, gradient row (-1 for solid)
    params: [f32; 4],
    /// shadow offset x, y, then the gradient's direction as a unit vector
    shadow_offset_and_gradient: [f32; 4],
    /// left, top, right, bottom
    clip: [f32; 4],
}

/// What a primitive with no clip is given.
///
/// A rectangle nothing can fall outside, rather than a flag: a branch in a
/// fragment shader costs more than four floats, and every primitive carries
/// the four floats either way.
const UNCLIPPED: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];

fn clip_of(clip: Option<Rect<DevicePixels>>) -> [f32; 4] {
    match clip {
        Some(c) => [px(c.left()), px(c.top()), px(c.right()), px(c.bottom())],
        None => UNCLIPPED,
    }
}

/// Colours reach the shader linearised.
///
/// The surface is an sRGB format, so the hardware encodes whatever the
/// fragment shader writes. Handing it the gamma-encoded values a theme is
/// written in encodes them twice and every mid-tone in the window comes out
/// pale -- and it does it silently for black and white, which are fixed points
/// of the curve and therefore exactly what a first test gets written with.
fn rgba(c: Rgba) -> [f32; 4] {
    let c = c.to_linear();
    [c.r, c.g, c.b, c.a]
}

fn px(v: DevicePixels) -> f32 {
    v.get()
}

/// One picture, laid out to match `Textured` in `textured.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuTextured {
    bounds: [f32; 4],
    uv: [f32; 4],
    tint: [f32; 4],
    clip: [f32; 4],
}

/// One masked item, laid out to match `Masked` in `masked.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuMasked {
    bounds: [f32; 4],
    uv: [f32; 4],
    colour: [f32; 4],
    clip: [f32; 4],
}

/// What a renderer draws onto: a configured surface and its size.
pub struct Surface<'window> {
    pub surface: wgpu::Surface<'window>,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    mask_pipeline: wgpu::RenderPipeline,
    picture_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    quads: wgpu::Buffer,
    quad_capacity: u64,
    gradients: wgpu::Texture,
    gradient_rows: u32,
    masked: wgpu::Buffer,
    masked_capacity: u64,
    coverage: wgpu::Texture,
    coverage_side: u32,
    textured: wgpu::Buffer,
    textured_capacity: u64,
    pictures: wgpu::Texture,
    pictures_side: u32,
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wisp quad"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wisp quad"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wisp quad"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wisp quad"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Premultiplied, matching what the shader emits. Straight
                    // alpha darkens every antialiased edge against a
                    // transparent background, which is the case this library
                    // is built for.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let mask_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wisp masked"),
            source: wgpu::ShaderSource::Wgsl(include_str!("masked.wgsl").into()),
        });
        let mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wisp masked"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &mask_shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &mask_shader,
                entry_point: Some("fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let picture_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wisp textured"),
            source: wgpu::ShaderSource::Wgsl(include_str!("textured.wgsl").into()),
        });
        let picture_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wisp textured"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &picture_shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &picture_shader,
                entry_point: Some("fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wisp globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let quad_capacity = 256;
        let quads = Self::quad_buffer(&device, quad_capacity);
        let gradient_rows = 1;
        let gradients = Self::gradient_texture(&device, gradient_rows);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("wisp gradients"),
            // Clamped, so that a `t` at either end holds the end colour rather
            // than wrapping round to the other one.
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let masked_capacity = 512;
        let masked = Self::masked_buffer(&device, masked_capacity);
        let coverage_side = 1;
        let coverage = Self::coverage_texture(&device, coverage_side);

        let textured_capacity = 256;
        let textured = Self::textured_buffer(&device, textured_capacity);
        let pictures_side = 1;
        let pictures = Self::picture_texture(&device, pictures_side);

        Self {
            device,
            queue,
            pipeline,
            mask_pipeline,
            picture_pipeline,
            layout,
            sampler,
            globals,
            quads,
            quad_capacity,
            gradients,
            gradient_rows,
            masked,
            masked_capacity,
            coverage,
            coverage_side,
            textured,
            textured_capacity,
            pictures,
            pictures_side,
        }
    }

    fn textured_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wisp textured"),
            size: capacity * size_of::<GpuTextured>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn picture_texture(device: &wgpu::Device, side: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wisp pictures"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // sRGB, so the hardware decodes on the way in and the shader works
            // in linear like everything else in the frame.
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    /// Hands the renderer the picture atlas.
    ///
    /// `dirty` is the region written since last time; `None` sends nothing.
    pub fn upload_pictures(
        &mut self,
        side: u32,
        pixels: &[u8],
        dirty: Option<(u32, u32, u32, u32)>,
    ) {
        if side != self.pictures_side {
            self.pictures_side = side;
            self.pictures = Self::picture_texture(&self.device, side);
            self.write_pictures(pixels, (0, 0, side, side));
            return;
        }
        if let Some(region) = dirty {
            self.write_pictures(pixels, region);
        }
    }

    fn write_pictures(&self, pixels: &[u8], (left, top, right, bottom): (u32, u32, u32, u32)) {
        let (width, height) = (right.saturating_sub(left), bottom.saturating_sub(top));
        if width == 0 || height == 0 {
            return;
        }
        for row in 0..height {
            let from = (((top + row) * self.pictures_side + left) * 4) as usize;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.pictures,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: left,
                        y: top + row,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels[from..from + (width * 4) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn masked_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wisp masked"),
            size: capacity * size_of::<GpuMasked>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn coverage_texture(device: &wgpu::Device, side: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wisp coverage"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // One channel: a glyph is a shape, and its colour is applied when
            // it is drawn.
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    /// Hands the renderer the coverage atlas that masked items are read from.
    ///
    /// `dirty` is the region written since last time, so that a frame which
    /// added one glyph does not re-send four megabytes. `None` sends nothing:
    /// the atlas on the GPU is already what the caller has.
    pub fn upload_coverage(
        &mut self,
        side: u32,
        pixels: &[u8],
        dirty: Option<(u32, u32, u32, u32)>,
    ) {
        if side != self.coverage_side {
            self.coverage_side = side;
            self.coverage = Self::coverage_texture(&self.device, side);
            // A new texture has nothing in it, so the region that changed is
            // all of it whatever the caller said.
            self.write_coverage(pixels, (0, 0, side, side));
            return;
        }
        if let Some(region) = dirty {
            self.write_coverage(pixels, region);
        }
    }

    fn write_coverage(&self, pixels: &[u8], (left, top, right, bottom): (u32, u32, u32, u32)) {
        let (width, height) = (right.saturating_sub(left), bottom.saturating_sub(top));
        if width == 0 || height == 0 {
            return;
        }
        // Rows have to be contiguous for one write, and a sub-rectangle of the
        // atlas is not, so the region is sent a row at a time. Text is added
        // in bursts and then not at all, so this is not a per-frame cost.
        for row in 0..height {
            let from = ((top + row) * self.coverage_side + left) as usize;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.coverage,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: left,
                        y: top + row,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels[from..from + width as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn quad_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wisp quads"),
            size: capacity * size_of::<GpuQuad>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn gradient_texture(device: &wgpu::Device, rows: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wisp gradients"),
            size: wgpu::Extent3d {
                width: GRADIENT_SAMPLES,
                height: rows.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Draws `scene` onto `view`, clearing to `clear` first.
    ///
    /// A transparent clear colour is the usual one here: the window underneath
    /// is meant to show through wherever nothing was drawn.
    pub fn draw(&mut self, scene: &Scene, view: &wgpu::TextureView, size: (u32, u32), clear: Rgba) {
        let (quads, ramps) = self.encode(scene);
        let masked: Vec<GpuMasked> = scene
            .masked()
            .iter()
            .map(|m| GpuMasked {
                bounds: [
                    px(m.bounds.left()),
                    px(m.bounds.top()),
                    px(m.bounds.size.width),
                    px(m.bounds.size.height),
                ],
                uv: [m.uv.left(), m.uv.top(), m.uv.right(), m.uv.bottom()],
                colour: rgba(m.colour),
                clip: clip_of(m.clip),
            })
            .collect();
        self.upload(&quads, &ramps, size);
        if masked.len() as u64 > self.masked_capacity {
            self.masked_capacity = (masked.len() as u64).next_power_of_two();
            self.masked = Self::masked_buffer(&self.device, self.masked_capacity);
        }
        if !masked.is_empty() {
            self.queue
                .write_buffer(&self.masked, 0, bytemuck::cast_slice(&masked));
        }

        let pictures: Vec<GpuTextured> = scene
            .textured()
            .iter()
            .map(|t| GpuTextured {
                bounds: [
                    px(t.bounds.left()),
                    px(t.bounds.top()),
                    px(t.bounds.size.width),
                    px(t.bounds.size.height),
                ],
                uv: [t.uv.left(), t.uv.top(), t.uv.right(), t.uv.bottom()],
                tint: rgba(t.tint),
                clip: clip_of(t.clip),
            })
            .collect();
        if pictures.len() as u64 > self.textured_capacity {
            self.textured_capacity = (pictures.len() as u64).next_power_of_two();
            self.textured = Self::textured_buffer(&self.device, self.textured_capacity);
        }
        if !pictures.is_empty() {
            self.queue
                .write_buffer(&self.textured, 0, bytemuck::cast_slice(&pictures));
        }

        let picture_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wisp textured"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.textured.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &self.pictures.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mask_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wisp masked"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.masked.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &self.coverage.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wisp quad"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.quads.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &self.gradients.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wisp frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wisp frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // `wgpu::Color` is linear, and premultiplied to match
                        // the blend the rest of the frame uses.
                        load: wgpu::LoadOp::Clear({
                            let c = clear.to_linear();
                            wgpu::Color {
                                r: (c.r * c.a) as f64,
                                g: (c.g * c.a) as f64,
                                b: (c.b * c.a) as f64,
                                a: c.a as f64,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !quads.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..6, 0..quads.len() as u32);
            }
            // Second, so that text sits on the boxes it is written in. Two
            // passes rather than one sorted list: swapping pipelines per item
            // costs more than the ordering is worth for an interface, where
            // text is essentially always on top of its own background.
            // Pictures between the boxes and the text: an avatar sits on its
            // card and text sits on the avatar.
            if !pictures.is_empty() {
                pass.set_pipeline(&self.picture_pipeline);
                pass.set_bind_group(0, &picture_bind_group, &[]);
                pass.draw(0..6, 0..pictures.len() as u32);
            }
            if !masked.is_empty() {
                pass.set_pipeline(&self.mask_pipeline);
                pass.set_bind_group(0, &mask_bind_group, &[]);
                pass.draw(0..6, 0..masked.len() as u32);
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Turns a scene into what the GPU needs: one instance per quad, and one
    /// row of colour ramp per gradient.
    fn encode(&self, scene: &Scene) -> (Vec<GpuQuad>, Vec<[u8; 4]>) {
        let mut ramps: Vec<[u8; 4]> = Vec::new();
        let quads = scene
            .quads()
            .iter()
            .map(|quad| self.encode_one(quad, &mut ramps))
            .collect();
        (quads, ramps)
    }

    fn encode_one(&self, quad: &SceneQuad, ramps: &mut Vec<[u8; 4]>) -> GpuQuad {
        let corners = quad.corners.fitted_to(quad.bounds);
        let painted = quad.painted_bounds();

        let (background, row, direction) = match &quad.background {
            Background::Solid(colour) => (rgba(*colour), -1.0, [0.0, 0.0]),
            gradient @ Background::LinearGradient { angle, .. } => {
                let row = (ramps.len() / GRADIENT_SAMPLES as usize) as f32;
                for step in 0..GRADIENT_SAMPLES {
                    let t = step as f32 / (GRADIENT_SAMPLES - 1) as f32;
                    let c = gradient.sample(t);
                    ramps.push([
                        (c.r * 255.0).round() as u8,
                        (c.g * 255.0).round() as u8,
                        (c.b * 255.0).round() as u8,
                        (c.a * 255.0).round() as u8,
                    ]);
                }
                ([0.0; 4], row, [angle.cos(), angle.sin()])
            }
        };

        let (border_width, border_colour) = match quad.border {
            Some(border) => (px(border.width), rgba(border.colour)),
            None => (0.0, [0.0; 4]),
        };
        let (blur, spread, shadow_colour, offset) = match quad.shadow {
            Some(s) => (
                px(s.blur),
                px(s.spread),
                rgba(s.colour),
                [px(s.offset.0), px(s.offset.1)],
            ),
            None => (0.0, 0.0, [0.0; 4], [0.0, 0.0]),
        };

        GpuQuad {
            bounds: [
                px(quad.bounds.left()),
                px(quad.bounds.top()),
                px(quad.bounds.size.width),
                px(quad.bounds.size.height),
            ],
            painted: [
                px(painted.left()),
                px(painted.top()),
                px(painted.size.width),
                px(painted.size.height),
            ],
            radii: [
                px(corners.top_left),
                px(corners.top_right),
                px(corners.bottom_right),
                px(corners.bottom_left),
            ],
            background,
            border_colour,
            shadow_colour,
            params: [border_width, blur, spread, row],
            shadow_offset_and_gradient: [offset[0], offset[1], direction[0], direction[1]],
            clip: clip_of(quad.clip),
        }
    }

    fn upload(&mut self, quads: &[GpuQuad], ramps: &[[u8; 4]], size: (u32, u32)) {
        self.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                viewport: [size.0.max(1) as f32, size.1.max(1) as f32],
                _pad: [0.0; 2],
            }),
        );

        if quads.len() as u64 > self.quad_capacity {
            // Doubling rather than fitting exactly: a scene that grows by one
            // quad a frame would otherwise reallocate every frame.
            self.quad_capacity = (quads.len() as u64).next_power_of_two();
            self.quads = Self::quad_buffer(&self.device, self.quad_capacity);
        }
        if !quads.is_empty() {
            self.queue
                .write_buffer(&self.quads, 0, bytemuck::cast_slice(quads));
        }

        let rows = (ramps.len() / GRADIENT_SAMPLES as usize).max(1) as u32;
        if rows > self.gradient_rows {
            self.gradient_rows = rows;
            self.gradients = Self::gradient_texture(&self.device, rows);
        }
        if !ramps.is_empty() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.gradients,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(ramps),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(GRADIENT_SAMPLES * 4),
                    rows_per_image: Some(rows),
                },
                wgpu::Extent3d {
                    width: GRADIENT_SAMPLES,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}
