use bytemuck::{Pod, Zeroable};
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
}

fn rgba(c: Rgba) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

fn px(v: DevicePixels) -> f32 {
    v.get()
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
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    quads: wgpu::Buffer,
    quad_capacity: u64,
    gradients: wgpu::Texture,
    gradient_rows: u32,
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

        Self {
            device,
            queue,
            pipeline,
            layout,
            sampler,
            globals,
            quads,
            quad_capacity,
            gradients,
            gradient_rows,
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
        self.upload(&quads, &ramps, size);

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
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: (clear.r * clear.a) as f64,
                            g: (clear.g * clear.a) as f64,
                            b: (clear.b * clear.a) as f64,
                            a: clear.a as f64,
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
