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
            .add_system_to_stage(
                CoreStage::First,
                create_resources.run_in_state(GameState::Lobby).run_if(
                    |next_state: Option<Res<NextState<GameState>>>| {
                        if let Some(next_state) = next_state {
                            matches!(*next_state, NextState(GameState::PlacementPhase))
                        } else {
                            false
                        }
                    },
                ),
            )
            .add_enter_system(GameState::PlacementPhase, spawn_components)
            .add_enter_system(GameState::PlacementPhase, move_camera)
            .add_exit_system(GameState::PlacementPhase, despawn_components)
            .add_system_to_stage(
                CoreStage::First,
                update_raycast_with_cursor.before(RaycastSystem::BuildRays::<RaycastSet>),
            )
            .add_system(select_ship.run_in_state(GameState::PlacementPhase))
            .add_system(rotate_ship.run_in_state(GameState::PlacementPhase))
            .add_system(preview_ship.run_in_state(GameState::PlacementPhase))
            .add_system(place_ship.run_in_state(GameState::PlacementPhase))
            .add_system(send_placement.run_in_state(GameState::PlacementPhase))
            .add_system(process_responses.run_in_state(GameState::PlacementPhase))
            .add_system(process_game_start_event.run_in_state(GameState::PlacementPhase))
            .add_system(leave_game.run_in_state(GameState::PlacementPhase));
    }
}

#[derive(Resource, Deref)]
pub struct Quadrant(AABB<[i32; 2]>);

impl Quadrant {
    pub fn new(corner: types::Coordinate, quadrant_size: u32) -> Quadrant {
        let corner = (corner.x, corner.y);
        Quadrant(util::quadrant_from_corner(corner, quadrant_size))
    }

    pub fn side_length(&self) -> i32 {
        self.upper()[0] + 1 - self.lower()[0]
    }
}

#[derive(Resource)]
struct GameAssets {
    ocean_scene: Handle<Scene>,
}

#[derive(Resource)]
struct SelectedShip {
    ship: ShipType,
    orientation: Orientation,
}

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
const OFFSET_Z: f32 = 50.0;

fn new_ship_model(ship: &Ship, meshes: &Res<ShipMeshes>) -> PbrBundle {
    let position = ship.position();
    let translation = Vec3::new(position.0 as f32 + 0.5, position.1 as f32 + 0.5, 0.0);
    let rotation = Quat::from_rotation_z(match ship.orientation() {
        Orientation::North => FRAC_PI_2,
        Orientation::East => 0.0,
        Orientation::South => -FRAC_PI_2,
        Orientation::West => PI,
    });
    PbrBundle {
        mesh: meshes
            .get(&ship.ship_type())
            .expect("There are meshes for all configured ship types")
            .clone(),
        transform: Transform::from_translation(translation).with_rotation(rotation),
        ..default()
    }
}

#[derive(Bundle)]
struct ShipBundle {
    model: PbrBundle,
    ship_info: ShipInfo,
}

#[derive(Component)]
struct ShipPreview;

impl ShipBundle {
    fn new(ship: &Ship, meshes: &Res<ShipMeshes>) -> Self {
        Self {
            model: new_ship_model(ship, meshes),
            ship_info: ShipInfo {
                _ship_id: ship.id(),
            },
        }
    }
}

#[derive(Component)]
struct DespawnOnExit;

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
    quadrant: Res<Quadrant>,
    config: Res<Config>,
    assets: Res<GameAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scale = config.board_size as f32 / OCEAN_SIZE;
    let transform = Transform::from_translation(Vec3::new(
        scale * OCEAN_SIZE / 2.0,
        scale * OCEAN_SIZE / 2.0,
        0.0,
    ))
    .with_scale(Vec3::new(scale, scale, 1.0));
    commands
        .spawn(SceneBundle {
            scene: assets.ocean_scene.clone(),
            transform,
            ..default()
        })
        .insert(Name::new("Ocean"))
        .insert(RaycastMesh::<RaycastSet>::default())
        .insert(DespawnOnExit);
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
        .insert(Name::new("Directional Light"))
        .insert(DespawnOnExit);

    let mesh = meshes.add(Mesh::from(shape::Plane {
        size: quadrant.side_length() as f32,
    }));
    let material = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        base_color: Color::rgba_u8(42, 0, 142, 69),
        ..default()
    });
    let click_plane_offset = quadrant.side_length() as f32 / 2.0;

    commands
        .spawn(PbrBundle {
            mesh,
            material,
            transform: Transform::from_xyz(
                quadrant.lower()[0] as f32 + click_plane_offset,
                quadrant.lower()[1] as f32 + click_plane_offset,
                OFFSET_Z,
            )
            .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            ..default()
        })
        .insert(RaycastMesh::<RaycastSet>::default())
        .insert(Name::new("Grid"))
        .insert(DespawnOnExit);
}

fn move_camera(mut camera: Query<(&mut Transform, With<Camera3d>)>, quadrant: Res<Quadrant>) {
    // TODO: Scale the camera so that the quadrant is entirely visible (?).
    let half_quadrant_size = quadrant.side_length() as f32 / 2.0;
    let mut camera_transform = camera.single_mut().0;
    camera_transform.translation = Vec3::new(
        quadrant.lower()[0] as f32 + half_quadrant_size,
        quadrant.lower()[1] as f32 + half_quadrant_size,
        camera_transform.translation.z,
    );
}

