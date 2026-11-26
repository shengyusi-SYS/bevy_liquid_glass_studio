use std::collections::{HashMap, HashSet};

use bevy::{
    asset::AssetId,
    camera::visibility::VisibilitySystems,
    ecs::system::SystemParam,
    prelude::*,
    ui::{ComputedStackIndex, UiSystems},
};

use crate::{
    GlassNode, GlassShape, GlassStatistics, GlassStyle,
    material::{
        GlassGroupMaterial, GlassGroupShape, GlassGroupUniform, GlassUniform, MAX_GROUP_SHAPES,
    },
    ui::{BackdropImages, GlassUiSystems},
};

/// Maximum number of [`GlassMember`] surfaces fused by one [`GlassGroup`].
///
/// Additional members render individually using the group's optical style.
/// Split large interfaces into groups to keep each union's fragment work small.
pub const MAX_GLASS_GROUP_NODES: usize = MAX_GROUP_SHAPES;

/// Geometry of a surface that participates in its nearest ancestor [`GlassGroup`].
///
/// The group supplies its optical style and shadow. A member without an ancestor
/// group does not render glass; replace it with [`GlassNode`] for an independent
/// surface. Native UI content, layout and input remain available in either case.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default, Clone)]
#[require(Node)]
pub struct GlassMember {
    pub shape: GlassShape,
}

/// Fuses descendant [`GlassMember`] surfaces into one liquid glass shape.
///
/// Put this component on an ordinary UI container. A member belongs to its
/// nearest ancestor group; its layout, children and input behavior stay native
/// Bevy UI. The group's style controls optics and shadow, while each member's
/// [`GlassShape`] controls geometry. [`GlassNode`] surfaces stay independent.
///
/// The union is drawn behind the container's children, so place background
/// content before the group container. Apply overflow clipping to this container
/// or its ancestors. Member shapes support axis-aligned translation and scaling.
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default, Clone)]
#[require(Node)]
pub struct GlassGroup {
    pub style: GlassStyle,
    /// Smooth union distance in logical pixels. Zero makes a hard union.
    pub blend_radius: f32,
}

