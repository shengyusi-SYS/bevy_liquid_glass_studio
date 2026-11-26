//! The preview uses official Feathers widgets; no widget framework lives in the library.
use super::{
    EffectivePreview, FollowGlass, FollowText, FusionText, INK, MUTED, PendingImport,
    PreviewSettings, StatusText, apply_reference_preset, start_image_import,
};
use bevy::{
    clipboard::Clipboard,
    feathers::{
        controls::{ButtonVariant, FeathersButton, FeathersSlider},
        display::caption,
    },
    prelude::*,
    ui_widgets::{Activate, ScrollArea, SliderPrecision, SliderStep, SliderValue, ValueChange},
};

use bevy_liquid_glass::{GlassBlurQuality, GlassLighting};

#[derive(Component, Clone, Copy, Debug)]
pub(super) enum Param {
    Width,
    Height,
    Radius,
    Roundness,
    MergeRadius,
    Blur,
    Thickness,
    Refraction,
    Index,
    Dispersion,
    Fresnel,
    FresnelRange,
    Glare,
    GlareAngle,
    Tint,
    Shadow,
    ShadowBlur,
    FresnelHardness,
    GlareRange,
    GlareHardness,
    GlareConvergence,
    GlareOpposite,
    ShadowX,
    ShadowY,
    ShadowSpread,
}

const PARAMETER_GROUPS: &[(&str, &[(Param, &str, f32, f32)])] = &[
    (
        "SHAPE",
        &[
            (Param::Width, "Width", 160.0, 1100.0),
            (Param::Height, "Height", 80.0, 300.0),
            (Param::Radius, "Corner radius", 0.0, 100.0),
            (Param::Roundness, "Superellipse", 2.0, 8.0),
            (Param::MergeRadius, "Merge radius", 0.0, 120.0),
        ],
    ),
    (
        "OPTICS",
        &[
            (Param::Blur, "Backdrop blur", 0.0, 40.0),
            (Param::Thickness, "Edge thickness", 1.0, 64.0),
            (Param::Refraction, "Refraction distance", 0.0, 0.2),
            (Param::Index, "Refractive index", 1.0, 2.5),
            (Param::Dispersion, "Dispersion", 0.0, 30.0),
        ],
    ),
    (
        "LIGHT & COLOR",
        &[
            (Param::Fresnel, "Fresnel", 0.0, 1.0),
            (Param::FresnelRange, "Fresnel range", 1.0, 80.0),
            (Param::Glare, "Glare", 0.0, 1.0),
            (Param::GlareAngle, "Light angle", -180.0, 180.0),
            (Param::Tint, "Tint opacity", 0.0, 0.65),
            (Param::Shadow, "Shadow opacity", 0.0, 0.5),
            (Param::ShadowBlur, "Shadow softness", 0.0, 48.0),
        ],
    ),
];

const ADVANCED_PARAMETERS: &[(Param, &str, f32, f32)] = &[
    (Param::FresnelHardness, "Fresnel hardness", 0.0, 1.0),
    (Param::GlareRange, "Glare range", 0.0, 100.0),
    (Param::GlareHardness, "Glare hardness", 0.0, 1.0),
    (Param::GlareConvergence, "Glare convergence", 0.0, 1.0),
    (Param::GlareOpposite, "Opposite highlight", 0.0, 1.0),
    (Param::ShadowX, "Shadow offset X", -40.0, 40.0),
    (Param::ShadowY, "Shadow offset Y", -40.0, 40.0),
    (Param::ShadowSpread, "Shadow spread", -20.0, 40.0),
];

#[derive(Component)]
pub(super) struct AdvancedControls;

#[derive(Component, Clone, Default)]
pub(super) struct ControlLabel(ControlKind);

#[derive(Clone, Copy, Default)]
enum ControlKind {
    #[default]
    Feedback,
    Advanced,
    BlurEdges,
    BlurQuality,
    Lighting,
    Effective,
}

impl Param {
    fn value(self, state: &PreviewSettings) -> f32 {
        let s = &state.style;
        match self {
            Self::Width => state.width,
            Self::Height => state.height,
            Self::Radius => state.shape.corner_radius,
            Self::Roundness => state.shape.roundness,
            Self::MergeRadius => state.blend_radius,
            Self::Blur => state.blur_radius,
            Self::Thickness => s.refraction_thickness,
            Self::Refraction => s.refraction_distance,
            Self::Index => s.refractive_index,
            Self::Dispersion => s.dispersion,
            Self::Fresnel => s.fresnel_strength,
            Self::FresnelRange => s.fresnel_range,
            Self::Glare => s.glare_strength,
            Self::GlareAngle => s.glare_angle,
            Self::Tint => s.tint.alpha(),
            Self::Shadow => s.shadow.color.alpha(),
            Self::ShadowBlur => s.shadow.blur,
            Self::FresnelHardness => s.fresnel_hardness,
            Self::GlareRange => s.glare_range,
            Self::GlareHardness => s.glare_hardness,
            Self::GlareConvergence => s.glare_convergence,
            Self::GlareOpposite => s.glare_opposite_strength,
            Self::ShadowX => s.shadow.offset.x,
            Self::ShadowY => s.shadow.offset.y,
            Self::ShadowSpread => s.shadow.spread,
        }
    }

