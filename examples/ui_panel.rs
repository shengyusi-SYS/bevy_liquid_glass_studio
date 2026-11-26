//! Interactive native preview of the public glass-node interface.
//!
//! `cargo run --example ui_panel`
//! Add `--features ui-debug` to use `examples/ui_debug.py`.
//! An optional `--image /path/to/image.png` loads a background on startup.
//! `--reference` selects the supplied web reference's shape and optical settings.
//! `--nodes N [--groups G] [--animate-load] --on-demand` exercises larger layouts.

use std::{
    path::PathBuf,
    sync::{Mutex, mpsc},
};

use bevy::{
    asset::RenderAssetUsages,
    feathers::{FeathersPlugins, dark_theme::create_dark_theme, theme::UiTheme},
    image::{CompressedImageFormats, ImageSampler, ImageType},
    picking::hover::Hovered,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    ui::Pressed,
    ui_widgets::{Activate, Button},
};
use bevy_liquid_glass::{
    GlassBackdrop, GlassBackdropRefresh, GlassBlurQuality, GlassGroup, GlassLighting, GlassMember,
    GlassShape, GlassStyle, LiquidGlassPlugin,
};

#[path = "support/preview_controls.rs"]
mod controls;
#[cfg(target_os = "linux")]
#[path = "support/native_gl.rs"]
mod native_gl;
#[path = "support/preview_config.rs"]
mod preview_config;
#[path = "support/preview_load.rs"]
mod preview_load;
#[cfg(feature = "ui-debug")]
#[path = "support/ui_debug.rs"]
mod ui_debug;

const PANEL_WIDTH: f32 = 340.0;
const INK: Color = Color::srgb(0.9, 0.94, 0.98);
const MUTED: Color = Color::srgb(0.56, 0.65, 0.75);

#[derive(Resource, Reflect, Clone)]
#[reflect(Resource)]
struct PreviewSettings {
    shape: GlassShape,
    feedback: bool,
    advanced: bool,
    style: GlassStyle,
    blur_radius: f32,
    blur_quality: GlassBlurQuality,
    backdrop_refresh: GlassBackdropRefresh,
    load_nodes: usize,
    load_groups: usize,
    animate_load: bool,
    blend_radius: f32,
    fusion_enabled: bool,
    follow: bool,
    reference: bool,
    clicks: u32,
    width: f32,
    height: f32,
    tint_preset: usize,
    status: String,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        Self {
            shape: GlassShape::default(),
            feedback: false,
            advanced: false,
            style: GlassStyle::default(),
            blur_radius: 12.0,
            blur_quality: GlassBlurQuality::Auto,
            backdrop_refresh: GlassBackdropRefresh::EveryFrame,
            load_nodes: 0,
            load_groups: 0,
            animate_load: false,
            blend_radius: 40.0,
            fusion_enabled: true,
            follow: true,
            reference: false,
            clicks: 0,
            width: 254.0,
            height: 142.0,
            tint_preset: 0,
            status: "Built-in background | import your own PNG, JPEG or WebP".into(),
        }
    }
}

fn apply_reference_preset(state: &mut PreviewSettings) {
    let mut style = GlassStyle {
        lighting: GlassLighting::OriginalCanvas,
        refraction_thickness: 5.0,
        refraction_distance: 0.15,
        dispersion: 4.0,
        refractive_index: 1.2,
        fresnel_strength: 0.0,
        glare_strength: 0.13,
        tint: Color::srgba(1.0, 1.0, 1.0, 0.0),
        ..default()
    };
    // The remaining controls retain the upstream defaults; its soft shadow
    // uses exp(-distance/25) * 0.6 * 0.15 with a downward 10px offset.
    style.shadow.offset = Vec2::new(0.0, 10.0);
    style.shadow.blur = 25.0;
    style.shadow.color = Color::srgba(0.0, 0.0, 0.0, 0.09);
    let status = state.status.clone();
    *state = PreviewSettings {
        shape: GlassShape {
            corner_radius: 22.0,
            ..default()
        },
        style,
        blur_radius: 1.0,
        blur_quality: GlassBlurQuality::FullResolution,
        blend_radius: 0.0,
        fusion_enabled: false,
        follow: false,
        reference: true,
        width: 902.0,
        height: 110.0,
        status,
        ..default()
    };
}

