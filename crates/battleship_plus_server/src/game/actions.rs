use std::collections::{HashMap, HashSet};

use log::{debug, error};
use rstar::{RTreeObject, AABB};

use battleship_plus_common::game::ship::{GetShipID, Ship};
use battleship_plus_common::game::ship_manager::ShotResult;
use battleship_plus_common::game::ActionValidationError;
use battleship_plus_common::game::{ship::ShipID, PlayerID};
use battleship_plus_common::messages::ship_action_request::ActionProperties;
use battleship_plus_common::messages::*;
use battleship_plus_common::types::*;
use bevy_quinnet_server::ClientId;

use crate::game::data::{Game, Turn};
use crate::game::states::GameState;

#[derive(Debug, Clone)]
pub enum Action {
    // Lobby actions
    TeamSwitch {
        player_id: PlayerID,
    },
    SetReady {
        player_id: PlayerID,
        request: SetReadyStateRequest,
    },

    // Preparation actions
    PlaceShips {
        player_id: PlayerID,
        ship_placements: Vec<ShipAssignment>,
    },

    // Game actions
    Move {
        ship_id: ShipID,
        properties: MoveProperties,
    },
    Rotate {
        ship_id: ShipID,
        properties: RotateProperties,
    },
    Shoot {
        ship_id: ShipID,
        properties: ShootProperties,
    },
    ScoutPlane {
        ship_id: ShipID,
        properties: ScoutPlaneProperties,
    },
    PredatorMissile {
        ship_id: ShipID,
        properties: PredatorMissileProperties,
    },
    EngineBoost {
        ship_id: ShipID,
        properties: EngineBoostProperties,
    },
    Torpedo {
        ship_id: ShipID,
        properties: TorpedoProperties,
    },
    MultiMissile {
        ship_id: ShipID,
        properties: MultiMissileProperties,
    },

    None,
}

#[derive(Debug, Clone)]
pub enum ActionExecutionError {
    Validation(ActionValidationError),
    OutOfState(GameState),
    InconsistentState(String),
}

impl Action {
    pub(crate) fn apply_on(
        &self,
        game: &mut Game,
    ) -> Result<Option<ActionResult>, ActionExecutionError> {
        // TODO Implementation: implement actions below
        // TODO Implementation: add tests for all actions
        // TODO Refactor: refactor :3

        match self {
            Action::TeamSwitch { player_id } => {
                check_player_exists(game, *player_id)?;

                match (game.team_a.remove(player_id), game.team_b.remove(player_id)) {
                    (true, false) => game.team_b.insert(*player_id),
                    (false, true) => game.team_a.insert(*player_id),
                    _ => {
                        let msg = format!("found illegal team assignment for player {player_id}");
                        error!("{}", msg.as_str());
                        return Err(ActionExecutionError::InconsistentState(msg));
                    }
                };

                game.unready_players();

                Ok(None)
            }
            Action::SetReady { player_id, request } => {
                check_player_exists(game, *player_id)?;

                match game.players.get_mut(player_id) {
                    Some(p) => p.is_ready = request.ready_state,
                    None => panic!("player should exist"),
                }
                Ok(None)
            }
            Action::PlaceShips {
                player_id,
                ship_placements,
            } => match game.validate_placement_request(*player_id, ship_placements) {
                Ok(ship_placement) => ship_placement,
                Err(e) => {
                    debug!("Player {player_id} sent an invalid ship placement: {e:?}");
                    return Err(ActionExecutionError::Validation(
                        ActionValidationError::InvalidShipPlacement(e),
                    ));
                }
            }
            .iter()
            .map(|(&ship_id, ship)| game.ships.place_ship(ship_id, ship.clone()))
            .find(|res| res.is_err())
            .map_or(Ok(None), |res| match res {
                Ok(_) => Ok(None),
                Err(e) => Err(ActionExecutionError::Validation(
                    ActionValidationError::InvalidShipPlacement(e),
                )),
            }),

            Action::Move {
                ship_id,
                properties,
            } => general_movement(
                game,
                ship_id,
                |game, action_points, ship_id, bord_bounds| {
                    game.ships.move_ship(
                        action_points,
                        ship_id,
                        properties.direction(),
                        bord_bounds,
                    )
                },
            ),
            Action::Rotate {
                ship_id,
                properties,
            } => general_movement(
                game,
                ship_id,
                |game, action_points, ship_id, board_bounds| {
                    game.ships.rotate_ship(
                        action_points,
                        ship_id,
                        properties.direction(),
                        board_bounds,
                    )
                },
            ),
            Action::Shoot {
                ship_id,
                properties,
            } => {
                let player_id = ship_id.0;
                check_player_exists(game, player_id)?;
                check_players_turn(game, player_id)?;

                let target = [
                    properties.target.as_ref().unwrap().x as i32,
                    properties.target.as_ref().unwrap().y as i32,
                ];

                let bounds = game.board_bounds();
                let player = game.players.get(&player_id).unwrap().clone();

                let mut action_points = game.turn.as_ref().unwrap().action_points_left;
                match game
                    .ships
                    .attack_with_ship(&mut action_points, ship_id, &target, &bounds)
                {
                    Ok(shot) => {
                        game.turn.as_mut().unwrap().action_points_left = action_points;
                        game.players
                            .insert(player_id, player)
                            .expect("unable to update player");

                        match shot {
                            ShotResult::Miss => Ok(None),
                            ShotResult::Hit(ship_id, damage) => {
                                Ok(Some(ActionResult::hit(ship_id, &target, damage)))
                            }
                            ShotResult::Destroyed(ship_id, damage) => {
                                Ok(Some(ActionResult::destroyed(ship_id, &target, damage)))
                            }
                        }
                    }
                    Err(e) => Err(ActionExecutionError::Validation(e)),
                }
            }
            // TODO Implementation: Action::ScoutPlane { .. } => {}
            // TODO Implementation: Action::PredatorMissile { .. } => {}
            // TODO Implementation: Action::EngineBoost { .. } => {}
            // TODO Implementation: Action::Torpedo { .. } => {}
            // TODO Implementation: Action::MultiMissile { .. } => {}
            Action::None => Ok(None),
            _ => todo!(),
        }
    }
}

