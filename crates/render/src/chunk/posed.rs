//! Native moving models share the world's textures, animation, lightmap and terrain shaders.
use super::*;
mod replacement;
use crate::queue_writes::ProfiledQueue as _;
use bevy::render::{
    extract_resource::ExtractResourcePlugin,
    render_phase::{
        AddRenderCommand, DrawFunctions, RenderCommand, RenderCommandResult, SetItemPipeline,
    },
    render_resource::{
        Buffer, BufferDescriptor, BufferUsages, VertexAttribute, VertexFormat, VertexStepMode,
    },
};
use bevy::{mesh::VertexBufferLayout, prelude::Shader};
use pipeline::layouts::{ChunkPipelineKey, ChunkPipelineSpecializer};
use render_model::posed_block::Vertex;
use render_scene::posed_block::PosedBlocks;

pub(super) fn install(app: &mut App) {
    app.init_resource::<PosedBlocks>();
    app.add_plugins(ExtractResourcePlugin::<PosedBlocks>::default());
    crate::pipeline_warmup::register::<Pipelines>(app);
    let r = app.sub_app_mut(RenderApp);
    r.init_resource::<Gpu>()
        .add_render_command::<Opaque3d, DrawSolid>()
        .add_render_command::<Transparent3d, DrawBlend>()
        .add_systems(
            RenderStartup,
            init.after(bevy::render::init_gpu_resource::<ChunkPipeline>),
        )
        .add_systems(
            Render,
            prepare
                .in_set(RenderSystems::PrepareResources)
                .after(prepare_gpu_chunks),
        )
        .add_systems(
            Render,
            replacement::prepare
                .in_set(RenderSystems::PrepareResources)
                .after(prepare_gpu_chunks),
        )
        .add_systems(Render, queue.in_set(RenderSystems::Queue));
}
#[derive(Resource)]
struct Pipelines {
    solid: Variants<RenderPipeline, ChunkPipelineSpecializer>,
    blend: Variants<RenderPipeline, ChunkPipelineSpecializer>,
}
impl crate::pipeline_warmup::PrewarmPipelines for Pipelines {
    fn prewarm(
        &mut self,
        cache: &PipelineCache,
        view: crate::pipeline_warmup::WarmView,
        ids: &mut crate::pipeline_warmup::WarmupIds,
    ) -> Result<(), BevyError> {
        let key = ChunkPipelineKey {
            msaa: view.msaa,
            hdr: view.hdr,
            enhanced: false,
        };
        ids.push(self.solid.specialize(cache, key)?);
        ids.push(self.blend.specialize(cache, key)?);
        Ok(())
    }
}
fn init(world: &mut World) {
    let compact = world.resource::<ChunkPipeline>().compact;
    let mut solid = pipeline::layouts::descriptors::build(compact)[2].clone();
    solid.label = Some("native moving model".into());
    solid.vertex.entry_point = Some("vertex_moving".into());
    solid.vertex.shader_defs.push("MOVING_BLOCKS".into());
    solid.vertex.buffers = vec![VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: vec![
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, origin) as u64,
                shader_location: 0,
                format: VertexFormat::Sint32x3,
            },
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, position) as u64,
                shader_location: 1,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, uv) as u64,
                shader_location: 2,
                format: VertexFormat::Float32x2,
            },
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, normal) as u64,
                shader_location: 3,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, material) as u64,
                shader_location: 4,
                format: VertexFormat::Uint32,
            },
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, light) as u64,
                shader_location: 5,
                format: VertexFormat::Uint32,
            },
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, flags) as u64,
                shader_location: 6,
                format: VertexFormat::Uint32,
            },
            VertexAttribute {
                offset: std::mem::offset_of!(Vertex, biome) as u64,
                shader_location: 7,
                format: VertexFormat::Uint32,
            },
        ],
    }];
    let mut blend = solid.clone();
    blend.fragment.as_mut().unwrap().entry_point = Some("fragment_blend".into());
    blend.fragment.as_mut().unwrap().targets[0]
        .as_mut()
        .unwrap()
        .blend = Some(BlendState::ALPHA_BLENDING);
    world.insert_resource(Pipelines {
        solid: Variants::new(ChunkPipelineSpecializer, solid),
        blend: Variants::new(ChunkPipelineSpecializer, blend),
    });
}
#[derive(Resource, Default)]
struct Gpu {
    buffer: Option<Buffer>,
    capacity: usize,
    solid: u32,
    count: u32,
    vertices: Vec<Vertex>,
    identity: Option<render_scene::ChunkTextureBindingIdentity>,
}
fn prepare(
    scene: Res<PosedBlocks>,
    textures: Res<ChunkGpuTextureAssets>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut gpu: ResMut<Gpu>,
) {
    gpu.solid = 0;
    gpu.count = 0;
    if scene.identity.is_none() {
        gpu.identity = None;
        gpu.buffer = None;
        gpu.vertices = Vec::new();
        gpu.capacity = 0;
        return;
    }
    if textures
        .prepared
        .as_ref()
        .is_none_or(|t| Some(t.identity) != scene.identity)
    {
        gpu.identity = None;
        return;
    }
    let mut vertices = Vec::with_capacity(scene.solid.len() + scene.blend.len());
    vertices.extend_from_slice(&scene.solid);
    vertices.extend_from_slice(&scene.blend);
    if vertices != gpu.vertices || gpu.identity != scene.identity {
        let bytes = bytemuck::cast_slice(&vertices);
        if bytes.len() > gpu.capacity {
            gpu.capacity = bytes.len().next_power_of_two();
            gpu.buffer = Some(device.create_buffer(&BufferDescriptor {
                label: Some("native moving vertices"),
                size: gpu.capacity as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if !bytes.is_empty() {
            queue.profiled_write_buffer(gpu.buffer.as_ref().unwrap(), 0, bytes);
        }
        gpu.vertices = vertices;
        gpu.identity = scene.identity;
    }
    gpu.solid = scene.solid.len() as u32;
    gpu.count = gpu.vertices.len() as u32;
}
#[allow(
    clippy::too_many_arguments,
    reason = "native terrain phase and view inputs"
)]
fn queue(
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<Pipelines>,
    scene: Res<PosedBlocks>,
    textures: Res<ChunkGpuTextureAssets>,
    arena: Res<ChunkGpuArena>,
    mut opaque: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    mut transparent: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    solid_functions: Res<DrawFunctions<Opaque3d>>,
    blend_functions: Res<DrawFunctions<Transparent3d>>,
    views: Query<(
        Entity,
        &MainEntity,
        &ExtractedView,
        &bevy::render::camera::ExtractedCamera,
        &Msaa,
    )>,
) {
    // Queue precedes PrepareResources: ranges must come from this frame's extraction,
    // not the previous frame's uploaded buffer. Preparation checks the identity again.
    let solid = scene.solid.len() as u32;
    let count = solid + scene.blend.len() as u32;
    if count == 0
        || arena.bind_group.is_none()
        || scene.identity.is_none()
        || textures
            .prepared
            .as_ref()
            .is_none_or(|t| Some(t.identity) != scene.identity)
    {
        return;
    }
    for (entity, main, view, camera, msaa) in &views {
        let key = ChunkPipelineKey {
            msaa: *msaa,
            hdr: camera.hdr,
            enhanced: false,
        };
        if solid > 0
            && let Some(phase) = opaque.get_mut(&view.retained_view_entity)
            && let Ok(id) = pipelines.solid.specialize(&cache, key)
        {
            phase.add(
                Opaque3dBatchSetKey {
                    draw_function: solid_functions.read().id::<DrawSolid>(),
                    pipeline: id,
                    material_bind_group_index: None,
                    lightmap_slab: None,
                    slabs: default(),
                },
                Opaque3dBinKey {
                    asset_id: AssetId::<Shader>::default().untyped(),
                },
                (entity, *main),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
        if count > solid
            && let Some(phase) = transparent.get_mut(&view.retained_view_entity)
            && let Ok(id) = pipelines.blend.specialize(&cache, key)
        {
            for start in (solid..count).step_by(6) {
                let quad = &scene.blend[(start - solid) as usize..][..6];
                let center = [0, 1, 2, 5]
                    .into_iter()
                    .map(|i| {
                        Vec3::from_array(quad[i].origin.map(|n| n as f32))
                            + Vec3::from_array(quad[i].position)
                    })
                    .sum::<Vec3>()
                    * 0.25;
                crate::transparent_phase::add(
                    phase,
                    Transparent3d {
                        sorting_info:
                            bevy::core_pipeline::core_3d::TransparentSortingInfo3d::AlwaysOnTop,
                        entity: (entity, *main),
                        pipeline: id,
                        draw_function: blend_functions.read().id::<DrawBlend>(),
                        distance: view.rangefinder3d().distance(&center),
                        batch_range: 0..1,
                        extra_index: PhaseItemExtraIndex::IndirectParametersIndex {
                            range: start..start + 6,
                            batch_set_index: None,
                        },
                        indexed: false,
                    },
                );
            }
        }
    }
}
type DrawSolid = (
    SetItemPipeline,
    crate::lighting::SetWorldLightmap,
    Draw<false>,
);
type DrawBlend = (
    SetItemPipeline,
    crate::lighting::SetWorldLightmap,
    Draw<true>,
);
struct Draw<const BLEND: bool>;
impl<P: PhaseItem, const BLEND: bool> RenderCommand<P> for Draw<BLEND> {
    type Param = (SRes<Gpu>, SRes<ChunkGpuArena>);
    type ViewQuery = Read<ViewUniformOffset>;
    type ItemQuery = ();
    fn render<'w>(
        item: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        _: Option<()>,
        (gpu, arena): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let gpu = gpu.into_inner();
        let arena = arena.into_inner();
        if gpu.identity.is_none() {
            return RenderCommandResult::Skip;
        }
        let (Some(buffer), Some(binding)) = (&gpu.buffer, &arena.bind_group) else {
            return RenderCommandResult::Skip;
        };
        let range = if BLEND {
            match item.extra_index() {
                PhaseItemExtraIndex::IndirectParametersIndex { range, .. } => range.clone(),
                _ => return RenderCommandResult::Skip,
            }
        } else {
            0..gpu.solid
        };
        if range.end > gpu.count {
            return RenderCommandResult::Skip;
        }
        pass.set_bind_group(0, binding, &[view.offset]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(range, 0..1);
        RenderCommandResult::Success
    }
}
