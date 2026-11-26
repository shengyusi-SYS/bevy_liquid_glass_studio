//! A standalone glass node and a fused group, using ordinary Bevy UI.
//! Run `./examples/preview.sh minimal`.

use bevy::prelude::*;
use bevy_liquid_glass::{GlassBackdrop, GlassGroup, GlassMember, GlassNode, LiquidGlassPlugin};

#[path = "support/native_gl.rs"]
mod native_gl;

fn main() {
    let plugins = native_gl::configure(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Minimal liquid glass".into(),
            resolution: (800, 560).into(),
            ..default()
        }),
        ..default()
    }));
    App::new()
        .add_plugins((plugins, LiquidGlassPlugin, bevy_framepace::FramepacePlugin))
        .insert_resource(bevy_framepace::FramepaceSettings {
            limiter: bevy_framepace::Limiter::from_framerate(30.0),
        })
        .insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_millis(100),
            ),
        })
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    info!("Minimal preview: frame cap 30 FPS; inactive updates 100 ms");
    commands.spawn((Camera2d, GlassBackdrop::default()));
    let tile = Node {
        width: px(168),
        height: px(124),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    };
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(48)),
                flex_direction: FlexDirection::Column,
                row_gap: px(36),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.15, 0.25)),
        ))
        .with_children(|root| {
            // Background content is drawn before the first glass surface.
            for i in 0..14 {
                root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(i as f32 * 64.0),
                        top: px(0),
                        width: px(20),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.1, 0.75, 0.9, 0.4)),
                ));
            }
            root.spawn(label("Single glass / fused group"));
            root.spawn((
                Node {
                    width: px(350),
                    ..tile.clone()
                },
                GlassNode::default(),
            ))
            .with_child(label("GlassNode"));
            root.spawn((
                Node {
                    column_gap: px(18),
                    ..default()
                },
                GlassGroup::default(),
            ))
            .with_children(|group| {
                for title in ["Member A", "Member B"] {
                    group
                        .spawn((tile.clone(), GlassMember::default()))
                        .with_child(label(title));
                }
            });
        });
}

fn label(text: &str) -> impl Bundle {
    (Text::new(text), TextFont::from_font_size(22.0))
}