/// Resolved values used by the visible glass surface after optional feedback.
#[derive(Resource, Reflect, Clone, Default, PartialEq)]
#[reflect(Resource)]
struct EffectivePreview {
    shape: GlassShape,
    style: GlassStyle,
    blend_radius: f32,
    hovered: bool,
    pressed: bool,
}

type ImportResult = Result<(String, Image), String>;

#[derive(Resource, Default)]
struct PendingImport(Option<Mutex<mpsc::Receiver<Option<ImportResult>>>>);

#[derive(Component)]
struct PreviewViewport;
#[derive(Component)]
struct ImportedBackground;
#[derive(Component)]
struct FollowGlass;
#[derive(Component)]
struct StaticGlass;
#[derive(Component)]
struct ReferenceHidden;
#[derive(Component)]
struct DemoGlassLayer;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct ClickText;
#[derive(Component, Clone, Default)]
struct FollowText;
#[derive(Component, Clone, Default)]
struct FusionText;

fn main() {
    let mut app = App::new();
    let plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Liquid Glass | Bevy UI".into(),
            resolution: (1280, 900).into(),
            ..default()
        }),
        ..default()
    });
    #[cfg(target_os = "linux")]
    let plugins = native_gl::configure(plugins);
    app.add_plugins((plugins, FeathersPlugins, LiquidGlassPlugin))
        .add_plugins(bevy_framepace::FramepacePlugin)
        .insert_resource(bevy_framepace::FramepaceSettings {
            limiter: bevy_framepace::Limiter::from_framerate(30.0),
        })
        .insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_millis(100),
            ),
        })
        .insert_resource(UiTheme(create_dark_theme()))
        .init_resource::<PreviewSettings>()
        .init_resource::<EffectivePreview>()
        .init_resource::<PendingImport>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                follow_pointer,
                apply_preview_settings,
                poll_image_import,
                fit_background,
                update_labels,
                controls::sync_sliders.after(apply_preview_settings),
                accept_dropped_images,
            ),
        );
    preview_load::install(&mut app);
    #[cfg(feature = "ui-debug")]
    app.add_plugins(ui_debug::UiDebugPlugin);
    info!("Preview: 30 FPS cap, inactive updates every 100 ms");
    app.run();
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut pending: ResMut<PendingImport>,
    mut settings: ResMut<PreviewSettings>,
) {
    commands.spawn((
        Camera2d,
        GlassBackdrop::default(),
        Name::new("PreviewCamera"),
    ));
    let background = images.add(default_background());
    let root = commands
        .spawn((
            Name::new("LiquidGlassPreview"),
            Node {
                width: percent(100),
                height: vh(100),
                max_height: vh(100),
                min_height: px(0),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.045, 0.06, 0.085)),
        ))
        .id();
    let viewport = commands
        .spawn((
            Name::new("PreviewViewport"),
            PreviewViewport,
            ChildOf(root),
            Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                min_width: px(300),
                height: percent(100),
                max_height: percent(100),
                min_height: px(0),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    commands.spawn((
        Name::new("ImportedBackground"),
        ImportedBackground,
        ChildOf(viewport),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ImageNode::new(background).with_mode(NodeImageMode::Stretch),
        Pickable::IGNORE,
    ));
    commands.spawn((
        Name::new("BackgroundTypography"),
        ChildOf(viewport),
        Node {
            position_type: PositionType::Absolute,
            left: px(52),
            top: px(158),
            ..default()
        },
        Text::new("LIQUID\nGLASS"),
        TextFont {
            font_size: FontSize::Px(92.0),
            ..default()
        },
        TextColor(Color::srgba(0.98, 1.0, 1.0, 0.54)),
        Pickable::IGNORE,
    ));
    commands.spawn((
        Name::new("BackgroundCaption"),
        ChildOf(viewport),
        Node {
            position_type: PositionType::Absolute,
            left: px(54),
            top: px(380),
            ..default()
        },
        Text::new("A material for your UI.\nMove, click, and explore the light."),
        TextFont {
            font_size: FontSize::Px(21.0),
            ..default()
        },
        TextColor(Color::srgba(0.91, 0.98, 1.0, 0.76)),
        Pickable::IGNORE,
    ));
    // All ordinary background UI precedes the first sampling node.
    commands.spawn((
        Name::new("BackgroundStripe"),
        ChildOf(viewport),
        Node {
            position_type: PositionType::Absolute,
            left: percent(62),
            top: px(80),
            width: px(12),
            height: px(660),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.96, 0.85, 0.54, 0.7)),
        Pickable::IGNORE,
    ));
    preview_load::spawn_root(&mut commands, viewport);
    let glass_layer = commands
        .spawn((
            Name::new("GlassLayer"),
            DemoGlassLayer,
            ChildOf(viewport),
            GlassGroup {
                style: GlassStyle::default(),
                blend_radius: 40.0,
            },
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .id();
    let static_glass = commands
        .spawn((
            Name::new("StaticGlass"),
            StaticGlass,
            ReferenceHidden,
            ChildOf(glass_layer),
            GlassMember::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(52),
                bottom: px(88),
                width: px(286),
                height: px(176),
                padding: UiRect::all(px(24)),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                ..default()
            },
        ))
        .id();
    commands.spawn((
        ChildOf(static_glass),
        Text::new("Made for Bevy UI"),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(INK),
        Pickable::IGNORE,
    ));
    commands.spawn((
        ChildOf(static_glass),
        Text::new("Normal layout.\nClear text. Shared backdrop."),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgba(0.94, 0.98, 1.0, 0.84)),
        Pickable::IGNORE,
    ));
    let follow_glass = commands
        .spawn((
            Name::new("FollowGlass"),
            FollowGlass,
            ChildOf(glass_layer),
            GlassMember::default(),
            Button,
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(450),
                top: px(470),
                width: px(254),
                height: px(142),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(9),
                ..default()
            },
        ))
        .observe(on_glass_click)
        .id();
    commands.spawn((
        Name::new("FollowButtonLabel"),
        ReferenceHidden,
        ChildOf(follow_glass),
        Text::new("Follow the light"),
        TextFont {
            font_size: FontSize::Px(23.0),
            ..default()
        },
        TextColor(INK),
        Pickable::IGNORE,
    ));
    commands.spawn((
        Name::new("ClickCount"),
        ClickText,
        ReferenceHidden,
        ChildOf(follow_glass),
        Text::new("CLICK ME | 0"),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgba(0.92, 0.98, 1.0, 0.82)),
        Pickable::IGNORE,
    ));
    let sidebar = commands
        .spawn((
            Name::new("ControlsPanel"),
            ChildOf(root),
            Node {
                width: px(PANEL_WIDTH),
                min_width: px(PANEL_WIDTH),
                height: percent(100),
                max_height: percent(100),
                min_height: px(0),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                padding: UiRect::all(px(24)),
                flex_direction: FlexDirection::Column,
                row_gap: px(14),
                ..default()
            },
            BackgroundColor(Color::srgb(0.055, 0.07, 0.095)),
        ))
        .id();
    controls::spawn_panel(&mut commands, sidebar);
    // Header and guidance are foreground UI, excluded from the backdrop snapshot.
    let header = commands
        .spawn((
            ChildOf(viewport),
            Name::new("PreviewHeader"),
            Node {
                position_type: PositionType::Absolute,
                left: px(38),
                right: px(38),
                top: px(30),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.spawn((
        ChildOf(header),
        Text::new("BEVY / LIQUID GLASS"),
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        TextColor(INK),
        Pickable::IGNORE,
    ));
    commands.spawn((
        ChildOf(header),
        Text::new("NATIVE UI PLAYGROUND"),
        TextFont {
            font_size: FontSize::Px(11.0),
            ..default()
        },
        TextColor(INK.with_alpha(0.7)),
        Pickable::IGNORE,
    ));
    commands.spawn((
        ChildOf(viewport),
        Node {
            position_type: PositionType::Absolute,
            left: px(38),
            bottom: px(28),
            ..default()
        },
        Text::new("Import a background | compare material parameters | move cards in live mode ->"),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(INK.with_alpha(0.8)),
        Pickable::IGNORE,
    ));
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--reference" => apply_reference_preset(&mut settings),
            "--nodes" => {
                settings.load_nodes = args
                    .next()
                    .expect("--nodes requires a count")
                    .parse()
                    .expect("--nodes requires a nonnegative integer");
            }
            "--groups" => {
                settings.load_groups = args
                    .next()
                    .expect("--groups requires a count")
                    .parse()
                    .expect("--groups requires a nonnegative integer");
            }
            "--animate-load" => settings.animate_load = true,
            "--on-demand" => settings.backdrop_refresh = GlassBackdropRefresh::OnDemand,
            "--image" => {
                if let Some(path) = args.next() {
                    start_image_import(Some(PathBuf::from(path)), &mut pending);
                }
            }
            _ => {}
        }
    }
}

