//! Blink lens pass. Uses the motion kit's warm-up, travel and cooldown envelopes.
//! Embedded in the binary so launching outside the workspace still loads the shader.

// encase 0.10 emits unused layout-check functions from the ShaderType derive.
#![allow(dead_code)]

use bevy::{
    asset::{load_internal_asset, weak_handle},
    core_pipeline::{
        core_3d::graph::{Core3d, Node3d},
        fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    },
    ecs::query::QueryItem,
    prelude::*,
    render::{
        extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin},
        render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner},
        render_resource::{
            binding_types::{sampler, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice},
        view::ViewTarget,
        RenderApp,
    },
};

const SHADER: Handle<Shader> = weak_handle!("9c70a649-8d09-4d28-b594-7383b451963e");

#[derive(Component, Default, Clone, Copy, ExtractComponent, ShaderType)]
pub struct BlinkLens {
    pub blur: f32,
    pub distortion: f32,
    pub time: f32,
    pub aspect: f32,
}

pub struct BlinkPostPlugin;

impl Plugin for BlinkPostPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "blink_post.wgsl", Shader::from_wgsl);
        app.add_plugins((ExtractComponentPlugin::<BlinkLens>::default(), UniformComponentPlugin::<BlinkLens>::default()));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .add_render_graph_node::<ViewNodeRunner<BlinkNode>>(Core3d, BlinkPass)
                .add_render_graph_edges(Core3d, (Node3d::Tonemapping, BlinkPass, Node3d::EndMainPassPostProcessing));
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.init_resource::<BlinkPipeline>();
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct BlinkPass;

#[derive(Default)]
struct BlinkNode;

impl ViewNode for BlinkNode {
    type ViewQuery = (&'static ViewTarget, &'static BlinkLens, &'static DynamicUniformIndex<BlinkLens>);

    fn run(
        &self,
        _: &mut RenderGraphContext,
        context: &mut RenderContext,
        (target, lens, index): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        // No texture flip or fullscreen work when the spell is inactive.
        if lens.blur <= 0.0001 && lens.distortion <= 0.0001 {
            return Ok(());
        }
        let data = world.resource::<BlinkPipeline>();
        let cache = world.resource::<PipelineCache>();
        let id = if target.is_hdr() { data.hdr } else { data.ldr };
        let Some(pipeline) = cache.get_render_pipeline(id) else { return Ok(()) };
        let uniforms = world.resource::<ComponentUniforms<BlinkLens>>();
        let Some(binding) = uniforms.uniforms().binding() else { return Ok(()) };
        let textures = target.post_process_write();
        let bind = context.render_device().create_bind_group(
            "blink_lens",
            &data.layout,
            &BindGroupEntries::sequential((textures.source, &data.sampler, binding)),
        );
        let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("blink_lens"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: textures.destination,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_render_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[index.index()]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}

#[derive(Resource)]
struct BlinkPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    hdr: CachedRenderPipelineId,
    ldr: CachedRenderPipelineId,
}

impl FromWorld for BlinkPipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let layout = device.create_bind_group_layout(
            "blink_lens",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    uniform_buffer::<BlinkLens>(true),
                ),
            ),
        );
        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        });
        let cache = world.resource::<PipelineCache>();
        let queue = |format| {
            cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("blink_lens".into()),
                layout: vec![layout.clone()],
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader: SHADER,
                    shader_defs: vec![],
                    entry_point: "fragment".into(),
                    targets: vec![Some(ColorTargetState {
                        format,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                push_constant_ranges: vec![],
                zero_initialize_workgroup_memory: false,
            })
        };
        let hdr = queue(ViewTarget::TEXTURE_FORMAT_HDR);
        let ldr = queue(TextureFormat::bevy_default());
        Self { layout, sampler, hdr, ldr }
    }
}
