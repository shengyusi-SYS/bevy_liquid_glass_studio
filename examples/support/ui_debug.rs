//! Optional BRP tools for the interactive example.

use std::{io::Cursor, net::Ipv4Addr};

use super::{EffectivePreview, PreviewSettings, preview_config};
use bevy::reflect::serde::TypedReflectSerializer;
use bevy::{
    diagnostic::{DiagnosticsStore, FrameCount},
    picking::hover::Hovered,
    prelude::*,
    remote::{BrpError, BrpResult, RemotePlugin, http::RemoteHttpPlugin},
    render::{
        diagnostic::RenderDiagnosticsPlugin,
        view::window::screenshot::{Screenshot, ScreenshotCaptured},
    },
    ui::Pressed,
    window::{PrimaryWindow, WindowEvent},
};
use bevy_liquid_glass::{GlassBackdrop, GlassGroup, GlassMember, GlassNode, GlassStatistics};
use serde_json::{Value, json};

/// Example-only UI inspection and input, available at localhost:15704.
pub struct UiDebugPlugin;

impl Plugin for UiDebugPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PreviewSettings>()
            .register_type::<EffectivePreview>()
            .register_type::<Window>()
            .register_type::<Node>()
            .register_type::<Visibility>()
            .register_type::<bevy::camera::CompositingSpace>()
            .init_resource::<Capture>()
            .add_plugins((
                RemotePlugin::default()
                    .with_method_main("glass/stats", statistics)
                    .with_method_main("glass/diagnostics", diagnostics)
                    .with_method_main("glass/ui_snapshot", snapshot)
                    .with_method_main("glass/rust_config", rust_config)
                    .with_method_main("glass/input", input)
                    .with_method_main("glass/capture", capture)
                    .with_method_main("glass/capture_status", capture_status),
                RemoteHttpPlugin::default()
                    .with_address(Ipv4Addr::LOCALHOST)
                    .with_port(15704),
            ));
        let benchmark = std::env::var_os("GLASS_SYNC_BENCHMARK").is_some();
        if benchmark || std::env::var_os("GLASS_RENDER_DIAGNOSTICS").is_some() {
            app.add_plugins(RenderDiagnosticsPlugin);
        }
        if benchmark {
            install_sync_benchmark(app);
        }
    }
}

// An opt-in development measurement for backends without GPU timestamp queries.
// Waiting for GPU completion changes pipelining; this is not frame/FPS timing.
fn install_sync_benchmark(app: &mut App) {
    use bevy::render::{
        RenderApp,
        render_resource::PollType,
        renderer::{RenderDevice, RenderGraph, RenderGraphSystems},
    };
    #[derive(Resource)]
    struct Timing {
        start: std::time::Instant,
        frames: u32,
        total: std::time::Duration,
    }
    app.sub_app_mut(RenderApp)
        .insert_resource(Timing { start: std::time::Instant::now(), frames: 0, total: std::time::Duration::ZERO })
        .add_systems(RenderGraph, (
            (|mut timing: ResMut<Timing>| timing.start = std::time::Instant::now())
                .in_set(RenderGraphSystems::Begin),
            (|device: Res<RenderDevice>, mut timing: ResMut<Timing>| {
                device.poll(PollType::wait_indefinitely()).expect("GPU benchmark wait failed");
                timing.frames += 1;
                if timing.frames > 60 {
                    let elapsed = timing.start.elapsed();
                    timing.total += elapsed;
                }
                if timing.frames == 180 {
                    info!("Glass sync benchmark: {:.3} ms/frame (120 samples, CPU + GPU completion, excludes presentation and frame cap)", timing.total.as_secs_f64() * 1000.0 / 120.0);
                    timing.frames = 60;
                    timing.total = std::time::Duration::ZERO;
                }
            }).after(RenderGraphSystems::Submit).before(RenderGraphSystems::Finish),
        ));
}

fn diagnostics(_: In<Option<Value>>, store: Res<DiagnosticsStore>) -> BrpResult {
    Ok(json!(store.iter().filter(|d| d.path().as_str().contains("liquid_glass")).map(|d| {
        json!({"path": d.path().as_str(), "value": d.value(), "average": d.average(), "suffix": d.suffix})
    }).collect::<Vec<_>>()))
}

