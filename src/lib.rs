//! Liquid glass nodes for Bevy UI.
//!
//! Add [`LiquidGlassPlugin`], put [`GlassBackdrop`] on a UI camera, then add
//! [`GlassNode`] to ordinary UI nodes for independent surfaces. To fuse surfaces,
//! put [`GlassMember`] descendants inside a [`GlassGroup`] container. The live
//! backdrop is captured once and shared by all glass on that camera.

mod group;
mod material;
mod material_cache;
mod render;
mod style;
mod ui;

pub use group::{GlassGroup, GlassMember, MAX_GLASS_GROUP_NODES};
pub use style::{GlassLighting, GlassShadow, GlassShape, GlassStyle};

use bevy::prelude::*;

/// An independent glass surface with its own geometry and optical style.
///
/// It participates in native UI layout and input, and stays independent even
/// inside a [`GlassGroup`]. Use [`GlassMember`] to participate in group fusion.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default, Clone)]
#[require(Node)]
pub struct GlassNode {
    pub shape: GlassShape,
    pub style: GlassStyle,
}

/// Shared background processing for the UI rendered by this camera.
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default, Clone)]
pub struct GlassBackdrop {
    /// Gaussian radius in logical pixels. All glass nodes on this camera share it.
    pub blur_radius: f32,
    /// Optional full-target image. `None` captures the live scene and preceding UI.
    pub source: Option<Handle<Image>>,
    /// Processing resolution for the shared blur. Raw refraction stays full size.
    pub blur_quality: GlassBlurQuality,
    /// Controls when the shared background is captured and blurred again.
    pub refresh: GlassBackdropRefresh,
}

/// Refresh policy for captured UI backgrounds and external images.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Default, Clone, PartialEq)]
pub enum GlassBackdropRefresh {
    /// Process the background on every rendered frame.
    #[default]
    EveryFrame,
    /// Reuse the result until configuration, image assets or target metadata change.
    /// After changing captured scene/UI or writing an image on the GPU, call
    /// `set_changed()` on the camera's `Mut<GlassBackdrop>` to request a refresh.
    OnDemand,
}

/// Work counters for the active library instance, available without GPU profiling.
#[derive(Resource, Reflect, Clone, Debug, Default)]
#[reflect(Resource)]
pub struct GlassStatistics {
    /// Enable last-frame CPU system timings. Disabled for normal consumers.
    pub collect_timings: bool,
    pub node_sync_us: f64,
    pub group_sync_us: f64,
    pub membership_sync_us: f64,
    pub node_updates: u64,
    pub group_updates: u64,
    pub membership_rebuilds: u64,
    /// Live shared materials used by independent nodes (group surfaces are separate).
    pub shared_materials: usize,
    /// Completed capture/blur recordings across currently configured cameras.
    pub backdrop_updates: u64,
}

/// Resolution policy for shared Gaussian processing. Raw refraction stays full size.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassBlurQuality {
    /// Full size below 8 physical pixels of blur radius; half size at 8 or above.
    #[default]
    Auto,
    /// Preserve the original full-resolution convolution for visual comparisons.
    FullResolution,
    /// Half width and height for any positive physical blur radius.
    /// This is an approximation; small details and moving edges can differ.
    HalfResolution,
}

impl Default for GlassBackdrop {
    fn default() -> Self {
        Self {
            blur_radius: 12.0,
            source: None,
            blur_quality: GlassBlurQuality::Auto,
            refresh: GlassBackdropRefresh::EveryFrame,
        }
    }
}

/// Installs glass styles, node synchronization and live backdrop rendering.
pub struct LiquidGlassPlugin;

impl Plugin for LiquidGlassPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<GlassGroup>()
            .register_type::<GlassMember>()
            .register_type::<GlassNode>()
            .register_type::<GlassShape>()
            .register_type::<GlassStyle>()
            .register_type::<GlassLighting>()
            .register_type::<GlassShadow>()
            .register_type::<GlassBackdrop>()
            .register_type::<GlassBlurQuality>()
            .register_type::<GlassBackdropRefresh>()
            .register_type::<GlassStatistics>()
            .init_resource::<GlassStatistics>()
            .add_plugins(ui::GlassUiPlugin)
            .add_plugins(group::GlassGroupsPlugin)
            .add_plugins(render::GlassRenderPlugin);
    }
}
