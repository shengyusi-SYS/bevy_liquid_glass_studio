use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use bevy::{
    asset::AssetEventSystems,
    camera::{CameraMainTextureUsages, CompositingSpace, Hdr, RenderTarget},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    ui::UiSystems,
    ui_render::UiMaterialPlugin,
};

use crate::{
    GlassBackdrop, GlassBackdropRefresh, GlassBlurQuality, GlassGroup, GlassMember, GlassNode,
    GlassStatistics,
    group::{GlassGroupSystems, GroupMember, StandaloneMember},
    material::{self, GlassGroupMaterial, GlassMaterial, GlassUniform},
    material_cache::SharedGlassMaterials,
};

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GlassUiSystems {
    PrepareBackdrops,
    Materials,
}

pub(crate) struct GlassUiPlugin;

impl Plugin for GlassUiPlugin {
    fn build(&self, app: &mut App) {
        material::register_shaders(app);
        app.init_resource::<SharedGlassMaterials>()
            .add_plugins((
                UiMaterialPlugin::<GlassMaterial>::default(),
                UiMaterialPlugin::<GlassGroupMaterial>::default(),
            ))
            .add_systems(
                PostUpdate,
                synchronize_styles
                    .after(GlassGroupSystems::Membership)
                    .before(UiSystems::Layout),
            )
            .add_systems(
                PostUpdate,
                (
                    prepare_backdrops.in_set(GlassUiSystems::PrepareBackdrops),
                    synchronize_nodes.in_set(GlassUiSystems::Materials),
                )
                    .chain()
                    .after(UiSystems::PostLayout)
                    .before(AssetEventSystems),
            );
    }
}

/// Main-world assets are also the handles used by ordinary UiMaterial binding.
#[derive(Component, Clone, Debug)]
pub(crate) struct BackdropImages {
    pub source: Handle<Image>,
    pub downsampled: Option<Handle<Image>>,
    pub vertical: Option<Handle<Image>>,
    pub blurred: Handle<Image>,
    pub size: UVec2,
    pub blur_size: Option<UVec2>,
    pub format: TextureFormat,
    pub blur_radius: f32,
    pub live: bool,
    pub refresh: GlassBackdropRefresh,
    pub revision: u64,
    pub cache: Arc<BackdropCache>,
    target_scale: f32,
    viewport: Option<(UVec2, UVec2, f32, f32)>,
    compositing: Option<CompositingSpace>,
}

#[derive(Debug, Default)]
pub(crate) struct BackdropCache {
    rendered: AtomicU64,
    updates: AtomicU64,
}

impl BackdropCache {
    pub fn pending(&self, revision: u64) -> bool {
        self.rendered.load(Ordering::Relaxed) != revision
    }
    pub fn invalidate(&self) {
        self.rendered.store(0, Ordering::Relaxed);
    }
    pub fn complete(&self, revision: u64) {
        self.rendered.store(revision, Ordering::Relaxed);
        self.updates.fetch_add(1, Ordering::Relaxed);
    }
}

