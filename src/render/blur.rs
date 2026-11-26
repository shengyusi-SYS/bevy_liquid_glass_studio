use std::collections::HashMap;

use bevy::{
    asset::{load_internal_asset, uuid_handle},
    core_pipeline::FullscreenShader,
    prelude::*,
    render::{
        diagnostic::RecordDiagnostics,
        render_asset::RenderAssets,
        render_resource::{binding_types::*, *},
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::GpuImage,
    },
    shader::Shader,
};

use super::ExtractedBackdrops;

const BLUR_SHADER: Handle<Shader> = uuid_handle!("bcb10b45-d0e6-4719-9bc8-46ed7e476680");

pub(super) fn register_shader(app: &mut App) {
    load_internal_asset!(
        app,
        BLUR_SHADER,
        "../../assets/shaders/blur.wesl",
        |source, _| Shader::from_wesl(source, "embedded://bevy_liquid_glass/blur.wesl")
    );
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum BlurSampling {
    Source,
    Working,
    PairedWorking,
}

#[derive(ShaderType, Clone)]
struct BlurUniform {
    step: Vec2,
    tap_count: u32,
    sampling: u32,
    kernel: [Vec4; 51],
}

impl BlurUniform {
    fn new(radius: f32, step: Vec2, sampling: BlurSampling) -> Self {
        let sigma = (radius / 3.0).max(0.01);
        let radius = radius.round().clamp(0.0, 200.0) as u32;
        let mut kernel = [Vec4::ZERO; 51];
        let mut sum = 0.0;
        for i in 0..=radius as usize {
            let w = (-0.5 * (i as f32 / sigma).powi(2)).exp();
            kernel[i / 4][i % 4] = w;
            sum += if i == 0 { w } else { w * 2.0 };
        }
        for weight in &mut kernel {
            *weight /= sum;
        }
        let paired = matches!(sampling, BlurSampling::PairedWorking);
        if paired {
            // Keep the center weight, then pack (combined weight, offset).
            // Bilinear sampling reconstructs the two original weighted texels.
            // The final unpaired texel keeps its integer offset.
            for i in (1..=radius as usize).step_by(2) {
                let first = kernel[i / 4][i % 4];
                let second = if i < radius as usize {
                    kernel[(i + 1) / 4][(i + 1) % 4]
                } else {
                    0.0
                };
                let weight = first + second;
                kernel[i / 4][i % 4] = weight;
                kernel[(i + 1) / 4][(i + 1) % 4] = i as f32 + second / weight;
            }
        }
        Self {
            step,
            tap_count: if paired { radius.div_ceil(2) } else { radius },
            sampling: sampling as u32,
            kernel,
        }
    }
}

#[derive(Resource)]
pub(super) struct BlurPipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedRenderPipelineId,
    downsample_pipeline: CachedRenderPipelineId,
    sampler: Sampler,
}

pub(super) fn initialize(
    mut commands: Commands,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    fullscreen: Res<FullscreenShader>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "liquid_glass_blur_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<BlurUniform>(false),
            ),
        ),
    );
    let descriptor = RenderPipelineDescriptor {
        label: Some("liquid_glass_blur".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: BLUR_SHADER.clone(),
            entry_point: Some("fragment".into()),
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    };
    let downsample_pipeline = cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("liquid_glass_downsample".into()),
        fragment: Some(FragmentState {
            entry_point: Some("downsample".into()),
            ..descriptor.fragment.clone().unwrap()
        }),
        ..descriptor.clone()
    });
    let pipeline = cache.queue_render_pipeline(descriptor);
    commands.insert_resource(BlurPipeline {
        layout,
        pipeline,
        downsample_pipeline,
        sampler: device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        }),
    });
}

struct PreparedBlur {
    source_view: TextureViewId,
    vertical_view: TextureViewId,
    source: AssetId<Image>,
    vertical: AssetId<Image>,
    blurred: AssetId<Image>,
    size: UVec2,
    blur_size: UVec2,
    radius: f32,
    downsampled: Option<PreparedDownsample>,
    vertical_group: BindGroup,
    horizontal_group: BindGroup,
    _vertical_uniform: UniformBuffer<BlurUniform>,
    _horizontal_uniform: UniformBuffer<BlurUniform>,
}

struct PreparedDownsample {
    image: AssetId<Image>,
    view: TextureViewId,
    group: BindGroup,
}

#[derive(Resource, Default)]
pub(super) struct PreparedBlurs(HashMap<Entity, PreparedBlur>);

