use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};
use bevy_inspector_egui::WorldInspectorPlugin;
use bevy_mod_raycast::{DefaultRaycastingPlugin, RaycastSource};
use iyes_loopless::prelude::*;

mod game;
mod game_state;
mod lobby;
mod models;
mod networking;
mod placement_phase;
mod server_selection;

use game_state::GameState;

//IP
//bsplus.floboja.net:30305

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            window: WindowDescriptor {
                title: "Battleship plus".to_string(),
                width: 1280.,
                height: 720.,
                mode: WindowMode::Windowed,
                resizable: false,
                decorations: true,
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            },
            ..default()
        }))
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(WorldInspectorPlugin::default())
        .add_plugin(DefaultRaycastingPlugin::<RaycastSet>::default())
        .add_loopless_state(GameState::Unconnected)
        .add_plugin(networking::NetworkingPlugin)
        .add_plugin(server_selection::ServerSelectionPlugin)
        .add_plugin(lobby::LobbyPlugin)
        .add_plugin(placement_phase::PlacementPhasePlugin)
        .add_plugin(game::GamePlugin)
        .add_startup_system(fps_counter)
        .add_startup_system(camera_setup)
        .insert_resource(lobby::UserName("Userus Namus XXVII.".to_string()))
        .add_system(text_update_system)
        .add_system(debug_state_change)
        .run();
}

#[derive(Component)]
struct FpsText;

struct RaycastSet;

fn camera_setup(mut commands: Commands) {
    commands
        .spawn(Camera3dBundle {
            projection: Projection::Orthographic(OrthographicProjection::default()),
            transform: Transform::from_translation(Vec3::new(0.0, 0.0, 100.0))
                .with_scale(Vec3::new(0.5, 0.5, 1.0))
                .looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        })
        .insert(RaycastSource::<RaycastSet>::default());
}

fn fps_counter(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        TextBundle::from_sections([
            TextSection::new(
                "FPS: ",
                TextStyle {
                    font: asset_server.load("fonts/LEMONMILK-Regular.otf"),
                    font_size: 20.0,
                    color: Color::WHITE,
                },
            ),
            TextSection::from_style(TextStyle {
                font: asset_server.load("fonts/LEMONMILK-Regular.otf"),
                font_size: 20.0,
                color: Color::GOLD,
            }),
        ]),
        FpsText,
    ));
}

fn text_update_system(diagnostics: Res<Diagnostics>, mut query: Query<&mut Text, With<FpsText>>) {
    for mut text in &mut query {
        if let Some(fps) = diagnostics.get(FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                // Update the value of the second section
                text.sections[1].value = format!("{value:.2}");
            }
        }
    }
}

fn debug_state_change(state: Res<CurrentState<GameState>>) {
    if state.is_changed() {
        println!("State changed to {state:?}");
    }
}
