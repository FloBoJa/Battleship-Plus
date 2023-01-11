use log::{debug, error};
use tokio::sync::RwLockWriteGuard;

use battleship_plus_common::messages::ship_action_request::ActionProperties;
use battleship_plus_common::messages::*;
use battleship_plus_common::types::*;
use bevy_quinnet::shared::ClientId;

use crate::game::data::{Game, PlayerID};
use crate::game::ship::ShipID;
use crate::game::ship_manager::ShipPlacementError;
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

#[derive(Debug, Clone)]
pub enum ActionValidationError {
    NonExistentPlayer { id: PlayerID },
    NonExistentShip { id: ShipID },
    Cooldown { remaining_rounds: u32 },
    InsufficientPoints { required: u32 },
    Unreachable,
    OutOfMap,
    InvalidShipPlacement(ShipPlacementError),
}

impl Action {
    pub(crate) fn apply_on(
        &self,
        game: &mut RwLockWriteGuard<Game>,
    ) -> Result<(), ActionExecutionError> {
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

                Ok(())
            }
            Action::SetReady { player_id, request } => {
                check_player_exists(game, *player_id)?;

                match game.players.get_mut(player_id) {
                    Some(p) => p.is_ready = request.ready_state,
                    None => panic!("player should exist"),
                }
                Ok(())
            }
            Action::PlaceShips {
                player_id,
                ship_placements,
            } => match game.validate_placement_request(*player_id, &ship_placements) {
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
            .filter(|res| res.is_err())
            .next()
            .unwrap()
            .map_err(|e| {
                ActionExecutionError::Validation(ActionValidationError::InvalidShipPlacement(e))
            }),
            Action::Move {
                ship_id,
                properties,
            } => {
                let player_id = ship_id.0;
                check_player_exists(game, player_id)?;

                let board_bounds = game.board_bounds();
                let mut player = game.players.get(&player_id).unwrap().clone();

                let trajectory = match game.ships.move_ship(
                    &mut player,
                    ship_id,
                    properties.direction(),
                    &board_bounds,
                ) {
                    Ok(trajectory) => trajectory,
                    Err(e) => return Err(ActionExecutionError::Validation(e)),
                };

                game.players.insert(player_id, player);

                let _ /*destroyed_ships*/ = game.ships.destroy_colliding_ships_in_envelope(&trajectory);
                // TODO Implementation: find a way to propagate destroyed ships

                Ok(())
            }
            Action::Rotate {
                ship_id,
                properties,
            } => {
                let player_id = ship_id.0;
                check_player_exists(game, player_id)?;

                let board_bounds = game.board_bounds();
                let mut player = game.players.get(&player_id).unwrap().clone();

                let trajectory = match game.ships.rotate_ship(
                    &mut player,
                    ship_id,
                    properties.direction(),
                    &board_bounds,
                ) {
                    Ok(trajectory) => trajectory,
                    Err(e) => return Err(ActionExecutionError::Validation(e)),
                };

                game.players.insert(player_id, player);

                let _ /*destroyed_ships*/ = game.ships.destroy_colliding_ships_in_envelope(&trajectory);
                // TODO Implementation: find a way to propagate destroyed ships

                Ok(())
            }
            Action::Shoot {
                ship_id,
                properties,
            } => {
                let player_id = ship_id.0;
                check_player_exists(game, player_id)?;

                let target = [
                    properties.target.as_ref().unwrap().x as i32,
                    properties.target.as_ref().unwrap().y as i32,
                ];

                let bounds = game.board_bounds();
                let mut player = game.players.get(&player_id).unwrap().clone();

                match game
                    .ships
                    .attack_with_ship(&mut player, ship_id, &target, &bounds)
                {
                    Ok(_) => {
                        game.players
                            .insert(player_id, player)
                            .expect("unable to update player");
                        Ok(())
                    }
                    Err(e) => Err(ActionExecutionError::Validation(e)),
                }
            }
            // TODO Implementation: Action::ScoutPlane { .. } => {}
            // TODO Implementation: Action::PredatorMissile { .. } => {}
            // TODO Implementation: Action::EngineBoost { .. } => {}
            // TODO Implementation: Action::Torpedo { .. } => {}
            // TODO Implementation: Action::MultiMissile { .. } => {}
            Action::None => Ok(()),
            _ => todo!(),
        }

        // TODO Implementation: find a good way to return Action Results
    }
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
