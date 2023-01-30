use std::f32::consts::{FRAC_PI_2, PI};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bevy::prelude::*;
use bevy_mod_raycast::{Intersection, RaycastMesh, RaycastMethod, RaycastSource, RaycastSystem};
use bevy_quinnet_client::Client;
use iyes_loopless::prelude::*;
use rstar::{Envelope, RTreeObject, AABB};

use battleship_plus_common::{
    game::{
        ship::{GetShipID, Orientation, Ship, ShipID},
        ship_manager::ShipManager,
    },
    messages::{self, EventMessage, GameStart, SetPlacementRequest, StatusCode, StatusMessage},
    types::{self, ShipAssignment, ShipType, Teams},
    util,
};

use crate::{
    game_state::{GameState, PlayerId},
    lobby::LobbyState,
    networking::{self, CurrentServer, ServerInformation},
    RaycastSet,
};

pub struct PlacementPhasePlugin;

impl Plugin for PlacementPhasePlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(load_assets)
            .add_enter_system(GameState::PlacementPhase, create_resources)
            .add_enter_system(GameState::PlacementPhase, spawn_components)
            .add_system_to_stage(
                CoreStage::First,
                update_raycast_with_cursor.before(RaycastSystem::BuildRays::<RaycastSet>),
            )
            .add_system(select_ship.run_in_state(GameState::PlacementPhase))
            .add_system(place_ship.run_in_state(GameState::PlacementPhase))
            .add_system(send_placement.run_in_state(GameState::PlacementPhase))
            .add_system(process_responses.run_in_state(GameState::PlacementPhase))
            .add_system(process_game_start_event.run_in_state(GameState::PlacementPhase));
    }
}

#[derive(Resource, Deref)]
pub struct Quadrant(AABB<[i32; 2]>);

impl Quadrant {
    pub fn new(corner: types::Coordinate, quadrant_size: u32) -> Quadrant {
        let corner = (corner.x, corner.y);
        Quadrant(util::quadrant_from_corner(corner, quadrant_size))
    }
}

#[derive(Resource)]
struct GameAssets {
    ocean_scene: Handle<Scene>,
}

#[derive(Resource, Deref)]
struct SelectedShip(ShipType);

#[derive(Resource, Deref, DerefMut, Default)]
struct Ships(ShipManager);

#[derive(Resource, Deref)]
struct PlayerTeam(Teams);

#[derive(Resource, Deref)]
struct Config(Arc<types::Config>);

enum State {
    Placing,
    WaitingForResponse,
    WaitingForGameStart,
}

#[derive(Resource)]
struct PlacementState(State);

impl Default for PlacementState {
    fn default() -> Self {
        Self(State::Placing)
    }
}

#[derive(Component)]
struct ShipInfo {
    _ship_id: ShipID,
}

#[derive(Resource, Deref)]
struct ShipMeshes(HashMap<ShipType, Handle<Mesh>>);

const OCEAN_SIZE: f32 = 320.0;
const OFFSET_X: f32 = OCEAN_SIZE / 2.0;
const OFFSET_Y: f32 = OCEAN_SIZE / 2.0;
const OFFSET_Z: f32 = 50.0;

#[derive(Bundle)]
struct ShipBundle {
    model: PbrBundle,
    ship_info: ShipInfo,
}

impl ShipBundle {
    fn new(ship: &Ship, meshes: &Res<ShipMeshes>, quadrant_size: i32) -> Self {
        let scale = OCEAN_SIZE / quadrant_size as f32;
        let position = ship.position();
        let translation = Vec3::new(
            (position.0 as f32 + 0.5) * scale - OFFSET_X,
            (position.1 as f32 + 0.5) * scale - OFFSET_Y,
            0.0,
        );
        let rotation = Quat::from_rotation_z(match ship.orientation() {
            Orientation::North => FRAC_PI_2,
            Orientation::East => 0.0,
            Orientation::South => -FRAC_PI_2,
            Orientation::West => PI,
        });
        let scale = Vec3::new(scale, scale, 1.0);
        Self {
            model: PbrBundle {
                mesh: meshes
                    .get(&ship.ship_type())
                    .expect("There are meshes for all configured ship types")
                    .clone(),
                transform: Transform::from_translation(translation)
                    .with_rotation(rotation)
                    .with_scale(scale),
                ..default()
            },
            ship_info: ShipInfo {
                _ship_id: ship.id(),
            },
        }
    }
}

