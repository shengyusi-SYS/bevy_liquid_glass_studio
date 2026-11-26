//! Adjustable developer load using the same public glass nodes as applications.
use bevy::prelude::*;
use bevy_liquid_glass::{GlassGroup, GlassMember, GlassNode};

use super::{DemoGlassLayer, PreviewSettings, apply_preview_settings};

#[derive(Component)]
pub(super) struct LoadNode;
#[derive(Component)]
pub(super) struct LoadGroup;
#[derive(Component)]
struct LoadRoot;
#[derive(Component)]
struct AnimatedLoad;

pub(super) fn install(app: &mut App) {
    app.add_systems(
        Update,
        (sync_load, animate_load)
            .chain()
            .after(apply_preview_settings),
    );
}

pub(super) fn spawn_root(commands: &mut Commands, viewport: Entity) {
    commands.spawn((
        Name::new("ScaleLoad"),
        LoadRoot,
        ChildOf(viewport),
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            left: px(36),
            right: px(36),
            top: px(80),
            bottom: px(80),
            ..default()
        },
    ));
}

fn grid(count: usize) -> Node {
    let columns = (count as f32).sqrt().ceil() as u16;
    let rows = count.div_ceil(columns as usize) as u16;
    Node {
        display: Display::Grid,
        min_width: px(0),
        min_height: px(0),
        grid_template_columns: vec![RepeatedGridTrack::flex(columns, 1.0)],
        grid_template_rows: vec![RepeatedGridTrack::flex(rows, 1.0)],
        column_gap: px(6),
        row_gap: px(6),
        ..default()
    }
}

fn spawn_node(
    commands: &mut Commands,
    parent: Entity,
    index: usize,
    settings: &PreviewSettings,
    grouped: bool,
) {
    let mut node = commands.spawn((
        Name::new(format!("LoadNode::{index}")),
        LoadNode,
        ChildOf(parent),
        Node {
            min_width: px(0),
            min_height: px(0),
            ..default()
        },
    ));
    if grouped {
        node.insert(GlassMember {
            shape: settings.shape,
        });
    } else {
        node.insert(GlassNode {
            shape: settings.shape,
            style: settings.style.clone(),
        });
    }
    if index == 0 {
        node.insert(AnimatedLoad);
    }
}

fn sync_load(
    mut commands: Commands,
    settings: Res<PreviewSettings>,
    mut topology: Local<Option<(usize, usize)>>,
    mut root: Single<(Entity, &mut Node), With<LoadRoot>>,
    mut demo: Single<&mut Node, (With<DemoGlassLayer>, Without<LoadRoot>)>,
    mut nodes: Query<&mut GlassNode, With<LoadNode>>,
    mut members: Query<&mut GlassMember, With<LoadNode>>,
    mut groups: Query<&mut GlassGroup, With<LoadGroup>>,
) {
    if !settings.is_changed() {
        return;
    }
    let count = settings.load_nodes;
    let group_count = settings.load_groups.min(count);
    if *topology != Some((count, group_count)) {
        *topology = Some((count, group_count));
        commands.entity(root.0).despawn_children();
        root.1.display = if count > 0 {
            Display::Grid
        } else {
            Display::None
        };
        demo.display = if count > 0 {
            Display::None
        } else {
            Display::Flex
        };
        if count == 0 {
            return;
        }
        let layout = grid(if group_count == 0 { count } else { group_count });
        root.1.grid_template_columns = layout.grid_template_columns;
        root.1.grid_template_rows = layout.grid_template_rows;
        root.1.column_gap = layout.column_gap;
        root.1.row_gap = layout.row_gap;
        if group_count == 0 {
            for index in 0..count {
                spawn_node(&mut commands, root.0, index, &settings, false);
            }
        } else {
            let mut index = 0;
            for group_index in 0..group_count {
                let members = count / group_count + usize::from(group_index < count % group_count);
                let group = commands
                    .spawn((
                        Name::new(format!("LoadGroup::{group_index}")),
                        LoadGroup,
                        ChildOf(root.0),
                        grid(members),
                        GlassGroup {
                            style: settings.style.clone(),
                            blend_radius: if settings.fusion_enabled {
                                settings.blend_radius
                            } else {
                                0.0
                            },
                        },
                    ))
                    .id();
                for _ in 0..members {
                    spawn_node(&mut commands, group, index, &settings, true);
                    index += 1;
                }
            }
        }
        info!(
            "Scale preview: {count} nodes, {group_count} fusion groups; animate_load={}",
            settings.animate_load
        );
        return;
    }
    for mut node in &mut nodes {
        if node.shape != settings.shape {
            node.shape = settings.shape;
        }
        if node.style != settings.style {
            node.style = settings.style.clone();
        }
    }
    for mut member in &mut members {
        if member.shape != settings.shape {
            member.shape = settings.shape;
        }
    }
    let blend = if settings.fusion_enabled {
        settings.blend_radius
    } else {
        0.0
    };
    for mut group in &mut groups {
        if group.style != settings.style {
            group.style = settings.style.clone();
        }
        if group.blend_radius != blend {
            group.blend_radius = blend;
        }
    }
}

fn animate_load(
    settings: Res<PreviewSettings>,
    time: Res<Time>,
    mut nodes: Query<&mut UiTransform, With<AnimatedLoad>>,
) {
    let transform = if settings.animate_load {
        UiTransform::from_translation(Val2::px(time.elapsed_secs().sin() * 12.0, 0.0))
    } else {
        UiTransform::IDENTITY
    };
    for mut node in &mut nodes {
        node.set_if_neq(transform);
    }
}
