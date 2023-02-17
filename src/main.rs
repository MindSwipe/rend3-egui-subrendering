mod rendering;
use std::{sync::Arc, time::Instant};

use rendering::*;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new();
    let window = {
        WindowBuilder::new()
            .with_title("r3e")
            .build(&event_loop)
            .unwrap()
    };

    let window_size = window.inner_size();

    let iad = pollster::block_on(rend3::create_iad(None, None, None, None)).unwrap();
    let surface = Arc::new(unsafe { iad.instance.create_surface(&window) });
    let formats = surface.get_supported_formats(&iad.adapter);
    let preferred_format = formats[0];
    let size = glam::UVec2::new(window_size.height, window_size.width);

    rend3::configure_surface(
        &surface,
        &iad.device,
        preferred_format,
        size,
        rend3::types::PresentMode::Mailbox,
    );

    let renderer = rend3::Renderer::new(
        iad.clone(),
        rend3::types::Handedness::Left,
        Some(size.x as f32 / size.y as f32),
    )
    .unwrap();

    let mut spp = rend3::ShaderPreProcessor::new();
    rend3_routine::builtin_shaders(&mut spp);
    let base_rendergraph = rend3_routine::base::BaseRenderGraph::new(&renderer, &spp);
    let mut data_core = renderer.data_core.lock();
    let pbr_routine = rend3_routine::pbr::PbrRoutine::new(
        &renderer,
        &mut data_core,
        &spp,
        &base_rendergraph.interfaces,
    );
    drop(data_core);
    let tonemapping_routine = rend3_routine::tonemapping::TonemappingRoutine::new(
        &renderer,
        &spp,
        &base_rendergraph.interfaces,
        preferred_format,
    );
    let mut egui_routine = rend3_egui::EguiRenderRoutine::new(
        &renderer,
        preferred_format,
        rend3::types::SampleCount::One,
        window_size.width,
        window_size.height,
        window.scale_factor() as f32,
    );

    let subiad = iad.clone();

    let mut subrendering = RenderingHandler::new(
        subiad,
        surface.clone(),
        glam::UVec2::new(window_size.width, window_size.height),
    );

    let mut platform =
        egui_winit_platform::Platform::new(egui_winit_platform::PlatformDescriptor {
            physical_width: window_size.width as u32,
            physical_height: window_size.height as u32,
            scale_factor: window.scale_factor(),
            font_definitions: egui::FontDefinitions::default(),
            style: Default::default(),
        });

    let mut resolution = glam::UVec2::new(window_size.width, window_size.height);
    let start_time = Instant::now();

    event_loop.run(move |event, _, ctrl| {
        platform.handle_event(&event);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *ctrl = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(physical_size),
                ..
            } => {
                resolution = glam::UVec2::new(physical_size.width, physical_size.height);

                rend3::configure_surface(
                    &surface,
                    &iad.device,
                    preferred_format,
                    glam::UVec2::new(resolution.x, resolution.y),
                    rend3::types::PresentMode::Mailbox,
                );

                renderer.set_aspect_ratio(resolution.x as f32 / resolution.y as f32);
            }
            Event::MainEventsCleared => {
                platform.update_time(start_time.elapsed().as_secs_f64());
                platform.begin_frame();

                let ctx = platform.context();
                egui::SidePanel::new(egui::panel::Side::Left, "Left Panel")
                    .resizable(true)
                    .show(&ctx, |ui| {
                        ui.label("Test label");
                        if ui.button("Add cube").clicked() {
                            subrendering.add_cube();
                        }
                    });

                let rect = ctx.available_rect();
                subrendering.resize(glam::UVec2::new(
                    *rect.x_range().end() as u32,
                    *rect.y_range().end() as u32,
                ));

                let texture =
                    subrendering
                        .renderer
                        .device
                        .create_texture(&wgpu::TextureDescriptor {
                            size: wgpu::Extent3d {
                                width: subrendering.resolution.x,
                                height: subrendering.resolution.y,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: subrendering.preferred_format,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            label: Some("Subrendering Texture"),
                        });

                let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));
                subrendering.render_to_texture(view.clone());

                let texture_id = egui_routine.internal.egui_texture_from_wgpu_texture(
                    &renderer.device,
                    view.as_ref(),
                    wgpu::FilterMode::Linear,
                );

                egui::CentralPanel::default().show(&ctx, |ui| {
                    egui::Frame::none().show(ui, |ui| {
                        ui.image(
                            texture_id,
                            egui::vec2(
                                subrendering.resolution.x as f32,
                                subrendering.resolution.y as f32,
                            ),
                        );
                    });
                });

                let egui::FullOutput {
                    shapes,
                    textures_delta,
                    ..
                } = platform.end_frame(Some(&window));
                let paint_jobs = platform.context().tessellate(shapes);

                let input = rend3_egui::Input {
                    clipped_meshes: &paint_jobs,
                    textures_delta,
                    context: platform.context(),
                };

                let frame = rend3::util::output::OutputFrame::Surface {
                    surface: Arc::clone(&surface),
                };

                let (cmd_bufs, ready) = renderer.ready();

                let mut graph = rend3::graph::RenderGraph::new();

                base_rendergraph.add_to_graph(
                    &mut graph,
                    &ready,
                    &pbr_routine,
                    None,
                    &tonemapping_routine,
                    resolution,
                    rend3::types::SampleCount::One,
                    glam::Vec4::ZERO,
                    glam::Vec4::new(0.0f32, 0.0f32, 0.0f32, 1.0f32),
                );

                let surface = graph.add_surface_texture();
                egui_routine.add_to_graph(&mut graph, input, surface);

                graph.execute(&renderer, frame, cmd_bufs, &ready);
            }
            _ => {}
        }
    });
}