fn statistics(
    _: In<Option<Value>>,
    stats: Res<GlassStatistics>,
    frame: Res<FrameCount>,
) -> BrpResult {
    Ok(json!({
        "frame": frame.0,
        "collect_timings": stats.collect_timings,
        "node_sync_us": stats.node_sync_us,
        "group_sync_us": stats.group_sync_us,
        "membership_sync_us": stats.membership_sync_us,
        "node_updates": stats.node_updates,
        "group_updates": stats.group_updates,
        "membership_rebuilds": stats.membership_rebuilds,
        "shared_materials": stats.shared_materials,
        "backdrop_updates": stats.backdrop_updates,
    }))
}

fn input(params: In<Option<Value>>, world: &mut World) -> BrpResult {
    let event: WindowEvent =
        serde_json::from_value(params.0.unwrap_or(Value::Null)).map_err(invalid_params)?;
    // Match both sides of winit's input path: window state and normal messages.
    match &event {
        WindowEvent::CursorMoved(value) => {
            world
                .get_mut::<Window>(value.window)
                .ok_or_else(|| invalid_params("Window does not exist"))?
                .set_cursor_position(Some(value.position));
            world.write_message(value.clone());
        }
        WindowEvent::MouseButtonInput(value) => {
            world.write_message(*value);
        }
        WindowEvent::MouseWheel(value) => {
            world.write_message(*value);
        }
        WindowEvent::FileDragAndDrop(value) => {
            world.write_message(value.clone());
        }
        WindowEvent::KeyboardInput(value) => {
            world.write_message(value.clone());
        }
        WindowEvent::Ime(value) => {
            world.write_message(value.clone());
        }
        _ => {
            return Err(invalid_params(
                "Expected cursor, mouse, keyboard, IME or file-drop input",
            ));
        }
    }
    world.write_message(event);
    Ok(json!({"accepted": true}))
}

fn invalid_params(error: impl std::fmt::Display) -> BrpError {
    BrpError {
        code: -32602,
        message: error.to_string(),
        data: None,
    }
}

fn reflected(value: &dyn PartialReflect, registry: &bevy::reflect::TypeRegistry) -> Value {
    serde_json::to_value(TypedReflectSerializer::new(value, registry))
        .expect("Preview types are registered for reflection")
}

fn rust_config(
    _: In<Option<Value>>,
    settings: Res<PreviewSettings>,
    effective: Res<EffectivePreview>,
) -> BrpResult {
    Ok(json!({"code": preview_config::rust_config(&settings, &effective)}))
}