fn despawn_components(
    mut commands: Commands,
    entities_to_despawn: Query<Entity, With<DespawnOnExit>>,
) {
    for entity in entities_to_despawn.iter() {
        commands.entity(entity).despawn_recursive();
    }
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
    mut selected_ship: Option<ResMut<SelectedShip>>,
    key_input: Res<Input<KeyCode>>,
) {
    if key_input.just_pressed(KeyCode::Key1) {
        update_ship_selection(&mut commands, &mut selected_ship, ShipType::Destroyer);
    } else if key_input.just_pressed(KeyCode::Key2) {
        update_ship_selection(&mut commands, &mut selected_ship, ShipType::Submarine);
    } else if key_input.just_pressed(KeyCode::Key3) {
        update_ship_selection(&mut commands, &mut selected_ship, ShipType::Cruiser);
    } else if key_input.just_pressed(KeyCode::Key4) {
        update_ship_selection(&mut commands, &mut selected_ship, ShipType::Battleship);
    } else if key_input.just_pressed(KeyCode::Key5) {
        update_ship_selection(&mut commands, &mut selected_ship, ShipType::Carrier);
    }
}

fn update_ship_selection(
    commands: &mut Commands,
    selected: &mut Option<ResMut<SelectedShip>>,
    ship: ShipType,
) {
    match selected {
        None => commands.insert_resource(SelectedShip {
            ship,
            orientation: Orientation::North,
        }),
        Some(selected) => selected.ship = ship,
    };
    trace!("Selected {ship:?}");
}

fn rotate_ship(selected: Option<ResMut<SelectedShip>>, key_input: Res<Input<KeyCode>>) {
    if !key_input.just_pressed(KeyCode::R) {
        return;
    }
    let mut selected = match selected {
        Some(ship) => ship,
        None => return,
    };
    let counter_clockwise = key_input.pressed(KeyCode::LShift);

    selected.orientation = if counter_clockwise {
        match selected.orientation {
            Orientation::North => Orientation::West,
            Orientation::East => Orientation::North,
            Orientation::South => Orientation::East,
            Orientation::West => Orientation::South,
        }
    } else {
        match selected.orientation {
            Orientation::North => Orientation::East,
            Orientation::East => Orientation::South,
            Orientation::South => Orientation::West,
            Orientation::West => Orientation::North,
        }
    };
}

fn preview_ship(
    mut commands: Commands,
    preview: Query<(Entity, With<ShipPreview>)>,
    intersections: Query<&Intersection<RaycastSet>>,
    selected: Option<ResMut<SelectedShip>>,
    (config, ship_meshes): (Res<Config>, Res<ShipMeshes>),
) {
    let position = board_position_from_intersection(intersections);
    let (selected, position) = if let (Some(selected), Some(position)) = (selected, position) {
        (selected, (position[0] as u32, position[1] as u32))
    } else {
        // It is inappropriate to show a selection, nothing is selected or the mouse is not on the board.
        if let Ok((entity, ())) = preview.get_single() {
            commands.entity(entity).despawn_recursive();
        }
        return;
    };

    let ship = Ship::new_from_type(
        selected.ship,
        (u32::MAX, u32::MAX),
        position,
        selected.orientation,
        config.0.clone(),
    );
    let new_model = new_ship_model(&ship, &ship_meshes);

    if preview.is_empty() {
        commands.spawn(ShipPreview)
    } else {
        commands.entity(preview.single().0)
    }
    .insert(new_model);
}

fn place_ship(
    mut commands: Commands,
    intersections: Query<&Intersection<RaycastSet>>,
    selected: Option<Res<SelectedShip>>,
    (quadrant, config, ship_meshes): (Res<Quadrant>, Res<Config>, Res<ShipMeshes>),
    mut ships: ResMut<Ships>,
    (player_id, player_team): (Res<PlayerId>, Res<PlayerTeam>),
    mouse_input: Res<Input<MouseButton>>,
) {
    let selected = match selected {
        Some(ship) => ship,
        None => return,
    };
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }
    let position = match board_position_from_intersection(intersections) {
        Some(position) => (position[0] as u32, position[1] as u32),
        None => return,
    };

    let ship_id = next_ship_id(selected.ship, &ships, &config, &player_id, &player_team);
    let ship_id = match ship_id {
        Some(id) => id,
        None => {
            warn!(
                "Already placed all available ships of type {:?}",
                selected.ship
            );
            return;
        }
    };

    let ship = Ship::new_from_type(
        selected.ship,
        ship_id,
        position,
        selected.orientation,
        config.0.clone(),
    );
    let envelope = &ship.envelope();
    if quadrant.contains_envelope(envelope) && ships.place_ship(ship_id, ship).is_ok() {
        trace!(
            "Placed a {:?} at {position:?} with orientation {:?}. {envelope:?}",
            selected.ship,
            selected.orientation
        );
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
            .spawn(ShipBundle::new(ship, &ship_meshes))
            .insert(Name::new(format!("{:?}: {ship_id:?}", selected.ship)));
    } else {
        warn!("That ship does not fit here, try a different tile");
    };
}

fn board_position_from_intersection(
    intersections: Query<&Intersection<RaycastSet>>,
) -> Option<[i32; 2]> {
    let intersection = intersections.get_single().ok()?;
    intersection
        .position()
        .map(|Vec3 { x, y, .. }| [*x as i32, *y as i32])
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

fn leave_game(mut commands: Commands, input: Res<Input<KeyCode>>) {
    if input.just_pressed(KeyCode::Escape) {
        info!("Disconnecting from the server on user request");
        commands.insert_resource(NextState(GameState::Unconnected));
    }
}
