//! Converts the visible material to ordinary Rust configuration, without an
//! example-specific runtime or a serialized preset format.

use bevy::prelude::*;
use bevy_liquid_glass::GlassLighting;

use super::{EffectivePreview, PreviewSettings};

pub(super) fn rust_config(state: &PreviewSettings, effective: &EffectivePreview) -> String {
    let shape = &effective.shape;
    let style = &effective.style;
    let lighting = match style.lighting {
        GlassLighting::Stable => "Stable",
        GlassLighting::OriginalCanvas => "OriginalCanvas",
    };
    format!(
        r#"// Liquid glass configuration copied from the native preview.
// Base shape: radius {base_radius}px, roundness {base_roundness}.
// Base optics: distance {base_distance}, Fresnel {base_fresnel}, glare {base_glare}.
// Feedback enabled: {feedback}; hovered: {hovered}; pressed: {pressed}.
// The values below freeze the effective result; no preview hover system is required.
// Shape distances are logical UI pixels, including the current preview fitting scale.
// Attach the backdrop to your existing UI camera and supply your own UI layout/background.
// Choose standalone GlassNode OR a GlassGroup containing GlassMember nodes.

use bevy::prelude::*;
use bevy_liquid_glass::{{
    GlassBackdrop, GlassBackdropRefresh, GlassBlurQuality, GlassGroup, GlassLighting, GlassMember, GlassNode,
    GlassShadow, GlassShape, GlassStyle,
}};

pub fn glass_config() -> (GlassNode, GlassBackdrop, GlassGroup, GlassMember) {{
    let shape = GlassShape {{
        corner_radius: {corner_radius},
        roundness: {roundness},
    }};
    let style = GlassStyle {{
        refraction_thickness: {thickness},
        refraction_distance: {distance},
        refractive_index: {index},
        dispersion: {dispersion},
        blur_edge: {blur_edge},
        fresnel_range: {fresnel_range},
        fresnel_hardness: {fresnel_hardness},
        fresnel_strength: {fresnel_strength},
        glare_angle: {glare_angle},
        glare_range: {glare_range},
        glare_hardness: {glare_hardness},
        glare_strength: {glare_strength},
        glare_convergence: {glare_convergence},
        glare_opposite_strength: {glare_opposite},
        tint: {tint},
        lighting: GlassLighting::{lighting},
        shadow: GlassShadow {{
            color: {shadow_color},
            offset: Vec2::new({shadow_x}, {shadow_y}),
            blur: {shadow_blur},
            spread: {shadow_spread},
        }},
    }};
    let backdrop = GlassBackdrop {{
        blur_radius: {blur}, source: None,
        blur_quality: GlassBlurQuality::{blur_quality},
        refresh: GlassBackdropRefresh::{refresh},
    }};
    let node = GlassNode {{ shape, style: style.clone() }};
    let group = GlassGroup {{ style, blend_radius: {blend} }};
    let member = GlassMember {{ shape }};
    (node, backdrop, group, member)
}}
"#,
        base_radius = scalar(state.shape.corner_radius),
        base_roundness = scalar(state.shape.roundness),
        base_distance = scalar(state.style.refraction_distance),
        base_fresnel = scalar(state.style.fresnel_strength),
        base_glare = scalar(state.style.glare_strength),
        feedback = state.feedback,
        blur_quality = format!("{:?}", state.blur_quality),
        refresh = format!("{:?}", state.backdrop_refresh),
        hovered = effective.hovered,
        pressed = effective.pressed,
        corner_radius = scalar(shape.corner_radius),
        roundness = scalar(shape.roundness),
        thickness = scalar(style.refraction_thickness),
        distance = scalar(style.refraction_distance),
        index = scalar(style.refractive_index),
        dispersion = scalar(style.dispersion),
        blur_edge = style.blur_edge,
        fresnel_range = scalar(style.fresnel_range),
        fresnel_hardness = scalar(style.fresnel_hardness),
        fresnel_strength = scalar(style.fresnel_strength),
        glare_angle = scalar(style.glare_angle),
        glare_range = scalar(style.glare_range),
        glare_hardness = scalar(style.glare_hardness),
        glare_strength = scalar(style.glare_strength),
        glare_convergence = scalar(style.glare_convergence),
        glare_opposite = scalar(style.glare_opposite_strength),
        tint = color(style.tint),
        shadow_color = color(style.shadow.color),
        shadow_x = scalar(style.shadow.offset.x),
        shadow_y = scalar(style.shadow.offset.y),
        shadow_blur = scalar(style.shadow.blur),
        shadow_spread = scalar(style.shadow.spread),
        blur = scalar(state.blur_radius),
        blend = scalar(effective.blend_radius),
    )
}

fn scalar(value: f32) -> String {
    let text = value.to_string();
    if text.contains(['.', 'e', 'E']) {
        text
    } else {
        format!("{text}.0")
    }
}

fn color(value: Color) -> String {
    let value = value.to_srgba();
    format!(
        "Color::srgba({}, {}, {}, {})",
        scalar(value.red),
        scalar(value.green),
        scalar(value.blue),
        scalar(value.alpha),
    )
}