fn on_glass_click(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    state.clicks += 1;
}

fn follow_pointer(
    mut moves: MessageReader<CursorMoved>,
    settings: Res<PreviewSettings>,
    viewport: Single<(&ComputedNode, &UiGlobalTransform), With<PreviewViewport>>,
    mut glass: Single<(&mut Node, &ComputedNode, &UiGlobalTransform), With<FollowGlass>>,
) {
    let cursor = moves.read().last().map(|event| event.position);
    let (area, transform) = *viewport;
    let scale = area.inverse_scale_factor();
    let size = area.size() * scale;
    let glass_size = glass.1.size() * glass.1.inverse_scale_factor();
    if size.min_element() <= 0.0 || glass_size.min_element() <= 0.0 {
        return; // Wait for the first layout before constraining the preview.
    }
    let origin = transform.translation * scale - size * 0.5;
    let measured = glass.2.translation * scale - origin - glass_size * 0.5;
    // The example places this node with pixel or percentage insets.
    let inset = |value, reference, measured| match value {
        Val::Px(value) => value,
        Val::Percent(value) => reference * value / 100.0,
        _ => measured,
    };
    let current = Vec2::new(
        inset(glass.0.left, size.x, measured.x),
        inset(glass.0.top, size.y, measured.y),
    );
    let mut position = current;
    if settings.reference && !settings.follow {
        position = (size - glass_size) * 0.5;
    } else if settings.follow
        && let Some(cursor) = cursor
    {
        let local = cursor - origin;
        if Rect::from_corners(Vec2::ZERO, size).contains(local) {
            position = local - glass_size * 0.5;
        }
    }
    // Resizing still constrains the existing position when the pointer is idle,
    // over the controls, or following is disabled.
    position = position.clamp(Vec2::ZERO, (size - glass_size).max(Vec2::ZERO));
    if position.x != current.x {
        glass.0.left = px(position.x);
    }
    if position.y != current.y {
        glass.0.top = px(position.y);
    }
}