fn general_movement<
    F: FnOnce(
        &mut Game,
        &mut u32,
        &ShipID,
        &AABB<[i32; 2]>,
    ) -> Result<AABB<[i32; 2]>, ActionValidationError>,
>(
    game: &mut Game,
    ship_id: &ShipID,
    do_movement: F,
) -> Result<Option<ActionResult>, ActionExecutionError> {
    let player_id = ship_id.0;
    check_player_exists(game, player_id)?;
    check_players_turn(game, player_id)?;

    let board_bounds = game.board_bounds();
    let player = game.players.get(&player_id).unwrap().clone();

    let mut action_points = game.turn.as_ref().unwrap().action_points_left;

    let old_vision = game.ships.get_ship_parts_seen_by([*ship_id].as_slice());
    let trajectory = match do_movement(game, &mut action_points, ship_id, &board_bounds) {
        Ok(trajectory) => trajectory,
        Err(e) => return Err(ActionExecutionError::Validation(e)),
    };
    game.turn.as_mut().unwrap().action_points_left = action_points;

    // update player stats
    game.players.insert(player_id, player);

    let new_vision = game.ships.get_ship_parts_seen_by([*ship_id].as_slice());

    let destroyed_ships = game.ships.destroy_colliding_ships_in_envelope(&trajectory);

    Ok(Some(ActionResult::movement_result(
        &destroyed_ships,
        &old_vision,
        &new_vision,
    )))
}

impl From<(ClientId, &SetPlacementRequest)> for Action {
    fn from((client_id, request): (ClientId, &SetPlacementRequest)) -> Self {
        Self::PlaceShips {
            player_id: client_id,
            ship_placements: request.clone().assignments,
        }
    }
}

impl From<(ClientId, &ShipActionRequest)> for Action {
    fn from((client_id, request): (ClientId, &ShipActionRequest)) -> Self {
        let ship_id: ShipID = (client_id, request.ship_number);
        match request.clone().action_properties {
            None => Action::None,
            Some(p) => match p {
                ActionProperties::MoveProperties(props) => Action::Move {
                    ship_id,
                    properties: props,
                },
                ActionProperties::ShootProperties(props) => Action::Shoot {
                    ship_id,
                    properties: props,
                },
                ActionProperties::RotateProperties(props) => Action::Rotate {
                    ship_id,
                    properties: props,
                },
                ActionProperties::TorpedoProperties(props) => Action::Torpedo {
                    ship_id,
                    properties: props,
                },
                ActionProperties::ScoutPlaneProperties(props) => Action::ScoutPlane {
                    ship_id,
                    properties: props,
                },
                ActionProperties::MultiMissileProperties(props) => Action::MultiMissile {
                    ship_id,
                    properties: props,
                },
                ActionProperties::PredatorMissileProperties(props) => Action::PredatorMissile {
                    ship_id,
                    properties: props,
                },
                ActionProperties::EngineBoostProperties(props) => Action::EngineBoost {
                    ship_id,
                    properties: props,
                },
            },
        }
    }
}

