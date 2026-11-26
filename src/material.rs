use bevy::{
    asset::{load_internal_asset, uuid_handle},
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType},
    shader::{Shader, ShaderRef},
};

use crate::style::{GlassLighting, GlassShadow, GlassShape, GlassStyle};

const GLASS_SHADER: Handle<Shader> = uuid_handle!("ba90f601-87b8-46cc-9cdf-b5e7a4465cef");

/// Texture samples and the material result use linear RGB. Inside the optical
/// model, sRGB working values preserve the original tint and highlight blends;
/// final target encoding follows the camera compositing space.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub(crate) struct GlassMaterial {
    #[uniform(0)]
    pub uniform: GlassUniform,
    #[texture(1)]
    #[sampler(2)]
    pub source_image: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub blurred_image: Handle<Image>,
}

impl UiMaterial for GlassMaterial {
    fn fragment_shader() -> ShaderRef {
        GLASS_SHADER.clone().into()
    }

    fn specialize(descriptor: &mut RenderPipelineDescriptor, _: UiMaterialKey<Self>) {
        descriptor.fragment.as_mut().unwrap().entry_point = Some("fragment".into());
    }
}

/// Fused groups use the same optics as individual nodes and add a union SDF.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub(crate) struct GlassGroupMaterial {
    #[uniform(0)]
    pub uniform: GlassUniform,
    #[texture(1)]
    #[sampler(2)]
    pub source_image: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub blurred_image: Handle<Image>,
    #[uniform(5)]
    pub group: GlassGroupUniform,
}

impl UiMaterial for GlassGroupMaterial {
    fn fragment_shader() -> ShaderRef {
        GLASS_SHADER.clone().into()
    }

    fn specialize(descriptor: &mut RenderPipelineDescriptor, _: UiMaterialKey<Self>) {
        descriptor.fragment.as_mut().unwrap().entry_point = Some("fragment_group".into());
    }
}

pub(crate) const MAX_GROUP_SHAPES: usize = 64;

/// One member's geometry in physical target pixels. Two vec4-sized slots keep
/// the fixed array compatible with uniform-buffer limits on native GL.
#[derive(ShaderType, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct GlassGroupShape {
    pub center: Vec2,
    pub half_size: Vec2,
    pub corner_radius: f32,
    pub roundness: f32,
    pub _padding: Vec2,
}

impl GlassGroupShape {
    pub fn new(center: Vec2, size: Vec2, corner_radius: f32, roundness: f32) -> Self {
        Self {
            center,
            half_size: size * 0.5,
            corner_radius,
            roundness,
            _padding: Vec2::ZERO,
        }
    }
}

#[derive(ShaderType, Debug, Clone, PartialEq)]
pub(crate) struct GlassGroupUniform {
    pub shape_count: u32,
    pub blend_radius: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_offset: Vec2,
    pub _padding: Vec2,
    pub shadow_color: Vec4,
    pub shapes: [GlassGroupShape; MAX_GROUP_SHAPES],
}

impl GlassGroupUniform {
    /// Shape data is already in physical pixels; the group setting and shadow
    /// are logical UI pixels and are converted together here.
    pub fn new(
        shapes: &[GlassGroupShape],
        blend_radius: f32,
        shadow: &GlassShadow,
        scale_factor: f32,
    ) -> Self {
        let count = shapes.len().min(MAX_GROUP_SHAPES);
        let mut packed = [GlassGroupShape::default(); MAX_GROUP_SHAPES];
        packed[..count].copy_from_slice(&shapes[..count]);
        let color = shadow.color.to_linear();
        Self {
            shape_count: count as u32,
            blend_radius: blend_radius * scale_factor,
            shadow_blur: shadow.blur * scale_factor,
            shadow_spread: shadow.spread * scale_factor,
            shadow_offset: shadow.offset * scale_factor,
            _padding: Vec2::ZERO,
            shadow_color: Vec4::new(color.red, color.green, color.blue, color.alpha),
            shapes: packed,
        }
    }
}

#[derive(ShaderType, Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlassUniform {
    pub target_size: Vec2,
    pub scale_factor: f32,
    pub corner_radius: f32,
    pub roundness: f32,
    pub refraction_thickness: f32,
    pub refraction_distance: f32,
    pub refractive_index: f32,
    pub dispersion: f32,
    pub blur_edge: u32,
    pub fresnel_range: f32,
    pub fresnel_hardness: f32,
    pub fresnel_strength: f32,
    pub glare_angle: f32,
    pub glare_range: f32,
    pub glare_hardness: f32,
    pub glare_strength: f32,
    pub glare_convergence: f32,
    pub glare_opposite_strength: f32,
    pub lighting_gain: f32,
    pub edge_feather: f32,
    pub tint: Vec4,
}

impl GlassUniform {
    pub fn new(style: &GlassStyle, target_size: Vec2, scale_factor: f32) -> Self {
        let tint = style.tint.to_linear();
        let (lighting_gain, edge_feather) = match style.lighting {
            GlassLighting::Stable => (1414.213562 / 900.0, 0.9 * scale_factor),
            GlassLighting::OriginalCanvas => {
                let height = target_size.y.max(1.0);
                (1414.213562 / height, 0.001 * height)
            }
        };
        Self {
            target_size,
            scale_factor,
            // Only the individual entry point reads these fields. Groups carry
            // their geometry in GlassGroupUniform instead.
            corner_radius: 0.0,
            roundness: 0.0,
            refraction_thickness: style.refraction_thickness,
            refraction_distance: style.refraction_distance,
            refractive_index: style.refractive_index,
            dispersion: style.dispersion,
            blur_edge: u32::from(style.blur_edge),
            fresnel_range: style.fresnel_range,
            fresnel_hardness: style.fresnel_hardness,
            fresnel_strength: style.fresnel_strength,
            glare_angle: style.glare_angle.to_radians(),
            glare_range: style.glare_range,
            glare_hardness: style.glare_hardness,
            glare_strength: style.glare_strength,
            glare_convergence: style.glare_convergence,
            glare_opposite_strength: style.glare_opposite_strength,
            lighting_gain,
            edge_feather,
            tint: Vec4::new(tint.red, tint.green, tint.blue, tint.alpha),
        }
    }

    pub fn with_shape(mut self, shape: GlassShape) -> Self {
        self.corner_radius = shape.corner_radius;
        self.roundness = shape.roundness;
        self
    }
}

pub(crate) fn register_shaders(app: &mut App) {
    load_internal_asset!(
        app,
        GLASS_SHADER,
        "../assets/shaders/glass.wesl",
        |source, _| Shader::from_wesl(source, "embedded://bevy_liquid_glass/glass.wesl")
    );
}