fn apply_preview_settings(
    settings: Res<PreviewSettings>,
    mut effective: ResMut<EffectivePreview>,
    mut cameras: Query<&mut GlassBackdrop>,
    mut members: Query<
        (&mut GlassMember, Option<&Hovered>, Has<Pressed>),
        Without<preview_load::LoadNode>,
    >,
    mut groups: Query<&mut GlassGroup, Without<preview_load::LoadGroup>>,
    mut follow: Single<&mut Node, With<FollowGlass>>,
    viewport: Single<&ComputedNode, With<PreviewViewport>>,
    mut reference_hidden: Query<&mut Node, (With<ReferenceHidden>, Without<FollowGlass>)>,
) {
    if settings.is_changed() {
        for mut backdrop in &mut cameras {
            if backdrop.blur_radius != settings.blur_radius {
                backdrop.blur_radius = settings.blur_radius;
            }
            if backdrop.blur_quality != settings.blur_quality {
                backdrop.blur_quality = settings.blur_quality;
            }
            if backdrop.refresh != settings.backdrop_refresh {
                backdrop.refresh = settings.backdrop_refresh;
            }
        }
        for mut node in &mut reference_hidden {
            node.display = if settings.reference {
                Display::None
            } else {
                Display::Flex
            };
        }
    }
    let available = viewport.size().x * viewport.inverse_scale_factor();
    let geometry_scale = if settings.reference && available > 0.0 {
        ((available - 72.0).max(1.0) / settings.width).min(1.0)
    } else {
        1.0
    };
    if settings.is_changed() || settings.reference {
        let width = px(settings.width * geometry_scale);
        let height = px(settings.height * geometry_scale);
        if follow.width != width {
            follow.width = width;
        }
        if follow.height != height {
            follow.height = height;
        }
    }
    let mut shape = settings.shape;
    shape.corner_radius *= geometry_scale;
    let (mut hovered, mut pressed) = (false, false);
    for (mut member, hover, press) in &mut members {
        hovered |= hover.is_some_and(|h| h.get());
        pressed |= press;
        if member.shape != shape {
            member.shape = shape;
        }
    }
    let mut style = settings.style.clone();
    if settings.feedback {
        if hovered {
            style.glare_strength = (style.glare_strength * 1.3).min(1.2);
            style.fresnel_strength += 0.1;
        }
        if pressed {
            style.refraction_distance *= 0.65;
        }
    }
    let blend_radius = if settings.fusion_enabled {
        settings.blend_radius
    } else {
        0.0
    };
    for mut group in &mut groups {
        if group.style != style {
            group.style = style.clone();
        }
        if group.blend_radius != blend_radius {
            group.blend_radius = blend_radius;
        }
    }
    effective.set_if_neq(EffectivePreview {
        shape,
        style,
        blend_radius,
        hovered,
        pressed,
    });
}