impl Default for GlassGroup {
    fn default() -> Self {
        Self {
            style: GlassStyle::default(),
            blend_radius: 40.0,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GroupMember {
    pub group: Entity,
}

/// An overflow member still gets the current group's style through the ordinary
/// material path. No optical settings are copied into the member itself.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StandaloneMember {
    pub group: Entity,
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct GroupSurface;

#[derive(Component)]
struct GroupSurfaceEntity(Entity);

#[derive(Component, Default)]
struct GroupMembers(Vec<Entity>);

#[derive(Default)]
struct MembershipState {
    owners: HashMap<Entity, Entity>,
    groups: HashMap<Entity, Vec<Entity>>,
    candidates: HashSet<Entity>,
    dirty_groups: HashSet<Entity>,
    visited: HashSet<Entity>,
    walk: Vec<Entity>,
}

#[derive(SystemParam)]
struct MembershipChanges<'w, 's> {
    added_members: Query<'w, 's, Entity, Added<GlassMember>>,
    added_nodes: Query<'w, 's, Entity, Added<GlassNode>>,
    added_groups: Query<'w, 's, Entity, Added<GlassGroup>>,
    changed_parents: Query<'w, 's, Entity, Changed<ChildOf>>,
    removed_members: RemovedComponents<'w, 's, GlassMember>,
    removed_nodes: RemovedComponents<'w, 's, GlassNode>,
    removed_groups: RemovedComponents<'w, 's, GlassGroup>,
    removed_parents: RemovedComponents<'w, 's, ChildOf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BackdropBinding {
    size: UVec2,
    source: AssetId<Image>,
    blurred: AssetId<Image>,
}

impl From<&BackdropImages> for BackdropBinding {
    fn from(images: &BackdropImages) -> Self {
        Self {
            size: images.size,
            source: images.source.id(),
            blurred: images.blurred.id(),
        }
    }
}

#[derive(Clone, Copy)]
struct SurfaceGeometry {
    size: Vec2,
    transform: UiGlobalTransform,
}

impl SurfaceGeometry {
    fn apply(
        self,
        computed: &mut Mut<'_, ComputedNode>,
        transform: &mut Mut<'_, UiGlobalTransform>,
    ) {
        if computed.size != self.size {
            computed.size = self.size;
        }
        if computed.unrounded_size != self.size {
            computed.unrounded_size = self.size;
        }
        if **transform != self.transform {
            **transform = self.transform;
        }
    }
}

#[derive(Default)]
struct GroupSynchronization {
    dirty: HashSet<Entity>,
    material_dirty: HashSet<Entity>,
    dirty_cameras: HashSet<Entity>,
    material_cameras: HashSet<Entity>,
    bindings: HashMap<Entity, BackdropBinding>,
    geometry: HashMap<Entity, SurfaceGeometry>,
    modified_images: HashSet<AssetId<Image>>,
    shapes: Vec<(u32, GlassGroupShape)>,
    packed: Vec<GlassGroupShape>,
}

#[derive(SystemParam)]
struct GroupChanges<'w, 's> {
    groups: Query<
        'w,
        's,
        Entity,
        (
            With<GlassGroup>,
            Without<GroupSurface>,
            Or<(
                Changed<GlassGroup>,
                Changed<GroupMembers>,
                Changed<ComputedNode>,
                Changed<UiGlobalTransform>,
                Changed<ComputedUiTargetCamera>,
            )>,
        ),
    >,
    members: Query<
        'w,
        's,
        &'static GroupMember,
        (
            With<GlassMember>,
            Without<GroupSurface>,
            Or<(
                Changed<GroupMember>,
                Changed<GlassMember>,
                Changed<ComputedNode>,
                Changed<UiGlobalTransform>,
                Changed<ComputedStackIndex>,
                Changed<InheritedVisibility>,
                Changed<CalculatedClip>,
            )>,
        ),
    >,
    cameras: Query<'w, 's, (Entity, &'static BackdropImages), Changed<BackdropImages>>,
    removed_cameras: RemovedComponents<'w, 's, BackdropImages>,
    removed_clips: RemovedComponents<'w, 's, CalculatedClip>,
    removed_surfaces: RemovedComponents<'w, 's, GroupSurface>,
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GlassGroupSystems {
    Membership,
    Synchronize,
}

pub(crate) struct GlassGroupsPlugin;

impl Plugin for GlassGroupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            synchronize_membership
                .in_set(GlassGroupSystems::Membership)
                .before(UiSystems::Prepare)
                .before(UiSystems::Stack)
                .before(VisibilitySystems::VisibilityPropagate),
        )
        .add_systems(
            PostUpdate,
            synchronize_groups
                .in_set(GlassGroupSystems::Synchronize)
                .after(UiSystems::Stack)
                .after(VisibilitySystems::VisibilityPropagate)
                .after(GlassUiSystems::PrepareBackdrops)
                .before(GlassUiSystems::Materials),
        );
    }
}

fn synchronize_membership(
    mut commands: Commands,
    mut state: Local<MembershipState>,
    mut statistics: ResMut<GlassStatistics>,
    groups: Query<(Entity, Option<&GroupSurfaceEntity>), With<GlassGroup>>,
    surface_entities: Query<&GroupSurfaceEntity>,
    surfaces: Query<(), With<GroupSurface>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    members: Query<
        (Option<&GroupMember>, Option<&StandaloneMember>),
        (With<GlassMember>, Without<GlassNode>),
    >,
    mut group_members: Query<&mut GroupMembers>,
    mut changes: MembershipChanges,
) {
    let started = statistics.collect_timings.then(std::time::Instant::now);
    let MembershipState {
        owners,
        groups: memberships,
        candidates,
        dirty_groups,
        visited,
        walk,
    } = &mut *state;
    candidates.clear();
    dirty_groups.clear();
    visited.clear();
    walk.clear();
    candidates.extend(changes.added_members.iter());
    candidates.extend(changes.added_nodes.iter());
    candidates.extend(changes.removed_members.read());
    candidates.extend(changes.removed_nodes.read());
    walk.extend(changes.changed_parents.iter());
    walk.extend(changes.removed_parents.read());

    for group in changes.removed_groups.read() {
        if groups.contains(group) {
            continue;
        }
        if let Some(previous) = memberships.get(&group) {
            candidates.extend(previous.iter().copied());
        }
        if let Ok(surface) = surface_entities.get(group) {
            if surfaces.contains(surface.0) {
                commands.entity(surface.0).despawn();
            }
            commands
                .entity(group)
                .remove::<(GroupSurfaceEntity, GroupMembers)>();
        }
        dirty_groups.insert(group);
    }
    for group in &changes.added_groups {
        let Ok((_, previous)) = groups.get(group) else {
            continue;
        };
        if previous.is_none_or(|surface| !surfaces.contains(surface.0)) {
            let surface = commands
                .spawn((
                    Name::new("Glass group surface"),
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(0.0),
                        height: Val::Px(0.0),
                        ..default()
                    },
                    ZIndex(-1),
                    Pickable::IGNORE,
                    GroupSurface,
                    ChildOf(group),
                ))
                .id();
            commands
                .entity(group)
                .insert((GroupSurfaceEntity(surface), GroupMembers::default()));
        }
        dirty_groups.insert(group);
        walk.push(group);
    }
    if candidates.is_empty() && walk.is_empty() && dirty_groups.is_empty() {
        if let Some(started) = started {
            statistics.membership_sync_us = started.elapsed().as_secs_f64() * 1_000_000.0;
        }
        return;
    }

    // Only hierarchy edits visit descendants. Deduplication keeps initial tree
    // construction and multiple edits within one subtree to one walk per node.
    while let Some(entity) = walk.pop() {
        if !visited.insert(entity) {
            continue;
        }
        if members.contains(entity) {
            candidates.insert(entity);
        }
        if let Ok(children) = children.get(entity) {
            walk.extend(children.iter());
        }
    }
    for entity in candidates.drain() {
        let previous = owners.get(&entity).copied();
        let next = members
            .contains(entity)
            .then(|| {
                parents
                    .iter_ancestors(entity)
                    .find(|parent| groups.contains(*parent))
            })
            .flatten();
        if previous == next {
            continue;
        }
        if let Some(previous) = previous {
            if let Some(members) = memberships.get_mut(&previous) {
                members.retain(|member| *member != entity);
            }
            owners.remove(&entity);
            dirty_groups.insert(previous);
        }
        if let Some(next) = next {
            owners.insert(entity, next);
            memberships.entry(next).or_default().push(entity);
            dirty_groups.insert(next);
        } else if let Ok(mut entity) = commands.get_entity(entity) {
            entity.remove::<(GroupMember, StandaloneMember)>();
        }
    }

    for group in dirty_groups.drain() {
        statistics.membership_rebuilds += 1;
        if !groups.contains(group) {
            memberships.remove(&group);
            continue;
        }
        let members_in_group = memberships.entry(group).or_default();
        members_in_group.sort_unstable_by_key(|entity| entity.to_bits());
        let fused = &members_in_group[..members_in_group.len().min(MAX_GLASS_GROUP_NODES)];
        if let Ok(mut current) = group_members.get_mut(group) {
            if current.0.as_slice() != fused {
                current.0.clear();
                current.0.extend_from_slice(fused);
            }
        } else {
            commands.entity(group).insert(GroupMembers(fused.to_vec()));
        }
        for (index, entity) in members_in_group.iter().copied().enumerate() {
            let Ok((previous, standalone)) = members.get(entity) else {
                continue;
            };
            if index < MAX_GLASS_GROUP_NODES {
                if previous.is_none_or(|previous| previous.group != group) {
                    commands.entity(entity).insert(GroupMember { group });
                }
                if standalone.is_some() {
                    commands.entity(entity).remove::<StandaloneMember>();
                }
            } else {
                if standalone.is_none_or(|standalone| standalone.group != group) {
                    commands.entity(entity).insert(StandaloneMember { group });
                }
                if previous.is_some() {
                    commands.entity(entity).remove::<GroupMember>();
                }
            }
        }
    }
    if let Some(started) = started {
        statistics.membership_sync_us = started.elapsed().as_secs_f64() * 1_000_000.0;
    }
}

fn synchronize_groups(
    mut commands: Commands,
    mut state: Local<GroupSynchronization>,
    mut statistics: ResMut<GlassStatistics>,
    mut materials: ResMut<Assets<GlassGroupMaterial>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    cameras: Query<&BackdropImages>,
    groups: Query<
        (
            Entity,
            &GlassGroup,
            &GroupSurfaceEntity,
            &GroupMembers,
            &ComputedNode,
            &UiGlobalTransform,
            &ComputedUiTargetCamera,
        ),
        Without<GroupSurface>,
    >,
    members: Query<
        (
            &GroupMember,
            &GlassMember,
            &ComputedNode,
            &UiGlobalTransform,
            &ComputedStackIndex,
            &InheritedVisibility,
            Option<&CalculatedClip>,
        ),
        Without<GroupSurface>,
    >,
    mut surfaces: Query<
        (
            Entity,
            &GroupSurface,
            &mut Node,
            &mut ComputedNode,
            &mut UiGlobalTransform,
            Option<&MaterialNode<GlassGroupMaterial>>,
        ),
        (
            With<GroupSurface>,
            Without<GlassGroup>,
            Without<GlassNode>,
            Without<GlassMember>,
        ),
    >,
    mut changes: GroupChanges,
) {
    let started = statistics.collect_timings.then(std::time::Instant::now);
    let GroupSynchronization {
        dirty,
        material_dirty,
        dirty_cameras,
        material_cameras,
        bindings,
        geometry,
        modified_images,
        shapes,
        packed,
    } = &mut *state;
    dirty.clear();
    material_dirty.clear();
    dirty_cameras.clear();
    material_cameras.clear();
    modified_images.clear();
    for event in image_events.read() {
        if let AssetEvent::Modified { id } = event {
            modified_images.insert(*id);
        }
    }
    dirty.extend(changes.groups.iter());
    dirty.extend(changes.members.iter().map(|member| member.group));
    for entity in changes.removed_clips.read() {
        if let Ok((member, ..)) = members.get(entity) {
            dirty.insert(member.group);
        }
    }
    for surface in changes.removed_surfaces.read() {
        geometry.remove(&surface);
    }
    for (entity, images) in &changes.cameras {
        let next = BackdropBinding::from(images);
        match bindings.insert(entity, next) {
            Some(previous) if previous.size == next.size => {
                if previous != next {
                    material_cameras.insert(entity);
                }
            }
            _ => {
                dirty_cameras.insert(entity);
            }
        }
    }
    for entity in changes.removed_cameras.read() {
        if cameras.get(entity).is_err() {
            bindings.remove(&entity);
            material_cameras.insert(entity);
        }
    }
    if !modified_images.is_empty() {
        for (camera, binding) in bindings.iter() {
            if modified_images.contains(&binding.source)
                || modified_images.contains(&binding.blurred)
            {
                material_cameras.insert(*camera);
            }
        }
    }
    if !dirty_cameras.is_empty() || !material_cameras.is_empty() {
        for (entity, .., camera) in &groups {
            if let Some(camera) = camera.get() {
                if dirty_cameras.contains(&camera) {
                    dirty.insert(entity);
                }
                if material_cameras.contains(&camera) {
                    material_dirty.insert(entity);
                }
            }
        }
    }

    // Layout may rewrite a transformed overlay. Restore its cached render
    // geometry without visiting members or rebuilding a material uniform.
    for (entity, _, _, mut computed, mut transform, _) in &mut surfaces {
        if (computed.is_changed() || transform.is_changed())
            && let Some(geometry) = geometry.get(&entity)
        {
            geometry.apply(&mut computed, &mut transform);
        }
    }
    for group_entity in dirty.drain() {
        let refresh_binding = material_dirty.remove(&group_entity);
        let Ok((_, glass, surface, group_members, parent, parent_transform, camera)) =
            groups.get(group_entity)
        else {
            continue;
        };
        let Ok((entity, _, mut node, mut computed, mut transform, material)) =
            surfaces.get_mut(surface.0)
        else {
            continue;
        };
        statistics.group_updates += 1;
        shapes.clear();
        for member in &group_members.0 {
            let Ok((_, glass, computed, transform, stack, visibility, clip)) = members.get(*member)
            else {
                continue;
            };
            if computed.is_empty()
                || !visibility.get()
                || clip.is_some_and(CalculatedClip::is_fully_clipped)
            {
                continue;
            }
            let (scale, _, center) = transform.to_scale_angle_translation();
            let scale = scale.abs();
            let size = computed.size() * scale;
            if size.min_element() <= 0.0 {
                continue;
            }
            shapes.push((
                stack.0,
                GlassGroupShape::new(
                    center,
                    size,
                    glass.shape.corner_radius
                        * computed.inverse_scale_factor().recip()
                        * scale.min_element(),
                    glass.shape.roundness,
                ),
            ));
        }
        shapes.sort_unstable_by_key(|(stack, _)| *stack);
        packed.clear();
        packed.extend(shapes.iter().map(|(_, shape)| *shape));
        if packed.is_empty() {
            if node.width != Val::Px(0.0) || node.height != Val::Px(0.0) {
                node.width = Val::Px(0.0);
                node.height = Val::Px(0.0);
            }
            let resolved = SurfaceGeometry {
                size: Vec2::ZERO,
                transform: *transform,
            };
            resolved.apply(&mut computed, &mut transform);
            geometry.insert(entity, resolved);
            if material.is_some() {
                commands
                    .entity(entity)
                    .remove::<MaterialNode<GlassGroupMaterial>>();
            }
            continue;
        }
        let scale_factor = parent.inverse_scale_factor().recip();
        let Some(backdrop) = camera.get().and_then(|camera| cameras.get(camera).ok()) else {
            if material.is_some() {
                commands
                    .entity(entity)
                    .remove::<MaterialNode<GlassGroupMaterial>>();
            }
            continue;
        };
        let uniform = GlassUniform::new(&glass.style, backdrop.size.as_vec2(), scale_factor);
        let mut bounds = Rect::from_center_size(packed[0].center, packed[0].half_size * 2.0);
        for shape in &packed[1..] {
            bounds = bounds.union(Rect::from_center_size(shape.center, shape.half_size * 2.0));
        }
        let shadow = &glass.style.shadow;
        let shadow_padding = if shadow.color.to_linear().alpha > 0.0 {
            shadow.offset.abs().max_element() + shadow.blur * 3.0 + shadow.spread.max(0.0)
        } else {
            0.0
        };
        let padding =
            ((shadow_padding + glass.blend_radius) * scale_factor).max(uniform.edge_feather);
        bounds.min -= Vec2::splat(padding);
        bounds.max += Vec2::splat(padding);

        let local_min =
            parent_transform.inverse().transform_point2(bounds.min) + parent.size() * 0.5;
        let logical_min = local_min * parent.inverse_scale_factor();
        let logical_size = bounds.size() * parent.inverse_scale_factor();
        let left = Val::Px(logical_min.x);
        let top = Val::Px(logical_min.y);
        let width = Val::Px(logical_size.x);
        let height = Val::Px(logical_size.y);
        if node.left != left || node.top != top || node.width != width || node.height != height {
            node.left = left;
            node.top = top;
            node.width = width;
            node.height = height;
        }
        let resolved = SurfaceGeometry {
            size: bounds.size(),
            transform: UiGlobalTransform::from_translation(bounds.center()),
        };
        resolved.apply(&mut computed, &mut transform);
        geometry.insert(entity, resolved);

        let group = GlassGroupUniform::new(
            packed,
            glass.blend_radius,
            &glass.style.shadow,
            scale_factor,
        );
        if let Some(material) = material {
            if let Some(mut current) = materials.get_mut(&material.0) {
                if current.uniform != uniform
                    || current.group != group
                    || current.source_image != backdrop.source
                    || current.blurred_image != backdrop.blurred
                    || refresh_binding
                {
                    current.uniform = uniform;
                    current.group = group;
                    current.source_image = backdrop.source.clone();
                    current.blurred_image = backdrop.blurred.clone();
                }
            }
        } else {
            commands
                .entity(entity)
                .insert(MaterialNode(materials.add(GlassGroupMaterial {
                    uniform,
                    group,
                    source_image: backdrop.source.clone(),
                    blurred_image: backdrop.blurred.clone(),
                })));
        }
    }
    for group_entity in material_dirty.drain() {
        let Ok((_, _, surface, .., camera)) = groups.get(group_entity) else {
            continue;
        };
        let Ok((entity, _, _, _, _, material)) = surfaces.get_mut(surface.0) else {
            continue;
        };
        let Some(backdrop) = camera.get().and_then(|camera| cameras.get(camera).ok()) else {
            if material.is_some() {
                commands
                    .entity(entity)
                    .remove::<MaterialNode<GlassGroupMaterial>>();
            }
            continue;
        };
        if let Some(material) = material
            && let Some(mut current) = materials.get_mut(&material.0)
        {
            // Modified Images need fresh GPU bindings even if handles are equal.
            current.source_image = backdrop.source.clone();
            current.blurred_image = backdrop.blurred.clone();
        }
    }
    if let Some(started) = started {
        statistics.group_sync_us = started.elapsed().as_secs_f64() * 1_000_000.0;
    }
}