fn snapshot(
    _: In<Option<Value>>,
    frame: Res<FrameCount>,
    registry: Res<AppTypeRegistry>,
    settings: Res<PreviewSettings>,
    effective: Res<EffectivePreview>,
    stats: Res<GlassStatistics>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    nodes: Query<(
        Entity,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&Name>,
        Option<&Text>,
        Option<&ChildOf>,
        Option<&ScrollPosition>,
        Option<&Hovered>,
        Option<&Pressed>,
        Option<&GlassNode>,
        Option<&GlassMember>,
    )>,
    parents: Query<&ChildOf>,
    groups: Query<&GlassGroup>,
    backdrops: Query<(Entity, &GlassBackdrop)>,
) -> BrpResult {
    let registry = registry.read();
    let windows: Vec<_> = windows
        .iter()
        .map(|(entity, window)| {
            json!({
                "entity": entity.to_bits(), "width": window.width(), "height": window.height(),
                "scale_factor": window.scale_factor(),
                "cursor": window.cursor_position().map(|p| [p.x, p.y]),
            })
        })
        .collect();
    let nodes: Vec<_> = nodes
        .iter()
        .map(
            |(
                entity,
                node,
                transform,
                name,
                text,
                parent,
                scroll,
                hovered,
                pressed,
                glass,
                member,
            )| {
                let group_entity = member.and_then(|_| {
                    parents
                        .iter_ancestors(entity)
                        .find(|ancestor| groups.contains(*ancestor))
                });
                let own_group = groups.get(entity).ok();
                let style = glass
                    .map(|glass| &glass.style)
                    .or_else(|| own_group.map(|group| &group.style))
                    .or_else(|| {
                        group_entity
                            .and_then(|id| groups.get(id).ok())
                            .map(|group| &group.style)
                    });
                let shape = glass
                    .map(|glass| &glass.shape)
                    .or_else(|| member.map(|member| &member.shape));
                let kind = if glass.is_some() {
                    Some("standalone")
                } else if member.is_some() {
                    Some("member")
                } else if own_group.is_some() {
                    Some("group")
                } else {
                    None
                };
                let actual_scroll = node.scroll_position * node.inverse_scale_factor();
                json!({
                    "entity": entity.to_bits(), "name": name.map(Name::as_str),
                    "text": text.map(|text| text.0.as_str()),
                    "parent": parent.map(|parent| parent.parent().to_bits()),
                    "center_physical": [transform.translation.x, transform.translation.y],
                    "size_physical": [node.size().x, node.size().y],
                    "scroll_requested": scroll.map(|scroll| [scroll.0.x, scroll.0.y]),
                    "scroll_actual": [actual_scroll.x, actual_scroll.y],
                    "hovered": hovered.map(|state| state.0), "pressed": pressed.is_some(),
                    "glass_kind": kind, "glass_group": group_entity.map(Entity::to_bits),
                    "glass_shape": shape.map(|shape| reflected(shape, &registry)),
                    "glass_style": style.map(|style| reflected(style, &registry)),
                })
            },
        )
        .collect();
    let backdrops: Vec<_> = backdrops
        .iter()
        .map(|(entity, backdrop)| {
            json!({
                "entity": entity.to_bits(), "blur_radius": backdrop.blur_radius,
                "blur_quality": format!("{:?}", backdrop.blur_quality),
                "external_source": backdrop.source.is_some(),
                "refresh": format!("{:?}", backdrop.refresh),
            })
        })
        .collect();
    Ok(json!({
        "frame": frame.0, "windows": windows, "nodes": nodes, "backdrops": backdrops,
        "statistics": {
            "collect_timings": stats.collect_timings,
        "node_sync_us": stats.node_sync_us,
        "group_sync_us": stats.group_sync_us,
        "membership_sync_us": stats.membership_sync_us,
        "node_updates": stats.node_updates, "group_updates": stats.group_updates,
            "membership_rebuilds": stats.membership_rebuilds,
            "shared_materials": stats.shared_materials, "backdrop_updates": stats.backdrop_updates,
        },
        "preview": {"settings": reflected(&*settings, &registry), "effective": reflected(&*effective, &registry)},
    }))
}

#[derive(Resource, Default)]
struct Capture {
    ticket: u64,
    result: Option<Result<Vec<u8>, String>>,
}

fn capture(_: In<Option<Value>>, mut commands: Commands, mut state: ResMut<Capture>) -> BrpResult {
    state.ticket += 1;
    state.result = None;
    let ticket = state.ticket;
    commands.spawn(Screenshot::primary_window()).observe(
        move |event: On<ScreenshotCaptured>, mut state: ResMut<Capture>| {
            if state.ticket != ticket {
                return;
            }
            state.result = Some((|| {
                let image = event
                    .image
                    .clone()
                    .try_into_dynamic()
                    .map_err(|e| e.to_string())?;
                let mut bytes = Cursor::new(Vec::new());
                image
                    .write_to(&mut bytes, image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                Ok(bytes.into_inner())
            })());
        },
    );
    Ok(json!({"ticket": ticket}))
}

fn capture_status(params: In<Option<Value>>, state: Res<Capture>) -> BrpResult {
    let ticket = params
        .0
        .as_ref()
        .and_then(|value| value.get("ticket"))
        .and_then(Value::as_u64);
    if ticket != Some(state.ticket) || state.ticket == 0 {
        return Err(invalid_params("Unknown or superseded capture ticket"));
    }
    match &state.result {
        None => Ok(json!({"status": "pending"})),
        Some(Ok(png)) => Ok(json!({"status": "complete", "png": png})),
        Some(Err(message)) => Err(BrpError {
            code: -32603,
            message: message.clone(),
            data: None,
        }),
    }
}
