//! rust-browser: navegador liviano de aprendizaje en Rust.
//!
//! - UI (pestañas + toolbar): egui renderizado con wgpu (Direct3D 12 en
//!   Windows). Se evita OpenGL a propósito: en sesiones remotas/híbridas la
//!   presentación WGL/EGL puede quedar en negro aunque todo lo demás funcione.
//! - Contenido: WebView2 (Chromium del sistema) embebido con wry.
//! - Un único webview compartido: el estado de cada pestaña vive en Rust
//!   y cambiar de pestaña vuelve a cargar su URL actual.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod history;
mod navigation;
mod ui;
mod webview;

use std::sync::Arc;

use winit::event_loop::EventLoop;

use app::{BrowserApp, UserEvent};

/// Ventana + superficie wgpu + integración egui (estado de input y renderer).
struct WgpuWindowContext {
    window: Arc<winit::window::Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl WgpuWindowContext {
    fn new(event_loop: &winit::event_loop::ActiveEventLoop, ctx: egui::Context) -> Self {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_resizable(true)
                        .with_inner_size(winit::dpi::LogicalSize {
                            width: 1200.0,
                            height: 800.0,
                        })
                        .with_title("Rust Browser"),
                )
                .expect("fallo creando la ventana"),
        );

        // DX12 primero (el camino nativo de Windows), Vulkan/GL de respaldo.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let surface = instance
            .create_surface(window.clone())
            .expect("fallo creando la surface wgpu");

        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            }))
            .expect("no hay adaptador wgpu compatible");

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rust-browser device"),
            ..Default::default()
        }))
        .expect("fallo pidiendo el dispositivo wgpu");

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let state = egui_winit::State::new(
            ctx,
            egui::ViewportId::ROOT,
            event_loop,
            None,
            event_loop.system_theme(),
            None,
        );
        let renderer = egui_wgpu::Renderer::new(
            &device,
            format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: false,
                predictable_texture_filtering: false,
            },
        );

        Self {
            window,
            surface,
            device,
            queue,
            config,
            state,
            renderer,
        }
    }

    fn window(&self) -> &winit::window::Window {
        self.window.as_ref()
    }

    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Pasa la UI de egui por el pipeline completo y presenta el frame.
    fn render(
        &mut self,
        ctx: &egui::Context,
        browser: &mut app::BrowserState,
        actions: &mut Vec<app::Action>,
    ) {
        let input = self.state.take_egui_input(&self.window);
        let full_output = ctx.run_ui(input, |ui| {
            ui::draw_chrome(ui, browser, actions);
        });
        self.state
            .handle_platform_output(&self.window, full_output.platform_output);

        let pixels_per_point = full_output.pixels_per_point;
        let clipped_primitives = ctx.tessellate(full_output.shapes, pixels_per_point);
        let mut textures_delta = full_output.textures_delta;

        let size = self.window.inner_size();
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [size.width, size.height],
            pixels_per_point,
        };

        for (texture_id, image_deltas) in textures_delta.set.drain() {
            for image_delta in image_deltas {
                self.renderer.update_texture(
                    &self.device,
                    &self.queue,
                    texture_id,
                    &image_delta,
                );
            }
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.config);
                frame
            }
            // Timeout/Occluded/Outdated/Lost: se salta este frame sin drama.
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        {
            let color = wgpu::Color {
                r: 0.09,
                g: 0.09,
                b: 0.11,
                a: 1.0,
            };
            let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // wgpu 30: render requiere RenderPass<'static>; forget_lifetime
            // relaja el lifetime (validación pasa a runtime, que aquí no violamos).
            self.renderer.render(
                &mut rpass.forget_lifetime(),
                &clipped_primitives,
                &screen_descriptor,
            );
        }

        self.queue.submit([encoder.finish()]);
        // En wgpu 30 la presentación se hace desde el queue.
        self.queue.present(frame);

        for texture_id in textures_delta.free.drain() {
            self.renderer.free_texture(&texture_id);
        }
    }

    fn on_window_event(
        &mut self,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(&self.window, event)
    }
}

fn main() {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("fallo construyendo el event loop");
    let proxy = event_loop.create_proxy();

    let mut app = BrowserApp::new(proxy);
    event_loop
        .run_app(&mut app)
        .expect("fallo ejecutando la aplicación");
}
