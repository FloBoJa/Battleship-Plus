use log::{debug, error};
use tokio::sync::RwLockWriteGuard;

use battleship_plus_common::messages::*;
use battleship_plus_common::types::*;

use crate::game::data::{Game, PlayerID};
use crate::game::ship::ShipID;
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
        request: SetPlacementRequest,
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
}

impl Action {
    pub(crate) fn apply_on(
        &self,
        mut game: RwLockWriteGuard<Game>,
    ) -> Result<(), ActionExecutionError> {
        // TODO: implement actions below
        // TODO: add tests for all actions
        // TODO: refactor :3

        match self {
            Action::TeamSwitch { player_id } => {
                check_player_exists(&game, *player_id)?;

                match (game.team_a.remove(player_id), game.team_b.remove(player_id)) {
                    (true, false) => game.team_b.insert(*player_id),
                    (false, true) => game.team_a.insert(*player_id),
                    _ => {
                        let msg =
                            format!("found illegal team assignment for player {}", player_id);
                        error!("{}", msg.as_str());
                        return Err(ActionExecutionError::InconsistentState(msg));
                    }
                };

                Ok(())
            }
            Action::SetReady { player_id, request } => {
                check_player_exists(&game, *player_id)?;

                match game.players.get_mut(player_id) {
                    Some(p) => p.is_ready = request.ready_state,
                    None => panic!("player should exist"),
                }
                Ok(())
            }
            // TODO: Action::PlaceShips { .. } => {}
            Action::Move { ship_id, properties } => {
                let player_id = ship_id.0;
                check_player_exists(&game, player_id)?;

                let board_bounds = game.board_bounds();
                let mut player = game.players.get(&player_id).unwrap().clone();

                let trajectory = match game.ships.move_ship(
                    &mut player,
                    &ship_id,
                    properties.direction(),
                    &board_bounds,
                ) {
                    Ok(trajectory) => trajectory,
                    Err(e) => return Err(ActionExecutionError::Validation(e)),
                };

                game.players.insert(player_id, player);

                let _ /*destroyed_ships*/ = game.ships.destroy_colliding_ships_in_envelope(&trajectory);
                // TODO: find a way to propagate destroyed ships

                Ok(())
            }
            // TODO: Action::Rotate { .. } => {}
            Action::Shoot { ship_id, properties } => {
                let player_id = ship_id.0;
                check_player_exists(&game, player_id)?;

                let target = [
                    properties.target.as_ref().unwrap().x as i32,
                    properties.target.as_ref().unwrap().y as i32,
                ];

                let bounds = game.board_bounds();
                let player = game.players.get_mut(&player_id).unwrap();

                match game.ships.attack_with_ship(player, ship_id, &target, &bounds) {
                    Ok(_) => {
                        // TODO: find a way to propagate this result to the caller
                        Ok(())
                    }
                    Err(e) => Err(ActionExecutionError::Validation(e)),
                }
            }
            // TODO: Action::ScoutPlane { .. } => {}
            // TODO: Action::PredatorMissile { .. } => {}
            // TODO: Action::EngineBoost { .. } => {}
            // TODO: Action::Torpedo { .. } => {}
            // TODO: Action::MultiMissile { .. } => {}
            _ => todo!(),
        }

        // TODO: find a good way to return Action Results
    }
}

fn check_player_exists(
    game: &Game,
    id: PlayerID,
) -> Result<(), ActionExecutionError> {
    if !game.players.contains_key(&id) {
        let msg = format!("PlayerID {} is unknown", id);
        debug!("{}", msg.as_str());
        Err(ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }))
    } else {
        Ok(())
    }
}