fn check_player_exists(game: &Game, id: PlayerID) -> Result<(), ActionExecutionError> {
    if !game.players.contains_key(&id) {
        let msg = format!("PlayerID {id} is unknown",);
        debug!("{}", msg.as_str());
        Err(ActionExecutionError::Validation(
            ActionValidationError::NonExistentPlayer { id },
        ))
    } else {
        Ok(())
    }
}

fn check_players_turn(game: &Game, id: PlayerID) -> Result<(), ActionExecutionError> {
    match game.turn {
        Some(Turn { player_id, .. }) if player_id == id => Ok(()),
        _ => Err(ActionExecutionError::Validation(
            ActionValidationError::NotPlayersTurn,
        )),
    }
}

#[derive(Debug, Clone)]
pub struct ActionResult {
    pub inflicted_damage_at: Vec<(i32, i32)>,
    pub inflicted_damage_by_ship: HashMap<ShipID, u32>,
    pub ships_destroyed: HashSet<ShipID>,
    pub gain_vision_at: Vec<Coordinate>,
    pub lost_vision_at: Vec<Coordinate>,
}

impl ActionResult {
    fn movement_result(
        destroyed_ships: &Option<Vec<Ship>>,
        old_vision: &[Coordinate],
        new_vision: &[Coordinate],
    ) -> Self {
        ActionResult {
            inflicted_damage_at: destroyed_ships
                .as_ref()
                .map_or(Vec::with_capacity(0), |ships| {
                    ships
                        .iter()
                        .flat_map(|ship| envelope_to_points(&ship.envelope()).collect::<Vec<_>>())
                        .collect()
                }),
            inflicted_damage_by_ship: destroyed_ships.as_ref().map_or(
                HashMap::with_capacity(0),
                |ships| {
                    ships.iter().fold(HashMap::new(), |mut acc, ship| {
                        acc.insert(ship.id(), ship.data().health);
                        acc
                    })
                },
            ),
            ships_destroyed: destroyed_ships
                .as_ref()
                .map_or(HashSet::with_capacity(0), |ships| {
                    HashSet::from_iter(ships.iter().map(|s| s.id()))
                }),
            gain_vision_at: difference(new_vision, old_vision),
            lost_vision_at: difference(old_vision, new_vision),
        }
    }

    fn hit(ship_id: ShipID, target: &[i32; 2], damage: u32) -> Self {
        ActionResult {
            inflicted_damage_at: vec![(target[0], target[1])],
            inflicted_damage_by_ship: HashMap::from([(ship_id, damage)]),
            ships_destroyed: HashSet::with_capacity(0),
            gain_vision_at: Vec::with_capacity(0),
            lost_vision_at: Vec::with_capacity(0),
        }
    }

    fn destroyed(ship_id: ShipID, target: &[i32; 2], damage: u32) -> Self {
        ActionResult {
            inflicted_damage_at: vec![(target[0], target[1])],
            inflicted_damage_by_ship: HashMap::from([(ship_id, damage)]),
            ships_destroyed: HashSet::from([ship_id]),
            gain_vision_at: Vec::with_capacity(0),
            lost_vision_at: Vec::with_capacity(0),
        }
    }
}

fn envelope_to_points(envelope: &AABB<[i32; 2]>) -> impl Iterator<Item = (i32, i32)> + '_ {
    (envelope.lower()[0]..=envelope.lower()[1])
        .flat_map(|x| (envelope.upper()[0]..=envelope.upper()[1]).map(move |y| (x, y)))
}

fn difference<T: PartialEq + Clone>(left: &[T], right: &[T]) -> Vec<T> {
    left.iter()
        .filter(|c| !right.contains(c))
        .cloned()
        .collect()
}