    fn apply(self, value: f32, state: &mut PreviewSettings) {
        let s = &mut state.style;
        match self {
            Self::Width => state.width = value,
            Self::Height => state.height = value,
            Self::Radius => state.shape.corner_radius = value,
            Self::Roundness => state.shape.roundness = value,
            Self::MergeRadius => state.blend_radius = value,
            Self::Blur => state.blur_radius = value,
            Self::Thickness => s.refraction_thickness = value,
            Self::Refraction => s.refraction_distance = value,
            Self::Index => s.refractive_index = value,
            Self::Dispersion => s.dispersion = value,
            Self::Fresnel => s.fresnel_strength = value,
            Self::FresnelRange => s.fresnel_range = value,
            Self::Glare => s.glare_strength = value,
            Self::GlareAngle => s.glare_angle = value,
            Self::Tint => s.tint = s.tint.with_alpha(value),
            Self::Shadow => s.shadow.color = s.shadow.color.with_alpha(value),
            Self::ShadowBlur => s.shadow.blur = value,
            Self::FresnelHardness => s.fresnel_hardness = value,
            Self::GlareRange => s.glare_range = value,
            Self::GlareHardness => s.glare_hardness = value,
            Self::GlareConvergence => s.glare_convergence = value,
            Self::GlareOpposite => s.glare_opposite_strength = value,
            Self::ShadowX => s.shadow.offset.x = value,
            Self::ShadowY => s.shadow.offset.y = value,
            Self::ShadowSpread => s.shadow.spread = value,
        }
    }
}

pub(super) fn spawn_panel(commands: &mut Commands, panel: Entity) {
    text(commands, panel, "Material lab", 27.0, INK);
    text(
        commands,
        panel,
        "BASE PARAMETERS | SHARED BACKDROP",
        10.0,
        MUTED,
    );
    commands.spawn_scene(bsn! {
        @FeathersButton { @variant: ButtonVariant::Primary }
        ChildOf(panel) Name::new("ImportImageButton")
        Node { min_height: px(36), flex_shrink: 0.0 }
        on(import_image)
        Children [ @caption("Import background image") ]
    });
    commands.spawn((
        ChildOf(panel),
        Name::new("ImageImportStatus"),
        StatusText,
        Text::new("Built-in background | import your own PNG, JPEG or WebP"),
        TextFont {
            font_size: FontSize::Px(11.0),
            ..default()
        },
        TextColor(MUTED),
        Node {
            min_height: px(30),
            flex_shrink: 0.0,
            ..default()
        },
    ));
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(panel) Name::new("FollowToggle")
        Node { min_height: px(32), flex_shrink: 0.0 }
        on(toggle_follow)
        Children [ @caption("Follow pointer: on") FollowText ]
    });
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(panel) Name::new("FusionToggle")
        Node { min_height: px(32), flex_shrink: 0.0 }
        on(toggle_fusion)
        Children [ @caption("Smooth merge: on") FusionText ]
    });
    let actions = commands
        .spawn((
            ChildOf(panel),
            Node {
                column_gap: px(8),
                min_height: px(30),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(actions) Name::new("ResetButton")
        Node { flex_grow: 1.0 }
        on(reset)
        Children [ @caption("Reset defaults") ]
    });
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(actions) Name::new("TintPreset")
        Node { flex_grow: 1.0 }
        on(cycle_tint)
        Children [ @caption("Tint color") ]
    });
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(panel) Name::new("ReferencePreset")
        Node { min_height: px(30), flex_shrink: 0.0 }
        on(reference_preset)
        Children [ @caption("Reference web preset") ]
    });
    let tools = commands
        .spawn((
            ChildOf(panel),
            Node {
                column_gap: px(8),
                min_height: px(32),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(tools) Name::new("FeedbackToggle")
        Node { flex_grow: 1.0, flex_basis: px(0) }
        on(toggle_feedback)
        Children [ @caption("Feedback: off") ControlLabel(ControlKind::Feedback) ]
    });
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(tools) Name::new("CopyRustConfig")
        Node { flex_grow: 1.0, flex_basis: px(0) }
        on(copy_rust_config)
        Children [ @caption("Copy Rust config") ]
    });
    commands.spawn((
        ChildOf(panel),
        Name::new("EffectiveValues"),
        ControlLabel(ControlKind::Effective),
        Text::new("Effective material"),
        TextFont {
            font_size: FontSize::Px(10.0),
            ..default()
        },
        TextColor(MUTED),
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let scroll = commands
        .spawn((
            Name::new("ParameterScrollArea"),
            ChildOf(panel),
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(11),
                width: percent(100),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                flex_basis: px(0),
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                padding: UiRect::right(px(7)),
                ..default()
            },
        ))
        .id();
    let defaults = PreviewSettings::default();
    for &(title, parameters) in PARAMETER_GROUPS {
        group(commands, scroll, title);
        for &(param, label, min, max) in parameters {
            parameter(commands, scroll, &defaults, param, label, min, max);
            if matches!(param, Param::Blur) {
                commands.spawn_scene(bsn! {
                    @FeathersButton
                    ChildOf(scroll) Name::new("BlurQualityCycle")
                    Node { min_height: px(30), flex_shrink: 0.0 }
                    on(cycle_blur_quality)
                    Children [ @caption("Blur quality: Auto") ControlLabel(ControlKind::BlurQuality) ]
                });
            }
        }
    }
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(scroll) Name::new("AdvancedToggle")
        Node { min_height: px(30), flex_shrink: 0.0 }
        on(toggle_advanced)
        Children [ @caption("Advanced controls: closed") ControlLabel(ControlKind::Advanced) ]
    });
    let advanced = commands
        .spawn((
            ChildOf(scroll),
            Name::new("AdvancedControls"),
            AdvancedControls,
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(11),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(advanced) Name::new("BlurEdgeToggle")
        Node { min_height: px(30), flex_shrink: 0.0 }
        on(toggle_blur_edges)
        Children [ @caption("Blur refracting edges: on") ControlLabel(ControlKind::BlurEdges) ]
    });
    commands.spawn_scene(bsn! {
        @FeathersButton
        ChildOf(advanced) Name::new("LightingToggle")
        Node { min_height: px(30), flex_shrink: 0.0 }
        on(toggle_lighting)
        Children [ @caption("Lighting: stable") ControlLabel(ControlKind::Lighting) ]
    });
    for &(param, label, min, max) in ADVANCED_PARAMETERS {
        parameter(commands, advanced, &defaults, param, label, min, max);
    }
    text(
        commands,
        panel,
        "Scroll for more | drag a slider or use arrow keys",
        10.0,
        MUTED,
    );
}