fn update_labels(
    settings: Res<PreviewSettings>,
    mut text: Query<(
        &mut Text,
        Has<StatusText>,
        Has<ClickText>,
        Has<FollowText>,
        Has<FusionText>,
    )>,
) {
    if !settings.is_changed() {
        return;
    }
    for (mut text, status, clicks, follow, fusion) in &mut text {
        if status {
            text.0 = if settings.load_nodes == 0 {
                settings.status.clone()
            } else {
                format!(
                    "{}\nLoad: {} nodes | {} groups | {:?}",
                    settings.status,
                    settings.load_nodes,
                    settings.load_groups.min(settings.load_nodes),
                    settings.backdrop_refresh
                )
            };
        }
        if clicks {
            text.0 = format!("CLICK ME | {}", settings.clicks);
        }
        if follow {
            text.0 = if settings.follow {
                "Follow pointer: on"
            } else {
                "Follow pointer: off"
            }
            .into();
        }
        if fusion {
            text.0 = if settings.fusion_enabled {
                "Smooth merge: on"
            } else {
                "Smooth merge: off"
            }
            .into();
        }
    }
}

fn start_image_import(path: Option<PathBuf>, pending: &mut PendingImport) {
    let (sender, receiver) = mpsc::channel();
    pending.0 = Some(Mutex::new(receiver));
    std::thread::spawn(move || {
        let path = path.or_else(|| {
            rfd::FileDialog::new()
                .set_title("Choose a preview background")
                .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                .pick_file()
        });
        let result = path.map(|path| {
            let label = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            let extension = path.extension().and_then(|v| v.to_str()).unwrap_or("png");
            let image = Image::from_buffer(
                &data,
                ImageType::Extension(extension),
                CompressedImageFormats::NONE,
                true,
                ImageSampler::linear(),
                RenderAssetUsages::default(),
            )
            .map_err(|e| e.to_string())?;
            Ok((label, image))
        });
        let _ = sender.send(result);
    });
}

