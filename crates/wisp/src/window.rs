use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use wisp_core::{Rgba, Scale, Scene};
use wisp_gpu::Renderer;
use wisp_ui::{Element, Pointer, Ui};

/// What a window is asked for when it is opened.
#[derive(Debug, Clone)]
pub struct WindowOptions {
    pub title: String,
    /// In points, which is what a window is asked for in everywhere.
    pub size: (f32, f32),
    /// Painted where nothing else is. Transparent is the interesting case and
    /// so it is the default: it is what a window over somebody's desktop needs.
    pub clear: Rgba,
    pub transparent: bool,
    pub decorated: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "wisp".into(),
            size: (800.0, 600.0),
            clear: Rgba::TRANSPARENT,
            transparent: true,
            decorated: true,
        }
    }
}

/// What the draw callback is told about the frame it is building.
pub struct Frame {
    /// The drawable area, in device pixels.
    pub size: (u32, u32),
    /// Device pixels per point on the display this window is on.
    pub scale: Scale,
    /// Seconds since the window opened.
    pub elapsed: f32,
}

/// Opens a window and draws it until it is closed.
///
/// `build` is called once per frame and returns the tree for that frame. It is
/// laid out, painted and hit-tested here; what the pointer did to the previous
/// frame is available from the [`Ui`] while the next one is being built, which
/// is what makes a button that has no state of its own possible.
///
/// The loop is paced by the surface. [`wgpu::PresentMode::AutoVsync`] blocks
/// until the display is ready, so frames come at the refresh rate rather than
/// against a clock of this library's own -- a timer beating against the
/// refresh is what judder is.
pub fn run<F>(options: WindowOptions, build: F) -> Result<(), winit::error::EventLoopError>
where
    F: FnMut(&mut Ui, &Frame) -> Element + 'static,
{
    let event_loop = EventLoop::new()?;
    // Poll, not Wait: something is animating in the sort of window this
    // library is for, and the surface's own pacing is what stops that becoming
    // a busy loop.
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App {
        options,
        build,
        state: None,
        opened: std::time::Instant::now(),
        scene: Scene::new(),
        ui: Ui::new(),
        pointer: Pointer::default(),
    })
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

struct App<F> {
    options: WindowOptions,
    build: F,
    state: Option<State>,
    opened: std::time::Instant,
    scene: Scene,
    ui: Ui,
    pointer: Pointer,
}

impl<F: FnMut(&mut Ui, &Frame) -> Element> App<F> {
    fn open(&mut self, event_loop: &ActiveEventLoop) -> Option<State> {
        let attributes = Window::default_attributes()
            .with_title(self.options.title.clone())
            .with_transparent(self.options.transparent)
            .with_decorations(self.options.decorated)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.options.size.0,
                self.options.size.1,
            ));
        let window = Arc::new(event_loop.create_window(attributes).ok()?);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone()).ok()?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;

        let size = window.inner_size();
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // The window is meant to let the desktop through where nothing was
            // drawn, so the surface has to be composited rather than opaque.
            // Not every platform offers it; opaque is the honest fallback.
            alpha_mode: capabilities
                .alpha_modes
                .iter()
                .copied()
                .find(|m| *m == wgpu::CompositeAlphaMode::PostMultiplied)
                .unwrap_or(capabilities.alpha_modes[0]),
            color_space: wgpu::SurfaceColorSpace::Srgb,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        crate::diagnostics::announce_window_id(&window);
        Some(State {
            window,
            surface,
            config,
            renderer: Renderer::new(device, queue, format),
        })
    }
}

impl<F: FnMut(&mut Ui, &Frame) -> Element> ApplicationHandler for App<F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            self.state = self.open(event_loop);
            match self.state.as_ref() {
                // Nothing else asks for the first frame. Without this the
                // window opens, waits for a redraw that is never requested,
                // and shows an empty surface for ever.
                Some(state) => state.window.request_redraw(),
                None => {
                    eprintln!("wisp: could not open a window with a usable GPU");
                    event_loop.exit();
                }
            }
        }
    }

    /// Asks for the next frame from outside the redraw itself.
    ///
    /// Requesting it *inside* the redraw handler works until one frame is
    /// dropped -- an occluded window, a surface that needs reconfiguring --
    /// and then nothing ever asks again and the window stays on whatever it
    /// last drew. Here the chain cannot break.
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // Kept in points, which is what layout and hit testing are in.
            WindowEvent::CursorMoved { position, .. } => {
                let scale = state.window.scale_factor() as f32;
                self.pointer.at = (position.x as f32 / scale, position.y as f32 / scale);
            }
            WindowEvent::MouseInput {
                state: pressed,
                button,
                ..
            } => {
                if button == winit::event::MouseButton::Left {
                    self.pointer.down = pressed == winit::event::ElementState::Pressed;
                }
            }
            // A pointer that has left cannot be over anything. Without this the
            // last thing it was over stays lit for as long as the window is
            // open, which is the kind of detail that makes an interface feel
            // dead.
            WindowEvent::CursorLeft { .. } => {
                self.pointer.at = (f32::MIN, f32::MIN);
            }
            WindowEvent::Resized(size) => {
                state.config.width = size.width.max(1);
                state.config.height = size.height.max(1);
                state
                    .surface
                    .configure(state.renderer.device(), &state.config);
            }
            WindowEvent::RedrawRequested => {
                let surface_texture = match state.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture) => texture,
                    // Nothing is visible, so nothing is worth drawing. Coming
                    // back next frame is the whole handling.
                    wgpu::CurrentSurfaceTexture::Occluded
                    | wgpu::CurrentSurfaceTexture::Timeout => return,
                    // The window was resized or the display changed under us.
                    // Reconfiguring and waiting for the next frame is cheaper
                    // than trying to rescue this one.
                    _ => {
                        state
                            .surface
                            .configure(state.renderer.device(), &state.config);
                        return;
                    }
                };
                let scale = Scale::new(state.window.scale_factor() as f32).unwrap_or(Scale::ONE);
                self.scene.clear();
                self.ui.point(self.pointer);
                let frame = Frame {
                    size: (state.config.width, state.config.height),
                    scale,
                    elapsed: self.opened.elapsed().as_secs_f32(),
                };
                let root = (self.build)(&mut self.ui, &frame);
                let in_points = (
                    state.config.width as f32 / scale.factor(),
                    state.config.height as f32 / scale.factor(),
                );
                self.ui.frame(&root, in_points, scale, &mut self.scene);
                // Any glyph the frame asked for has been rasterised by now,
                // so this is where the atlas and the GPU agree again. Only
                // what changed is sent.
                let atlas = self.ui.text().atlas_mut();
                let dirty = atlas.take_dirty();
                let (side, pixels) = (atlas.side(), atlas.pixels().to_vec());
                state.renderer.upload_coverage(side, &pixels, dirty);

                let view = surface_texture.texture.create_view(&Default::default());
                state.renderer.draw(
                    &self.scene,
                    &view,
                    (state.config.width, state.config.height),
                    self.options.clear,
                );
                // Presented before the next frame is asked for, so that the
                // request lands after the surface has had this one.
                state.renderer.queue().present(surface_texture);
            }
            _ => {}
        }
    }
}