fn load_assets(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(GameAssets {
        ocean_scene: assets.load("models/ocean.glb#Scene0"),
    });
}

fn create_resources(
    mut commands: Commands,
    lobby_state: Res<LobbyState>,
    player_id: Res<PlayerId>,
    current_server: Res<CurrentServer>,
    servers: Query<&ServerInformation>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    commands.init_resource::<Ships>();
    commands.init_resource::<PlacementState>();

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

    let ship_lengths: HashMap<ShipType, usize> = HashMap::from_iter(vec![
        (ShipType::Destroyer, 2),
        (ShipType::Submarine, 3),
        (ShipType::Cruiser, 3),
        (ShipType::Battleship, 4),
        (ShipType::Carrier, 5),
    ]);

    let ship_meshes = ship_lengths
        .iter()
        .map(|(ship_type, length)| {
            (
                *ship_type,
                meshes.add(
                    shape::Box {
                        min_x: -0.5,
                        max_x: -0.5 + *length as f32,
                        min_y: -0.5,
                        max_y: 0.5,
                        min_z: 0.0,
                        max_z: 5.0,
                    }
                    .into(),
                ),
            )
        })
        .collect();
    commands.insert_resource(ShipMeshes(ship_meshes));
}

fn spawn_components(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands
        .spawn(SceneBundle {
            scene: assets.ocean_scene.clone(),
            ..default()
        })
        .insert(Name::new("Ocean"))
        .insert(RaycastMesh::<RaycastSet>::default());
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

    let mesh = meshes.add(Mesh::from(shape::Plane { size: OCEAN_SIZE }));
    let material = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        base_color: Color::rgba(1.0, 1.0, 1.0, 0.2),
        ..default()
    });

    commands
        .spawn(PbrBundle {
            mesh,
            material,
            transform: Transform::from_xyz(0.0, 0.0, OFFSET_Z)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            ..default()
        })
        .insert(RaycastMesh::<RaycastSet>::default())
        .insert(Name::new("Grid"));
}

// Taken from bevy_mod_raycast examples.
fn update_raycast_with_cursor(
    mut cursor: EventReader<CursorMoved>,
    mut query: Query<&mut RaycastSource<RaycastSet>>,
) {
    // Grab the most recent cursor event if it exists:
    let cursor_position = match cursor.iter().last() {
        Some(cursor_moved) => cursor_moved.position,
        None => return,
    };

    for mut pick_source in &mut query {
        pick_source.cast_method = RaycastMethod::Screenspace(cursor_position);
    }
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
    mut commands: Commands,
    intersections: Query<&Intersection<RaycastSet>>,
    selected_ship: Option<Res<SelectedShip>>,
    (quadrant, config, ship_meshes): (Res<Quadrant>, Res<Config>, Res<ShipMeshes>),
    mut ships: ResMut<Ships>,
    (player_id, player_team): (Res<PlayerId>, Res<PlayerTeam>),
    mouse_input: Res<Input<MouseButton>>,
) {
    let intersection = match intersections.get_single() {
        Ok(intersection) => intersection,
        Err(_) => return,
    };
    let selected_ship = match selected_ship {
        Some(ship) => ship,
        None => return,
    };
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }

    let quadrant_size = quadrant.upper()[0] + 1 - quadrant.lower()[0];
    let coordinates = match intersection.position() {
        Some(Vec3 { x, y, .. }) => {
            let mut coordinates = [(OFFSET_X + *x), (OFFSET_Y + *y)];
            coordinates[0] /= OCEAN_SIZE;
            coordinates[1] /= OCEAN_SIZE;
            coordinates[0] *= quadrant_size as f32;
            coordinates[1] *= quadrant_size as f32;
            [coordinates[0] as i32, coordinates[1] as i32]
        }
        None => return,
    };

    let ship_id = next_ship_id(**selected_ship, &ships, &config, &player_id, &player_team);
    let ship_id = match ship_id {
        Some(id) => id,
        None => {
            warn!(
                "Already placed all available ships of type {:?}",
                **selected_ship
            );
            return;
        }
    };

    choose_orientation_and_place_ship(
        &mut commands,
        &quadrant,
        &mut ships,
        (**selected_ship, ship_id),
        coordinates,
        &config,
        &ship_meshes,
    );
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

