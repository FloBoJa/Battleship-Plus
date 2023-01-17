use std::{collections::HashSet, sync::Arc};

use bevy::prelude::*;
use bevy_mod_picking::{PickableBundle, PickingEvent};
use iyes_loopless::prelude::*;
use rstar::{Envelope, RTreeObject, AABB};

use battleship_plus_common::{
    game::{
        ship::{Orientation, Ship, ShipID},
        ship_manager::ShipManager,
    },
    types::{self, ShipType, Teams},
    util,
};

use crate::{
    game_state::{GameState, PlayerId},
    lobby::LobbyState,
    networking::{CurrentServer, ServerInformation},
};

pub struct PlacementPhasePlugin;

impl Plugin for PlacementPhasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ships>()
            .add_startup_system(load_assets)
            .add_enter_system(GameState::PlacementPhase, create_convenience_resources)
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

#[derive(Resource, Deref, DerefMut, Default)]
struct Ships(ShipManager);

#[derive(Resource, Deref)]
struct PlayerTeam(Teams);

#[derive(Resource, Deref)]
struct Config(Arc<types::Config>);

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

fn create_convenience_resources(
    mut commands: Commands,
    lobby_state: Res<LobbyState>,
    player_id: Res<PlayerId>,
    current_server: Res<CurrentServer>,
    servers: Query<&ServerInformation>,
) {
    let is_team_a = lobby_state
        .team_state_a
        .iter()
        .any(|player_state| player_state.player_id == **player_id);
    let team = if is_team_a {
        Teams::TeamA
    } else {
        Teams::TeamB
    };
    commands.insert_resource(PlayerTeam(team));

    let config = servers
        .get_component::<ServerInformation>(**current_server)
        .expect("Current server always has a ServerInformation")
        .config
        .clone()
        .expect("Joined server always has a configuration");
    commands.insert_resource(Config(Arc::new(config)));
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
    mut ships: ResMut<Ships>,
    player_id: Res<PlayerId>,
    player_team: Res<PlayerTeam>,
    config: Res<Config>,
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

        // TODO: Add config and team association as resources in lobby stage
        let ship_id = next_ship_id(**selected_ship, &ships, &config, &player_id, &player_team);
        let ship_id = match ship_id {
            Some(id) => id,
            None => {
                warn!(
                    "Already placed all available ships of type {:?}",
                    **selected_ship
                );
                continue;
            }
        };
        choose_orientation_and_place_ship(
            &quadrant,
            &mut ships,
            **selected_ship,
            ship_id,
            coordinates,
            &config,
        );
    }
}

fn next_ship_id(
    ship_type: ShipType,
    ships: &ResMut<Ships>,
    config: &Res<Config>,
    player_id: &Res<PlayerId>,
    team: &Res<PlayerTeam>,
) -> Option<ShipID> {
    let used_ship_ids: HashSet<_> = ships
        .iter_ships()
        .filter(|((ship_player_id, _), _)| *ship_player_id == ***player_id)
        .filter(|(_, ship)| ship.ship_type() == ship_type)
        .map(|((_, ship_id), _)| *ship_id)
        .collect();
    let ship_set = match ***team {
        Teams::TeamA => &config.ship_set_team_a,
        Teams::TeamB => &config.ship_set_team_b,
        Teams::None => unreachable!(),
    };
    (0..ship_set.len())
        .into_iter()
        .map(|i| (i as u32, ShipType::from_i32(ship_set[i])))
        .map(|(id, ship_type)| (id, ship_type.expect("Ship sets contain ShipTypes")))
        .filter(|(_, entry_ship_type)| *entry_ship_type == ship_type)
        .map(|(id, _)| id)
        .find(|id| !used_ship_ids.contains(id))
        .map(|ship_id| (***player_id, ship_id))
}

fn choose_orientation_and_place_ship(
    quadrant: &Res<Quadrant>,
    ships: &mut ResMut<Ships>,
    ship_type: ShipType,
    ship_id: ShipID,
    stern: [i32; 2],
    config: &Res<Config>,
) {
    // TODO: Let player choose orientation.

    let position = (stern[0] as u32, stern[1] as u32);
    for orientation in [
        Orientation::North,
        Orientation::East,
        Orientation::South,
        Orientation::West,
    ] {
        let ship = Ship::new_from_type(ship_type, ship_id, position, orientation, config.0.clone());
        let envelope = &ship.envelope();
        if quadrant.contains_envelope(envelope) && ships.place_ship(ship_id, ship).is_ok() {
            trace!("Placed a {ship_type:?} at {:?}", envelope);
            return;
        }
    }

    warn!("That ship does not fit here, try a different tile");
}
