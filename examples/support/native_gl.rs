use bevy::app::PluginGroupBuilder;

#[cfg(target_os = "linux")]
use bevy::{
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    winit::{DisplayHandleWrapper, WinitPlugin},
};

pub fn configure(plugins: PluginGroupBuilder) -> PluginGroupBuilder {
    #[cfg(target_os = "linux")]
    if std::env::var("WGPU_BACKEND").as_deref() == Ok("gl") {
        return plugins
            .disable::<RenderPlugin>()
            .add_after::<WinitPlugin>(WindowDisplayRenderPlugin);
    }
    plugins
}

#[cfg(target_os = "linux")]
struct WindowDisplayRenderPlugin;

#[cfg(target_os = "linux")]
impl Plugin for WindowDisplayRenderPlugin {
    fn build(&self, app: &mut App) {
        let mut settings = WgpuSettings::default();
        // Pass Winit's owned display after WinitPlugin has initialized it.
        // Bevy retains adapter selection and device/feature configuration.
        let display = app.world().resource::<DisplayHandleWrapper>().0.clone();
        settings.instance_display_handle = Some(std::sync::Arc::new(display));
        app.add_plugins(RenderPlugin {
            render_creation: settings.into(),
            ..default()
        });
    }
}