fn are_all_ships_placed(
    ships: &Res<Ships>,
    config: &Res<Config>,
    player_id: &Res<PlayerId>,
    team: &Res<PlayerTeam>,
) -> bool {
    let used_ship_ids: HashSet<_> = ships
        .iter_ships()
        .filter(|((ship_player_id, _), _)| *ship_player_id == ***player_id)
        .map(|((_, ship_id), _)| *ship_id)
        .collect();
    let ship_set = match ***team {
        Teams::TeamA => &config.ship_set_team_a,
        Teams::TeamB => &config.ship_set_team_b,
        Teams::None => unreachable!(),
    };
    !(0..ship_set.len() as u32)
        .into_iter()
        .any(|id| !used_ship_ids.contains(&id))
}

fn choose_orientation_and_place_ship(
    commands: &mut Commands,
    quadrant: &Res<Quadrant>,
    ships: &mut ResMut<Ships>,
    (ship_type, ship_id): (ShipType, ShipID),
    stern: [i32; 2],
    config: &Res<Config>,
    ship_meshes: &Res<ShipMeshes>,
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
        let quadrant_size = quadrant.upper()[0] + 1 - quadrant.lower()[0];
        if quadrant.contains_envelope(envelope) && ships.place_ship(ship_id, ship).is_ok() {
            trace!("Placed a {ship_type:?} at {position:?} with orientation {orientation:?}. {envelope:?}");
            let ship = ships
                .iter_ships()
                .find_map(|(this_ship_id, ship)| {
                    if *this_ship_id == ship_id {
                        Some(ship)
                    } else {
                        None
                    }
                })
                .expect("This ship was just inserted");
            commands
                .spawn(ShipBundle::new(ship, ship_meshes, quadrant_size))
                .insert(Name::new(format!("{ship_type:?}: {ship_id:?}")));
            return;
        }
    }

    warn!("That ship does not fit here, try a different tile");
}

fn send_placement(
    mut commands: Commands,
    key_input: Res<Input<KeyCode>>,
    ships: Res<Ships>,
    config: Res<Config>,
    (player_id, team): (Res<PlayerId>, Res<PlayerTeam>),
    client: Res<Client>,
    mut placement_state: ResMut<PlacementState>,
) {
    if !key_input.just_pressed(KeyCode::Return) {
        return;
    }
    if let State::WaitingForResponse = placement_state.0 {
        warn!("Still waiting for a response, cannot send placements.");
        return;
    }
    if !are_all_ships_placed(&ships, &config, &player_id, &team) {
        warn!("Not all ships are placed yet, cannot send placements.");
        return;
    }
    let assignments = ships
        .iter_ships()
        .filter(|((ship_player_id, _), _)| *ship_player_id == **player_id)
        .map(|((_, ship_id), ship)| {
            let position = ship.position();
            let coordinate = Some(types::Coordinate {
                x: position.0 as u32,
                y: position.1 as u32,
            });
            ShipAssignment {
                ship_number: *ship_id,
                coordinate,
                direction: types::Direction::from(ship.orientation()) as i32,
            }
        })
        .collect();

    let message = SetPlacementRequest { assignments };
    if let Err(error) = client.connection().send_message(message.into()) {
        error!("Could not send SetPlacementRequest: {error}, disonnecting");
        commands.insert_resource(NextState(GameState::Unconnected));
    }
    placement_state.0 = State::WaitingForResponse;
}

