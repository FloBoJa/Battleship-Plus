use bevy::prelude::*;
use bevy_mod_picking::{PickableBundle, PickingEvent};
use iyes_loopless::prelude::*;
use rstar::{Envelope, RTree, AABB};

use battleship_plus_common::{
    types::{self, ShipType},
    util,
};

use crate::game_state::GameState;

pub struct PlacementPhasePlugin;

impl Plugin for PlacementPhasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShipEnvelopes>()
            .add_startup_system(load_assets)
            .add_enter_system(GameState::PlacementPhase, spawn_components)
            .add_system(select_ship.run_in_state(GameState::PlacementPhase))
            .add_system(place_ship.run_in_state(GameState::PlacementPhase));
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

#[derive(Resource, Deref)]
struct SelectedShip(ShipType);

#[derive(Resource, Deref, Default)]
struct ShipEnvelopes(RTree<[i32; 2]>);

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

fn select_ship(
    mut commands: Commands,
    mut selected_ship_resource: Option<ResMut<SelectedShip>>,
    key_input: Res<Input<KeyCode>>,
) {
    if key_input.just_pressed(KeyCode::Key1) {
        update_selected_ship_resource(
            &mut commands,
            &mut selected_ship_resource,
            ShipType::Destroyer,
        );
    } else if key_input.just_pressed(KeyCode::Key2) {
        update_selected_ship_resource(
            &mut commands,
            &mut selected_ship_resource,
            ShipType::Submarine,
        );
    } else if key_input.just_pressed(KeyCode::Key3) {
        update_selected_ship_resource(
            &mut commands,
            &mut selected_ship_resource,
            ShipType::Cruiser,
        );
    } else if key_input.just_pressed(KeyCode::Key4) {
        update_selected_ship_resource(
            &mut commands,
            &mut selected_ship_resource,
            ShipType::Battleship,
        );
    } else if key_input.just_pressed(KeyCode::Key5) {
        update_selected_ship_resource(
            &mut commands,
            &mut selected_ship_resource,
            ShipType::Carrier,
        );
    }
}

fn update_selected_ship_resource(
    commands: &mut Commands,
    selected_ship_resource: &mut Option<ResMut<SelectedShip>>,
    ship: ShipType,
) {
    match selected_ship_resource {
        None => commands.insert_resource(SelectedShip(ship)),
        Some(resource) => resource.0 = ship,
    };
    trace!("Selected {ship:?}");
}

fn place_ship(
    mut events: EventReader<PickingEvent>,
    selected_ship: Option<Res<SelectedShip>>,
    tiles: Query<&Tile>,
    quadrant: Res<Quadrant>,
    ship_envelopes: ResMut<ShipEnvelopes>,
) {
    let selected_ship = match selected_ship {
        Some(ship) => ship,
        None => return,
    };
    for event in events.iter() {
        let entity = match event {
            PickingEvent::Clicked(entity) => *entity,
            _ => continue,
        };

        let coordinates = match tiles.get_component::<Tile>(entity) {
            Ok(Tile { coordinate: (x, y) }) => [*x, *y],
            Err(_) => continue,
        };

        let length = ship_length(**selected_ship);
        let ship_envelope = choose_envelope(&quadrant, &ship_envelopes, coordinates, length);
        let _ship_envelope = match ship_envelope {
            Some(envelope) => envelope,
            None => {
                warn!("No legal orientation found, try a different tile.");
                continue;
            }
        };

        // TODO: Use ship manager from the common crate for placement.
    }
}

fn ship_length(ship: ShipType) -> i32 {
    match ship {
        ShipType::Destroyer => 2,
        ShipType::Submarine => 3,
        ShipType::Cruiser => 3,
        ShipType::Battleship => 4,
        ShipType::Carrier => 5,
    }
}

fn choose_envelope(
    quadrant: &Res<Quadrant>,
    ship_envelopes: &ResMut<ShipEnvelopes>,
    stern: [i32; 2],
    length: i32,
) -> Option<AABB<[i32; 2]>> {
    // TODO: Let player choose orientation.

    let ship_envelope = AABB::from_corners([stern[0], stern[1]], [stern[0] + length, stern[1]]);
    if is_legal_ship_envelope(&ship_envelope, quadrant, ship_envelopes) {
        return Some(ship_envelope);
    }

    let ship_envelope = AABB::from_corners([stern[0], stern[1]], [stern[0], stern[1] + length]);
    if is_legal_ship_envelope(&ship_envelope, quadrant, ship_envelopes) {
        return Some(ship_envelope);
    }

    if stern[0] >= length {
        let ship_envelope = AABB::from_corners([stern[0], stern[1]], [stern[0] - length, stern[1]]);
        if is_legal_ship_envelope(&ship_envelope, quadrant, ship_envelopes) {
            return Some(ship_envelope);
        }
    }

    if stern[1] >= length {
        let ship_envelope = AABB::from_corners([stern[0], stern[1]], [stern[0], stern[1] - length]);
        if is_legal_ship_envelope(&ship_envelope, quadrant, ship_envelopes) {
            return Some(ship_envelope);
        }
    }

    None
}

fn is_legal_ship_envelope(
    ship_envelope: &AABB<[i32; 2]>,
    quadrant: &Res<Quadrant>,
    ship_envelopes: &ResMut<ShipEnvelopes>,
) -> bool {
    quadrant.contains_envelope(ship_envelope)
        && ship_envelopes
            .locate_in_envelope_intersecting(ship_envelope)
            .next()
            .is_none()
}