pub(super) fn prepare(
    backdrops: Res<ExtractedBackdrops>,
    images: Res<RenderAssets<GpuImage>>,
    pipeline: Res<BlurPipeline>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut prepared: ResMut<PreparedBlurs>,
) {
    prepared
        .0
        .retain(|entity, _| backdrops.0.contains_key(entity));
    for (entity, backdrop) in &backdrops.0 {
        let config = &backdrop.images;
        let Some(blur_size) = config.blur_size else {
            prepared.0.remove(entity);
            continue;
        };
        let source = config.source.id();
        let vertical = config.vertical.as_ref().unwrap().id();
        let blurred = config.blurred.id();
        let downsampled = config.downsampled.as_ref().map(Handle::id);
        let downsampled_image = downsampled.and_then(|id| images.get(id));
        if downsampled.is_some() && downsampled_image.is_none() {
            prepared.0.remove(entity);
            continue;
        }
        let (Some(source_image), Some(vertical_image), Some(_)) = (
            images.get(source),
            images.get(vertical),
            images.get(blurred),
        ) else {
            prepared.0.remove(entity);
            continue;
        };
        if prepared.0.get(entity).is_some_and(|p| {
            p.source == source
                && p.vertical == vertical
                && p.blurred == blurred
                && p.size == config.size
                && p.blur_size == blur_size
                && p.radius == config.blur_radius
                && p.downsampled.as_ref().map(|d| (d.image, d.view))
                    == downsampled.zip(downsampled_image.map(|image| image.texture_view.id()))
                && p.source_view == source_image.texture_view.id()
                && p.vertical_view == vertical_image.texture_view.id()
        }) {
            continue;
        }
        let mut vertical_uniform = UniformBuffer::from(BlurUniform::new(
            config.blur_radius * blur_size.y as f32 / config.size.y as f32,
            Vec2::new(0.0, 1.0 / blur_size.y as f32),
            if downsampled.is_some() {
                BlurSampling::PairedWorking
            } else {
                BlurSampling::Source
            },
        ));
        let mut horizontal_uniform = UniformBuffer::from(BlurUniform::new(
            config.blur_radius * blur_size.x as f32 / config.size.x as f32,
            Vec2::new(1.0 / blur_size.x as f32, 0.0),
            if downsampled.is_some() {
                BlurSampling::PairedWorking
            } else {
                BlurSampling::Working
            },
        ));
        vertical_uniform.write_buffer(&device, &queue);
        horizontal_uniform.write_buffer(&device, &queue);
        let layout = cache.get_bind_group_layout(&pipeline.layout);
        let vertical_group = device.create_bind_group(
            "liquid_glass_vertical_blur",
            &layout,
            &BindGroupEntries::sequential((
                &downsampled_image.unwrap_or(source_image).texture_view,
                &pipeline.sampler,
                vertical_uniform.binding().unwrap(),
            )),
        );
        let horizontal_group = device.create_bind_group(
            "liquid_glass_horizontal_blur",
            &layout,
            &BindGroupEntries::sequential((
                &vertical_image.texture_view,
                &pipeline.sampler,
                horizontal_uniform.binding().unwrap(),
            )),
        );
        let downsampled = downsampled
            .zip(downsampled_image)
            .map(|(image, gpu_image)| PreparedDownsample {
                image,
                view: gpu_image.texture_view.id(),
                group: device.create_bind_group(
                    "liquid_glass_downsample",
                    &layout,
                    &BindGroupEntries::sequential((
                        &source_image.texture_view,
                        &pipeline.sampler,
                        vertical_uniform.binding().unwrap(),
                    )),
                ),
            });
        prepared.0.insert(
            *entity,
            PreparedBlur {
                source_view: source_image.texture_view.id(),
                vertical_view: vertical_image.texture_view.id(),
                source,
                vertical,
                blurred,
                size: config.size,
                blur_size,
                radius: config.blur_radius,
                downsampled,
                vertical_group,
                horizontal_group,
                _vertical_uniform: vertical_uniform,
                _horizontal_uniform: horizontal_uniform,
            },
        );
    }
}

impl PreparedBlurs {
    pub(super) fn ready(
        &self,
        camera: Entity,
        pipeline: &BlurPipeline,
        cache: &PipelineCache,
    ) -> bool {
        self.0.get(&camera).is_some_and(|prepared| {
            cache.get_render_pipeline(pipeline.pipeline).is_some()
                && (prepared.downsampled.is_none()
                    || cache
                        .get_render_pipeline(pipeline.downsample_pipeline)
                        .is_some())
        })
    }

    pub(super) fn render(
        &self,
        camera: Entity,
        pipeline: &BlurPipeline,
        cache: &PipelineCache,
        images: &RenderAssets<GpuImage>,
        ctx: &mut RenderContext,
    ) {
        let Some(prepared) = self.0.get(&camera) else {
            // Zero radius aliases the blurred input to the raw source.
            return;
        };
        let diagnostics = ctx.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let downsample = prepared.downsampled.iter().map(|downsample| {
            (
                downsample.image,
                &downsample.group,
                "liquid_glass_downsample",
                pipeline.downsample_pipeline,
            )
        });
        for (image, group, label, pipeline_id) in downsample.chain([
            (
                prepared.vertical,
                &prepared.vertical_group,
                "liquid_glass_vertical",
                pipeline.pipeline,
            ),
            (
                prepared.blurred,
                &prepared.horizontal_group,
                "liquid_glass_horizontal",
                pipeline.pipeline,
            ),
        ]) {
            let render_pipeline = cache.get_render_pipeline(pipeline_id).unwrap();
            let image = images.get(image).unwrap();
            let mut pass = ctx
                .command_encoder()
                .begin_render_pass(&RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &image.texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(default()),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            let span = diagnostics.pass_span(&mut pass, label);
            pass.set_pipeline(render_pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
            span.end(&mut pass);
        }
    }
}