fn process_responses(
    mut commands: Commands,
    mut events: EventReader<networking::ResponseReceivedEvent>,
    mut placement_state: ResMut<PlacementState>,
) {
    for networking::ResponseReceivedEvent(StatusMessage {
        code,
        message,
        data,
    }) in events.iter()
    {
        let original_code = code;
        let code = StatusCode::from_i32(*code);
        match code {
            Some(StatusCode::Ok) => {
                process_response_data(data, message, &mut placement_state);
            }
            Some(StatusCode::OkWithWarning) => {
                if message.is_empty() {
                    warn!("Received OK response with warning but without message");
                } else {
                    warn!("Received OK response with warning: {message}");
                }
                process_response_data(data, message, &mut placement_state);
            }
            Some(StatusCode::BadRequest) => {
                if message.is_empty() {
                    warn!("Illegal ship placement");
                } else {
                    warn!("Illegal ship placement: {message}");
                }
                placement_state.0 = State::Placing;
            }
            Some(StatusCode::ServerError) => {
                if message.is_empty() {
                    error!("Server error, disconnecting");
                } else {
                    error!("Server error with message \"{message}\", disconnecting");
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            Some(StatusCode::UnsupportedVersion) => {
                if message.is_empty() {
                    error!("Unsupported protocol version, disconnecting");
                } else {
                    error!("Unsupported protocol version, disconnecting. Attached message: \"{message}\"");
                }
            }
            Some(other_code) => {
                if message.is_empty() {
                    error!("Received inappropriate status code {other_code:?}, disconnecting");
                } else {
                    error!("Received inappropriate status code {other_code:?} with message \"{message}\", disconnecting");
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            None => {
                if message.is_empty() {
                    error!("Received unknown status code {original_code}, disconnecting");
                } else {
                    error!("Received unknown status code {original_code} with message \"{message}\", disconnecting");
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
        }
    }
}

fn process_response_data(
    data: &Option<messages::status_message::Data>,
    message: &str,
    placement_state: &mut ResMut<PlacementState>,
) {
    match data {
        Some(messages::status_message::Data::PlacementResponse(_)) => {
            if let State::WaitingForResponse = placement_state.0 {
                debug!("Placement successful, waiting for game to begin...");
                placement_state.0 = State::WaitingForGameStart;
            } else {
                warn!("Received unexpected PlacementResponse");
            }
        }
        Some(_other_response) => {
            // ignore
        }
        None => {
            if message.is_empty() {
                warn!("No data in OK response");
            } else {
                warn!("No data in OK response with message: {message}");
            }
            // ignore
        }
    }
}

fn process_game_start_event(
    mut commands: Commands,
    mut events: EventReader<EventMessage>,
    placement_state: Res<PlacementState>,
) {
    for event in events.iter() {
        match event {
            EventMessage::GameStart(GameStart {
                state: Some(_state),
            }) => {
                match placement_state.0 {
                    State::WaitingForGameStart => {}
                    State::WaitingForResponse => {
                        debug!("Received unexpected GameStart, interpreting it as successful placement")
                    }
                    State::Placing => {
                        warn!("Received unexpected GameStart");
                        continue;
                    }
                }

                // TODO: Game initialization.
                warn!("Unimplemented: skipping game initialization");
                commands.insert_resource(NextState(GameState::Game));
            }
            EventMessage::GameStart(GameStart { state: None }) => {
                // TODO: Robustness: request server state manually.
                error!("Received GameStart without server state, disconnecting");
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            _other_events => {
                // ignore
            }
        }
    }
}
