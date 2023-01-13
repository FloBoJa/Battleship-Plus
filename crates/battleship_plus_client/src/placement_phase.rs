use bevy::prelude::*;
use bevy_mod_picking::{DebugCursorPickingPlugin, DebugEventsPickingPlugin, PickableBundle};
use iyes_loopless::prelude::*;
use rstar::AABB;

use battleship_plus_common::{types, util};

use crate::game_state::GameState;

pub struct PlacementPhasePlugin;

impl Plugin for PlacementPhasePlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(load_assets)
            .add_enter_system(GameState::PlacementPhase, spawn_components);
    }
}

#[derive(Resource, Deref)]
pub struct Quadrant(AABB<[i32; 2]>);

impl Quadrant {
    pub fn new(corner: types::Coordinate, board_size: u32, player_count: u32) -> Quadrant {
        let corner = (corner.x, corner.y);
        Quadrant(util::quadrant_from_corner(corner, board_size, player_count))
    }

    fn coordinate_iter(&self) -> impl Iterator<Item = (i32, i32)> {
        let size_x = self.upper()[0] - self.lower()[0];
        let size_y = self.upper()[1] - self.lower()[1];
        (0..size_x).flat_map(move |x| (0..size_y).map(move |y| (x, y)))
    }
}

#[derive(Resource)]
struct GameAssets {
    ocean_scene: Handle<Scene>,
}

#[derive(Bundle, Default)]
struct TileBundle {
    tile: Tile,
    model: PbrBundle,
    pickable: PickableBundle,
}

#[derive(Component, Default)]
struct Tile {
    coordinate: (i32, i32),
}

impl TileBundle {
    fn new(
        coordinate: (i32, i32),
        translation: Vec3,
        mesh: Handle<Mesh>,
        material: Handle<StandardMaterial>,
    ) -> TileBundle {
        TileBundle {
            tile: Tile { coordinate },
            model: PbrBundle {
                mesh,
                material,
                transform: Transform::from_translation(translation),
                ..default()
            },
            ..default()
        }
    }
}

fn load_assets(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(GameAssets {
        ocean_scene: assets.load("models/ocean.glb#Scene0"),
    });
}

fn spawn_components(
    mut commands: Commands,
    assets: Res<GameAssets>,
    quadrant: Res<Quadrant>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands
        .spawn(SceneBundle {
            scene: assets.ocean_scene.clone(),
            ..default()
        })
        .insert(Name::new("Ocean"))
        .insert(PickableBundle::default());
    commands
        .spawn(DirectionalLightBundle {
            transform: Transform::from_rotation(Quat::from_axis_angle(
                Vec3::new(1.0, -1.0, 0.0),
                0.2,
            )),
            directional_light: DirectionalLight {
                illuminance: 10000.0,
                ..default()
            },
            ..default()
        })
        .insert(Name::new("Directional Light"));

    const OCEAN_SIZE: f32 = 320.0;
    const OFFSET_X: f32 = -OCEAN_SIZE / 2.0;
    const OFFSET_Y: f32 = -OCEAN_SIZE / 2.0;
    const OFFSET_Z: f32 = 50.0;

    let quadrant_size = quadrant.upper()[0] - quadrant.lower()[0];
    let quadrant_size = quadrant_size as f32;
    let tile_size = OCEAN_SIZE / quadrant_size;
    let tile_mesh = meshes.add(Mesh::from(shape::Cube { size: tile_size }));
    let tile_material = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        base_color: Color::rgba(1.0, 1.0, 1.0, 0.2),
        ..default()
    });

    commands
        .spawn(SpatialBundle::default())
        .insert(Name::new("Grid"))
        .with_children(|child_builder| {
            quadrant.coordinate_iter().for_each(|coordinate| {
                child_builder
                    .spawn(TileBundle::new(
                        coordinate,
                        Vec3::new(
                            coordinate.0 as f32 * tile_size + OFFSET_X,
                            coordinate.1 as f32 * tile_size + OFFSET_Y,
                            OFFSET_Z,
                        ),
                        tile_mesh.clone(),
                        tile_material.clone(),
                    ))
                    .insert(Name::new(format!("{coordinate:?}")));
            });
        });
}
