extern crate raw_window_handle;

use std::sync::Arc;

use rend3::{
    types::{DirectionalLight, Object, ResourceHandle},
    InstanceAdapterDevice, Renderer,
};
use rend3_routine::{base::BaseRenderGraph, pbr::PbrRoutine, tonemapping::TonemappingRoutine};
use wgpu::{Surface, TextureFormat};

pub struct RenderingHandler {
    pub renderer: Arc<Renderer>,
    rendergraph: BaseRenderGraph,
    pbr_routine: PbrRoutine,
    tonemapping_routine: TonemappingRoutine,
    pub resolution: rend3::types::glam::UVec2,
    pub preferred_format: TextureFormat,
    surface: Arc<Surface>,
    objects: Vec<ResourceHandle<Object>>,
    lights: Vec<ResourceHandle<DirectionalLight>>,
}

impl RenderingHandler {
    pub fn new(
        iad: InstanceAdapterDevice,
        surface: Arc<Surface>,
        size: rend3::types::glam::UVec2,
    ) -> Self {
        let formats = surface.get_supported_formats(&iad.adapter);
        let preferred_format = formats[0];

        rend3::configure_surface(
            &surface,
            &iad.device,
            preferred_format,
            size,
            rend3::types::PresentMode::Mailbox,
        );

        let renderer = rend3::Renderer::new(
            iad,
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

        let view_location = glam::Vec3::new(3.0, 3.0, -5.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, -0.55, 0.5, 0.0);
        let view = view * glam::Mat4::from_translation(-view_location);

        renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Perspective {
                vfov: 60.0,
                near: 0.1,
            },
            view,
        });

        let directional_handle = renderer.add_directional_light(rend3::types::DirectionalLight {
            color: glam::Vec3::ONE,
            intensity: 10.0,
            // Direction will be normalized
            direction: glam::Vec3::new(-1.0, -4.0, 2.0),
            distance: 400.0,
        });

        Self {
            renderer: renderer,
            rendergraph: base_rendergraph,
            pbr_routine: pbr_routine,
            tonemapping_routine: tonemapping_routine,
            resolution: size,
            preferred_format: preferred_format,
            surface: surface,
            objects: Vec::new(),
            lights: vec![directional_handle],
        }
    }

    pub fn resize(&mut self, new_size: rend3::types::glam::UVec2) {
        self.resolution = new_size;

        rend3::configure_surface(
            &self.surface,
            &self.renderer.device,
            self.preferred_format,
            self.resolution,
            rend3::types::PresentMode::Mailbox,
        );

        self.renderer
            .set_aspect_ratio(self.resolution.x as f32 / self.resolution.y as f32);
    }

    pub fn render_to_texture(&self, texture: Arc<wgpu::TextureView>) {
        let frame = rend3::util::output::OutputFrame::View(texture);

        let (cmd_bufs, ready) = self.renderer.ready();
        let mut graph = rend3::graph::RenderGraph::new();

        self.rendergraph.add_to_graph(
            &mut graph,
            &ready,
            &self.pbr_routine,
            None,
            &self.tonemapping_routine,
            self.resolution,
            rend3::types::SampleCount::One,
            rend3::types::glam::Vec4::ZERO,
            rend3::types::glam::Vec4::new(0.10, 0.05, 0.10, 1.0),
        );

        graph.execute(&self.renderer, frame, cmd_bufs, &ready);
    }

    pub fn add_cube(&mut self) {
        let mesh = create_mesh();
        let mesh_handle = self.renderer.add_mesh(mesh);
        let material = rend3_routine::pbr::PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
            ..rend3_routine::pbr::PbrMaterial::default()
        };
        let material_handle = self.renderer.add_material(material);

        let object = rend3::types::Object {
            mesh_kind: rend3::types::ObjectMeshKind::Static(mesh_handle),
            material: material_handle,
            transform: glam::Mat4::IDENTITY,
        };

        let object_handle = self.renderer.add_object(object);
        self.objects.push(object_handle);
    }
}

fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn create_mesh() -> rend3::types::Mesh {
    let vertex_positions = [
        // far side (0.0, 0.0, 1.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        // near side (0.0, 0.0, -1.0)
        vertex([-1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // right side (1.0, 0.0, 0.0)
        vertex([1.0, -1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        // left side (-1.0, 0.0, 0.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // top (0.0, 1.0, 0.0)
        vertex([1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        // bottom (0.0, -1.0, 0.0)
        vertex([1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
    ];

    let index_data: &[u32] = &[
        0, 1, 2, 2, 3, 0, // far
        4, 5, 6, 6, 7, 4, // near
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // top
        20, 21, 22, 22, 23, 20, // bottom
    ];

    rend3::types::MeshBuilder::new(vertex_positions.to_vec(), rend3::types::Handedness::Left)
        .with_indices(index_data.to_vec())
        .build()
        .unwrap()
}
