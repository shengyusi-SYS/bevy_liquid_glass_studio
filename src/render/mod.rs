//! Register shared backdrop work at Bevy's native UI pass segments.
mod blur;

use std::collections::{HashMap, HashSet};

use bevy::{
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems,
        diagnostic::RecordDiagnostics,
        render_asset::RenderAssets,
        render_phase::ViewSortedRenderPhases,
        render_resource::{PipelineCache, TextureViewId},
        renderer::RenderContext,
        sync_world::RenderEntity,
        texture::GpuImage,
        view::{ExtractedView, ViewTarget},
    },
    ui_render::{TransparentUi, UiPassSegments, UiViewTarget},
};

use crate::{
    GlassBackdropRefresh, GlassNode,
    group::{GroupMember, GroupSurface, StandaloneMember},
    ui::BackdropImages,
};
use blur::{BlurPipeline, PreparedBlurs};

pub(crate) struct GlassRenderPlugin;

impl Plugin for GlassRenderPlugin {
    fn build(&self, app: &mut App) {
        blur::register_shader(app);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedBackdrops>()
            .init_resource::<PreparedBlurs>()
            .init_resource::<BackdropViews>()
            .add_systems(ExtractSchedule, extract_backdrops)
            .add_systems(RenderStartup, blur::initialize)
            .add_systems(
                Render,
                (
                    request_backdrop_pass
                        .after(RenderSystems::PhaseSort)
                        .before(RenderSystems::Prepare),
                    blur::prepare.in_set(RenderSystems::PrepareBindGroups),
                ),
            );
    }
}

pub(super) struct ExtractedBackdrop {
    images: BackdropImages,
    nodes: HashSet<Entity>,
}

#[derive(Resource, Default)]
pub(super) struct ExtractedBackdrops(HashMap<Entity, ExtractedBackdrop>);

fn extract_backdrops(
    mut extracted: ResMut<ExtractedBackdrops>,
    cameras: Extract<Query<(&RenderEntity, &BackdropImages)>>,
    nodes: Extract<
        Query<
            (Entity, &ComputedUiTargetCamera, &InheritedVisibility),
            (
                Or<(With<GlassNode>, With<StandaloneMember>, With<GroupSurface>)>,
                Without<GroupMember>,
            ),
        >,
    >,
    main_cameras: Extract<Query<&RenderEntity>>,
    mut active: Local<HashSet<Entity>>,
) {
    active.clear();
    for (render_entity, images) in &cameras {
        active.insert(render_entity.id());
        let backdrop = extracted
            .0
            .entry(render_entity.id())
            .or_insert_with(|| ExtractedBackdrop {
                images: images.clone(),
                nodes: HashSet::new(),
            });
        backdrop.images = images.clone();
        backdrop.nodes.clear();
    }
    extracted.0.retain(|camera, _| active.contains(camera));
    for (entity, camera, visible) in &nodes {
        if !visible.get() {
            continue;
        }
        let Some(camera) = camera
            .get()
            .and_then(|entity| main_cameras.get(entity).ok())
        else {
            continue;
        };
        if let Some(backdrop) = extracted.0.get_mut(&camera.id()) {
            backdrop.nodes.insert(entity);
        }
    }
    extracted.0.retain(|_, backdrop| !backdrop.nodes.is_empty());
}

#[derive(Resource, Default)]
struct BackdropViews(HashMap<Entity, BackdropViewIds>);

#[derive(PartialEq, Eq)]
struct BackdropViewIds {
    source: TextureViewId,
    downsampled: Option<TextureViewId>,
    vertical: Option<TextureViewId>,
    blurred: TextureViewId,
}

fn request_backdrop_pass(
    backdrops: Res<ExtractedBackdrops>,
    views: Query<(&ExtractedView, &UiViewTarget)>,
    phases: Res<ViewSortedRenderPhases<TransparentUi>>,
    mut segments: ResMut<UiPassSegments>,
    images: Res<RenderAssets<GpuImage>>,
    mut gpu_views: ResMut<BackdropViews>,
) {
    gpu_views
        .0
        .retain(|entity, _| backdrops.0.contains_key(entity));
    for (view, target) in &views {
        let Some(backdrop) = backdrops.0.get(&target.0) else {
            continue;
        };
        // External images with zero blur are sampled directly by the material.
        if !backdrop.images.live && backdrop.images.blur_size.is_none() {
            continue;
        }
        let config = &backdrop.images;
        let current_views = images
            .get(config.source.id())
            .zip(images.get(config.blurred.id()))
            .and_then(|(source, blurred)| {
                let downsampled = match &config.downsampled {
                    Some(handle) => Some(images.get(handle.id())?.texture_view.id()),
                    None => None,
                };
                let vertical = match &config.vertical {
                    Some(handle) => Some(images.get(handle.id())?.texture_view.id()),
                    None => None,
                };
                Some(BackdropViewIds {
                    source: source.texture_view.id(),
                    downsampled,
                    vertical,
                    blurred: blurred.texture_view.id(),
                })
            });
        if let Some(current_views) = current_views {
            if gpu_views.0.get(&target.0) != Some(&current_views) {
                config.cache.invalidate();
                gpu_views.0.insert(target.0, current_views);
            }
        } else {
            config.cache.invalidate();
        }
        if config.refresh == GlassBackdropRefresh::OnDemand
            && !config.cache.pending(config.revision)
        {
            continue;
        }
        let Some(phase) = phases.get(&view.retained_view_entity) else {
            continue;
        };
        let Some(first) = phase
            .items
            .values()
            .position(|item| backdrop.nodes.contains(&item.entity.1.id()))
        else {
            continue;
        };
        let index = if backdrop.images.live { first } else { 0 };
        segments.add(view.retained_view_entity, index, Some(render_backdrop));
    }
}

fn render_backdrop(world: &World, ui_view: Entity, ctx: &mut RenderContext) {
    let target_entity = world.get::<UiViewTarget>(ui_view).unwrap().0;
    let backdrops = world.resource::<ExtractedBackdrops>();
    let backdrop = &backdrops.0[&target_entity];
    if backdrop.images.refresh == GlassBackdropRefresh::OnDemand
        && !backdrop.images.cache.pending(backdrop.images.revision)
    {
        return;
    }
    let pipeline = world.resource::<BlurPipeline>();
    let prepared = world.resource::<PreparedBlurs>();
    let cache = world.resource::<PipelineCache>();
    let images = world.resource::<RenderAssets<GpuImage>>();
    let ready = if backdrop.images.blur_size.is_none() {
        images.get(backdrop.images.source.id()).is_some()
    } else {
        prepared.ready(target_entity, pipeline, cache)
    };
    if !ready {
        return;
    }
    if backdrop.images.live {
        let target = world.get::<ViewTarget>(target_entity).unwrap();
        let image = images.get(backdrop.images.source.id()).unwrap();
        let source = target.main_texture();
        let diagnostics = ctx.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let span = diagnostics.time_span(ctx.command_encoder(), "liquid_glass_capture");
        ctx.command_encoder().copy_texture_to_texture(
            source.as_image_copy(),
            image.texture.as_image_copy(),
            source.size(),
        );
        span.end(ctx.command_encoder());
    }
    prepared.render(target_entity, pipeline, cache, images, ctx);
    backdrop.images.cache.complete(backdrop.images.revision);
}