fn target_image(
    images: &mut Assets<Image>,
    size: UVec2,
    format: TextureFormat,
    capture: bool,
) -> Handle<Image> {
    let mut image = Image::new_uninit(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        default(),
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | if capture {
            TextureUsages::COPY_DST
        } else {
            TextureUsages::RENDER_ATTACHMENT
        };
    image.sampler = bevy::image::ImageSampler::linear();
    images.add(image)
}

fn prepare_backdrops(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    removed: Query<Entity, (With<BackdropImages>, Without<GlassBackdrop>)>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut statistics: ResMut<GlassStatistics>,
    mut cameras: Query<(
        Entity,
        &Camera,
        Ref<GlassBackdrop>,
        Ref<RenderTarget>,
        Option<Ref<CompositingSpace>>,
        Option<&Hdr>,
        Option<&BackdropImages>,
        &mut CameraMainTextureUsages,
    )>,
) {
    let modified: HashSet<_> = image_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    statistics.backdrop_updates = 0;
    for entity in &removed {
        commands.entity(entity).remove::<BackdropImages>();
    }
    for (entity, camera, settings, target, space, hdr, previous, mut usages) in &mut cameras {
        if let Some(previous) = previous {
            statistics.backdrop_updates += previous.cache.updates.load(Ordering::Relaxed);
        }
        let Some(size) = camera
            .physical_target_size()
            .filter(|size| size.min_element() > 0)
        else {
            continue;
        };
        if !camera.is_active {
            continue;
        }
        if !usages.0.contains(TextureUsages::COPY_SRC) {
            usages.0 |= TextureUsages::COPY_SRC;
        }
        let format = if hdr.is_some() {
            TextureFormat::Rgba16Float
        } else {
            TextureFormat::Rgba8UnormSrgb
        };
        let live = settings.source.is_none();
        let blur_radius = (settings.blur_radius * camera.target_scaling_factor().unwrap_or(1.0))
            .round()
            .clamp(0.0, 200.0);
        let blur_size = (blur_radius > 0.0).then(|| {
            let half = match settings.blur_quality {
                GlassBlurQuality::Auto => blur_radius >= 8.0,
                GlassBlurQuality::FullResolution => false,
                GlassBlurQuality::HalfResolution => true,
            };
            if half { (size + UVec2::ONE) / 2 } else { size }
        });
        let capture_rebuild =
            previous.is_none_or(|p| p.size != size || p.format != format || p.live != live);
        let rebuild = capture_rebuild || previous.is_some_and(|p| p.blur_size != blur_size);
        let source = if let Some(source) = &settings.source {
            source.clone()
        } else if !capture_rebuild {
            previous.unwrap().source.clone()
        } else {
            target_image(&mut images, size, format, true)
        };
        let target_scale = camera.target_scaling_factor().unwrap_or(1.0);
        let compositing = space.as_ref().map(|space| **space);
        let viewport = camera.viewport.as_ref().map(|v| {
            (
                v.physical_position,
                v.physical_size,
                v.depth.start,
                v.depth.end,
            )
        });
        let changed = rebuild
            || settings.is_changed()
            || target.is_changed()
            || previous.is_some_and(|p| {
                p.target_scale != target_scale
                    || p.viewport != viewport
                    || p.compositing != compositing
            })
            || previous.is_some_and(|p| p.source != source || p.blur_radius != blur_radius)
            || modified.contains(&source.id());
        let revision = previous.map_or(1, |p| p.revision.wrapping_add(u64::from(changed)).max(1));
        if rebuild {
            let downsampled = blur_size
                .filter(|blur_size| *blur_size != size)
                .map(|size| target_image(&mut images, size, TextureFormat::Rgba16Float, false));
            let vertical = blur_size
                .map(|size| target_image(&mut images, size, TextureFormat::Rgba16Float, false));
            let blurred = blur_size.map_or_else(
                || source.clone(),
                |size| target_image(&mut images, size, TextureFormat::Rgba16Float, false),
            );
            commands.entity(entity).insert(BackdropImages {
                source,
                downsampled,
                vertical,
                blurred,
                size,
                blur_size,
                format,
                blur_radius,
                live,
                refresh: settings.refresh,
                target_scale,
                viewport,
                compositing,
                revision,
                cache: previous
                    .map_or_else(|| Arc::new(BackdropCache::default()), |p| p.cache.clone()),
            });
        } else if let Some(previous) = previous {
            if changed {
                let mut updated = previous.clone();
                if blur_size.is_none() {
                    updated.blurred = source.clone();
                }
                updated.source = source;
                updated.blur_radius = blur_radius;
                updated.refresh = settings.refresh;
                updated.revision = revision;
                updated.target_scale = target_scale;
                updated.viewport = viewport;
                updated.compositing = compositing;
                commands.entity(entity).insert(updated);
            }
        }
    }
}

// Native UI must resolve the new corner geometry before material extraction.
fn synchronize_styles(
    mut commands: Commands,
    groups: Query<Ref<GlassGroup>>,
    mut nodes: Query<
        (
            Entity,
            Option<Ref<GlassNode>>,
            Option<Ref<GlassMember>>,
            Option<Ref<StandaloneMember>>,
            Option<Ref<GroupMember>>,
            &mut Node,
            Option<&BoxShadow>,
            Option<&MaterialNode<GlassMaterial>>,
        ),
        Or<(With<GlassNode>, With<GlassMember>)>,
    >,
    mut removed_nodes: RemovedComponents<GlassNode>,
    mut removed_members: RemovedComponents<GlassMember>,
    mut removed_standalone: RemovedComponents<StandaloneMember>,
    mut removed: Local<HashSet<Entity>>,
) {
    removed.clear();
    removed.extend(removed_nodes.read());
    removed.extend(removed_members.read());
    removed.extend(removed_standalone.read());
    for (entity, glass, member, standalone, fused, mut node, shadow, material) in &mut nodes {
        let group = standalone
            .as_ref()
            .and_then(|member| groups.get(member.group).ok());
        let changed = node.is_changed()
            || removed.contains(&entity)
            || fused.as_ref().is_some_and(|value| value.is_changed())
            || glass.as_ref().is_some_and(|value| value.is_changed())
            || member.as_ref().is_some_and(|value| value.is_changed())
            || standalone.as_ref().is_some_and(|value| value.is_changed())
            || group.as_ref().is_some_and(|value| value.is_changed());
        if !changed {
            continue;
        }
        let (shape, style) = if let Some(glass) = &glass {
            (&glass.shape, Some(&glass.style))
        } else {
            (
                &member.as_ref().unwrap().shape,
                group.as_ref().map(|group| &group.style),
            )
        };
        let radius = BorderRadius::all(Val::Px(shape.corner_radius));
        if node.border_radius != radius {
            node.border_radius = radius;
        }
        if let Some(style) = style {
            let resolved_shadow = style.box_shadow();
            if shadow.is_none_or(|shadow| *shadow != resolved_shadow) {
                commands.entity(entity).insert(resolved_shadow);
            }
        } else if material.is_some() || shadow.is_some() {
            // Fused members are drawn by the union surface. Members outside a
            // group have no optical style and stop rendering glass.
            commands
                .entity(entity)
                .remove::<(MaterialNode<GlassMaterial>, NodeMaterialInputs, BoxShadow)>();
        }
    }
    for entity in removed.iter().copied() {
        if !nodes.contains(entity)
            && let Ok(mut entity) = commands.get_entity(entity)
        {
            entity.remove::<(MaterialNode<GlassMaterial>, NodeMaterialInputs, BoxShadow)>();
        }
    }
}

#[derive(Component, PartialEq)]
struct NodeMaterialInputs {
    camera: Entity,
    size: UVec2,
    scale: f32,
    source: AssetId<Image>,
    blurred: AssetId<Image>,
}

fn synchronize_nodes(
    mut commands: Commands,
    mut materials: ResMut<Assets<GlassMaterial>>,
    mut shared: ResMut<SharedGlassMaterials>,
    mut statistics: ResMut<GlassStatistics>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut material_events: MessageReader<AssetEvent<GlassMaterial>>,
    cameras: Query<&BackdropImages>,
    groups: Query<Ref<GlassGroup>>,
    nodes: Query<
        (
            Entity,
            Option<Ref<GlassNode>>,
            Option<Ref<GlassMember>>,
            Option<Ref<StandaloneMember>>,
            &ComputedNode,
            &ComputedUiTargetCamera,
            Option<&MaterialNode<GlassMaterial>>,
            Option<&NodeMaterialInputs>,
        ),
        (
            Or<(With<GlassNode>, With<StandaloneMember>)>,
            Without<GroupMember>,
        ),
    >,
    mut changed_images: Local<HashSet<AssetId<Image>>>,
) {
    let started = statistics.collect_timings.then(std::time::Instant::now);
    changed_images.clear();
    changed_images.extend(image_events.read().filter_map(|event| match event {
        AssetEvent::Modified { id } => Some(*id),
        _ => None,
    }));
    if material_events
        .read()
        .filter(|event| matches!(event, AssetEvent::Removed { .. }))
        .count()
        > 0
    {
        shared.retain_live(&materials);
    }
    shared.refresh_images(&changed_images, &mut materials);
    for (entity, glass, member, standalone, computed, camera, material, previous) in &nodes {
        let background = camera
            .get()
            .and_then(|camera| cameras.get(camera).ok().map(|backdrop| (camera, backdrop)));
        let Some((camera, backdrop)) = background else {
            if material.is_some() {
                commands
                    .entity(entity)
                    .remove::<(MaterialNode<GlassMaterial>, NodeMaterialInputs)>();
            }
            continue;
        };
        let inputs = NodeMaterialInputs {
            camera,
            size: backdrop.size,
            scale: computed.inverse_scale_factor().recip(),
            source: backdrop.source.id(),
            blurred: backdrop.blurred.id(),
        };
        let group = standalone
            .as_ref()
            .and_then(|member| groups.get(member.group).ok());
        let changed = glass.as_ref().is_some_and(|value| value.is_changed())
            || member.as_ref().is_some_and(|value| value.is_changed())
            || standalone.as_ref().is_some_and(|value| value.is_changed())
            || group.as_ref().is_some_and(|value| value.is_changed());
        if !changed && material.is_some() && previous == Some(&inputs) {
            continue;
        }
        let (shape, style) = if let Some(glass) = &glass {
            (&glass.shape, &glass.style)
        } else {
            let (Some(member), Some(group)) = (&member, &group) else {
                continue;
            };
            (&member.shape, &group.style)
        };
        let uniform =
            GlassUniform::new(style, backdrop.size.as_vec2(), inputs.scale).with_shape(*shape);
        let handle = shared.resolve(uniform, &backdrop.source, &backdrop.blurred, &mut materials);
        if material.is_none_or(|material| material.0 != handle) {
            commands.entity(entity).insert(MaterialNode(handle));
        }
        if previous != Some(&inputs) {
            commands.entity(entity).insert(inputs);
        }
        statistics.node_updates += 1;
    }
    statistics.shared_materials = shared.len();
    if let Some(started) = started {
        statistics.node_sync_us = started.elapsed().as_secs_f64() * 1_000_000.0;
    }
}