fn poll_image_import(
    mut pending: ResMut<PendingImport>,
    mut state: ResMut<PreviewSettings>,
    mut images: ResMut<Assets<Image>>,
    mut backgrounds: Query<&mut ImageNode, With<ImportedBackground>>,
    mut backdrops: Query<&mut GlassBackdrop>,
) {
    let result = pending
        .0
        .as_ref()
        .and_then(|receiver| receiver.lock().unwrap().try_recv().ok());
    let Some(result) = result else {
        return;
    };
    pending.0 = None;
    match result {
        Some(Ok((label, image))) => {
            let handle = images.add(image);
            for mut node in &mut backgrounds {
                node.image = handle.clone();
            }
            for mut backdrop in &mut backdrops {
                backdrop.set_changed();
            }
            state.status = label;
        }
        Some(Err(error)) => state.status = format!("Could not load image: {error}"),
        None => state.status = "Image selection cancelled".into(),
    }
}

fn fit_background(
    viewport: Single<&ComputedNode, With<PreviewViewport>>,
    images: Res<Assets<Image>>,
    mut background: Single<&mut ImageNode, With<ImportedBackground>>,
    mut backdrops: Query<&mut GlassBackdrop>,
) {
    let Some(image) = images.get(&background.image) else {
        return;
    };
    let target = viewport.size();
    if target.y <= 0.0 {
        return;
    }
    let size = image.size().as_vec2();
    let target_aspect = target.x / target.y;
    let image_aspect = size.x / size.y;
    let cropped = if image_aspect > target_aspect {
        Vec2::new(size.y * target_aspect, size.y)
    } else {
        Vec2::new(size.x, size.x / target_aspect)
    };
    let min = (size - cropped) * 0.5;
    let rect = Some(Rect::from_corners(min, min + cropped));
    if background.rect != rect {
        background.rect = rect;
        for mut backdrop in &mut backdrops {
            backdrop.set_changed();
        }
    }
}

fn default_background() -> Image {
    // A small generated image keeps the example self-contained and makes refraction visible.
    let width = 960_u32;
    let height = 960_u32;
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let p = Vec2::new(x as f32 / width as f32, y as f32 / height as f32);
            let blue = (-(p - Vec2::new(0.65, 0.3)).length_squared() * 6.0).exp();
            let coral = (-(p - Vec2::new(0.13, 0.78)).length_squared() * 7.5).exp();
            let mint = (-(p - Vec2::new(0.93, 0.9)).length_squared() * 10.0).exp();
            let line = if (x + y / 3) % 96 < 2 { 0.085 } else { 0.0 };
            let color = Vec3::new(0.035, 0.065, 0.12)
                + Vec3::new(0.06, 0.34, 0.55) * blue
                + Vec3::new(0.65, 0.17, 0.13) * coral
                + Vec3::new(0.10, 0.32, 0.20) * mint
                + Vec3::splat(line);
            data.extend_from_slice(&[
                (color.x.min(1.0) * 255.0) as u8,
                (color.y.min(1.0) * 255.0) as u8,
                (color.z.min(1.0) * 255.0) as u8,
                255,
            ]);
        }
    }
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn accept_dropped_images(
    mut events: MessageReader<FileDragAndDrop>,
    mut pending: ResMut<PendingImport>,
    mut state: ResMut<PreviewSettings>,
) {
    for event in events.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            state.status = "Loading background...".into();
            start_image_import(Some(path_buf.clone()), &mut pending);
        }
    }
}
