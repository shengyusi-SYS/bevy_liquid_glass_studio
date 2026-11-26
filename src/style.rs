use bevy::prelude::*;

/// Geometry shared by independent surfaces and fused group members.
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Clone, Default, PartialEq)]
pub struct GlassShape {
    /// Corner radius in logical pixels, limited by the node's shorter half-side.
    pub corner_radius: f32,
    /// Superellipse exponent. `2.0` makes circular corners; larger values make a squircle.
    pub roundness: f32,
}

impl Default for GlassShape {
    fn default() -> Self {
        Self {
            corner_radius: 28.0,
            roundness: 5.0,
        }
    }
}

/// Controls whether lighting follows UI size or the original canvas model.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Clone, Default, PartialEq)]
pub enum GlassLighting {
    /// Lighting intensity and edge feathering stay stable across window sizes and DPI.
    #[default]
    Stable,
    /// Retains the original model's physical canvas-height dependence.
    OriginalCanvas,
}

/// Optical appearance and shadow for an independent surface or a fused group.
///
/// Geometric distances use logical UI pixels, independent of the target's scale
/// factor. Fresnel and glare ranges retain the original shader's range controls.
/// Colors use Bevy's normal [`Color`] conversion. A target's background blur is
/// configured separately so that many glass nodes can share one blurred image.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(Clone, Default, PartialEq)]
pub struct GlassStyle {
    /// Lighting and antialiasing calibration. Stable is suitable for reusable UI.
    pub lighting: GlassLighting,
    /// Depth of the refracting edge in logical pixels.
    pub refraction_thickness: f32,
    /// Dimensionless background-distance gain from the original model.
    /// `0.15` retains the original value; this is not a distance in pixels.
    pub refraction_distance: f32,
    /// Refractive index. Glass is approximately `1.4` to `1.5`.
    pub refractive_index: f32,
    /// Chromatic dispersion gain; zero keeps RGB samples together.
    pub dispersion: f32,
    /// Whether the refracting rim also uses the blurred background.
    pub blur_edge: bool,
    /// Fresnel range control from the original model, normally `0.0..=100.0`.
    /// Its square controls falloff; it is not a literal rim width in pixels.
    pub fresnel_range: f32,
    /// Hardness of the Fresnel rim, normally `0.0..=1.0`.
    pub fresnel_hardness: f32,
    /// Strength of the Fresnel rim, normally `0.0..=1.0`.
    pub fresnel_strength: f32,
    /// Rotation of the paired glare highlights, in degrees.
    pub glare_angle: f32,
    /// Glare range control from the original model, normally `0.0..=100.0`.
    /// Its square controls falloff; it is not a literal rim width in pixels.
    pub glare_range: f32,
    /// Hardness of the glare rim, normally `0.0..=1.0`.
    pub glare_hardness: f32,
    /// Glare intensity, normally `0.0..=1.0`.
    pub glare_strength: f32,
    /// Concentration of glare around its main direction, normally `0.0..=1.0`.
    pub glare_convergence: f32,
    /// Relative intensity of the highlight on the opposite side.
    pub glare_opposite_strength: f32,
    /// Tint color; alpha controls the amount of tint rather than node coverage.
    pub tint: Color,
    /// Native nodes use Bevy UI shadows; fused groups shade the union silhouette.
    pub shadow: GlassShadow,
}

impl Default for GlassStyle {
    fn default() -> Self {
        Self {
            lighting: GlassLighting::Stable,
            refraction_thickness: 20.0,
            refraction_distance: 0.05,
            refractive_index: 1.4,
            dispersion: 7.0,
            blur_edge: true,
            fresnel_range: 30.0,
            fresnel_hardness: 0.2,
            fresnel_strength: 0.2,
            glare_angle: -45.0,
            glare_range: 30.0,
            glare_hardness: 0.2,
            glare_strength: 0.9,
            glare_convergence: 0.5,
            glare_opposite_strength: 0.8,
            tint: Color::srgba(1.0, 1.0, 1.0, 0.0),
            shadow: GlassShadow::default(),
        }
    }
}

impl GlassStyle {
    /// The ordinary UI shadow corresponding to this style.
    pub fn box_shadow(&self) -> BoxShadow {
        BoxShadow::new(
            self.shadow.color,
            Val::Px(self.shadow.offset.x),
            Val::Px(self.shadow.offset.y),
            Val::Px(self.shadow.spread),
            Val::Px(self.shadow.blur),
        )
    }
}

/// Logical-pixel drop-shadow settings, shared with native Bevy UI rendering.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(Clone, Default, PartialEq)]
pub struct GlassShadow {
    pub color: Color,
    pub offset: Vec2,
    pub blur: f32,
    pub spread: f32,
}

impl Default for GlassShadow {
    fn default() -> Self {
        Self {
            color: Color::srgba(0.0, 0.0, 0.0, 0.16),
            offset: Vec2::new(0.0, 6.0),
            blur: 18.0,
            spread: 0.0,
        }
    }
}