fn parameter(
    commands: &mut Commands,
    parent: Entity,
    state: &PreviewSettings,
    param: Param,
    label: &'static str,
    min: f32,
    max: f32,
) {
    let row = commands
        .spawn((
            ChildOf(parent),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    text(commands, row, label, 12.0, INK.with_alpha(0.8));
    let value = param.value(state);
    let precision = if matches!(param, Param::Refraction) {
        3
    } else {
        2
    };
    let slider = commands
        .spawn_scene(bsn! {
            @FeathersSlider { @min: min, @max: max }
            ChildOf(row) Name::new(format!("Param::{param:?}"))
            param SliderValue(value) SliderPrecision(precision)
            Node { min_height: px(28), flex_shrink: 0.0 }
            on(slider_changed)
        })
        .id();
    if matches!(param, Param::Refraction) {
        commands.entity(slider).insert(SliderStep(0.001));
    }
}

fn group(commands: &mut Commands, parent: Entity, title: &'static str) {
    commands.spawn((
        ChildOf(parent),
        Text::new(title),
        TextFont {
            font_size: FontSize::Px(10.0),
            ..default()
        },
        TextColor(Color::srgb(0.46, 0.75, 0.9)),
        Node {
            margin: UiRect::top(px(12)),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}

fn text(commands: &mut Commands, parent: Entity, label: &'static str, size: f32, color: Color) {
    commands.spawn((
        ChildOf(parent),
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    ));
}

fn slider_changed(
    change: On<ValueChange<f32>>,
    params: Query<&Param>,
    mut state: ResMut<PreviewSettings>,
) {
    if let Ok(param) = params.get(change.source) {
        param.apply(change.value, &mut state);
    }
}

pub(super) fn sync_sliders(
    state: Res<PreviewSettings>,
    effective: Res<EffectivePreview>,
    sliders: Query<(Entity, &Param, &SliderValue)>,
    mut advanced: Query<&mut Node, With<AdvancedControls>>,
    mut labels: Query<(&ControlLabel, &mut Text)>,
    mut commands: Commands,
) {
    if state.is_changed() {
        for (entity, param, value) in &sliders {
            let next = param.value(&state);
            if value.0 != next {
                commands.entity(entity).insert(SliderValue(next));
            }
        }
        for mut node in &mut advanced {
            let display = if state.advanced {
                Display::Flex
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
            }
        }
    }
    if state.is_changed() || effective.is_changed() {
        for (label, mut text) in &mut labels {
            let value = match label.0 {
                ControlKind::Feedback => format!("Feedback: {}", on_off(state.feedback)),
                ControlKind::Advanced => format!(
                    "Advanced controls: {}",
                    if state.advanced { "open" } else { "closed" }
                ),
                ControlKind::BlurEdges => {
                    format!("Blur refracting edges: {}", on_off(state.style.blur_edge))
                }
                ControlKind::BlurQuality => format!(
                    "Blur quality: {}",
                    match state.blur_quality {
                        GlassBlurQuality::Auto => "Auto",
                        GlassBlurQuality::HalfResolution => "Half",
                        GlassBlurQuality::FullResolution => "Full",
                    }
                ),
                ControlKind::Lighting => {
                    format!("Lighting: {}", lighting_name(state.style.lighting))
                }
                ControlKind::Effective => format!(
                    "EFFECTIVE | {} | feedback {}\nRadius {:.1}px | distance {:.3} | index {:.2}\nFresnel {:.2} | glare {:.2} | tint {:.2}\nBlur {:.1}px | merge {:.1}px | hover {} / press {}",
                    lighting_name(effective.style.lighting),
                    on_off(state.feedback),
                    effective.shape.corner_radius,
                    effective.style.refraction_distance,
                    effective.style.refractive_index,
                    effective.style.fresnel_strength,
                    effective.style.glare_strength,
                    effective.style.tint.alpha(),
                    state.blur_radius,
                    effective.blend_radius,
                    on_off(effective.hovered),
                    on_off(effective.pressed),
                ),
            };
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn lighting_name(value: GlassLighting) -> &'static str {
    match value {
        GlassLighting::Stable => "stable",
        GlassLighting::OriginalCanvas => "original canvas",
    }
}

fn import_image(
    _: On<Activate>,
    mut pending: ResMut<PendingImport>,
    mut state: ResMut<PreviewSettings>,
) {
    if pending.0.is_none() {
        state.status = "Choose a background image...".into();
        start_image_import(None, &mut pending);
    }
}

fn toggle_follow(
    _: On<Activate>,
    mut state: ResMut<PreviewSettings>,
    mut node: Single<&mut Node, With<FollowGlass>>,
) {
    state.follow = !state.follow;
    if !state.follow {
        node.left = percent(53);
        node.top = percent(52);
    }
}

fn reset(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    let status = state.status.clone();
    *state = PreviewSettings {
        status,
        ..default()
    };
}

fn reference_preset(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    apply_reference_preset(&mut state);
}

fn toggle_fusion(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    state.fusion_enabled = !state.fusion_enabled;
}

fn cycle_tint(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    const TINTS: [Color; 4] = [
        Color::srgb(0.92, 0.96, 1.0),
        Color::srgb(1.0, 0.61, 0.72),
        Color::srgb(0.52, 1.0, 0.79),
        Color::srgb(1.0, 0.8, 0.43),
    ];
    state.tint_preset = (state.tint_preset + 1) % TINTS.len();
    let alpha = if state.style.tint.alpha() == 0.0 {
        0.15
    } else {
        state.style.tint.alpha()
    };
    state.style.tint = TINTS[state.tint_preset].with_alpha(alpha);
}

fn toggle_feedback(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    state.feedback = !state.feedback;
}

fn toggle_advanced(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    state.advanced = !state.advanced;
}

fn toggle_blur_edges(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    state.style.blur_edge = !state.style.blur_edge;
}

fn cycle_blur_quality(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    state.blur_quality = match state.blur_quality {
        GlassBlurQuality::Auto => GlassBlurQuality::HalfResolution,
        GlassBlurQuality::HalfResolution => GlassBlurQuality::FullResolution,
        GlassBlurQuality::FullResolution => GlassBlurQuality::Auto,
    };
}

fn toggle_lighting(_: On<Activate>, mut state: ResMut<PreviewSettings>) {
    state.style.lighting = match state.style.lighting {
        GlassLighting::Stable => GlassLighting::OriginalCanvas,
        GlassLighting::OriginalCanvas => GlassLighting::Stable,
    };
}

fn copy_rust_config(
    _: On<Activate>,
    mut state: ResMut<PreviewSettings>,
    effective: Res<EffectivePreview>,
    mut clipboard: ResMut<Clipboard>,
) {
    let code = super::preview_config::rust_config(&state, &effective);
    state.status = match clipboard.set_text(code) {
        Ok(()) => "Effective Rust configuration copied to clipboard".into(),
        Err(error) => format!("Could not copy Rust configuration: {error}"),
    };
}
