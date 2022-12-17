use std::sync::Arc;

use log::{debug, error};
use tokio::sync::RwLock;

use battleship_plus_common::messages::*;

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
        player_id: PlayerID,
        request: MoveRequest,
    },
    Rotate {
        player_id: PlayerID,
        request: RotateRequest,
    },
    Shoot {
        player_id: PlayerID,
        request: ShootRequest,
    },
    ScoutPlane {
        player_id: PlayerID,
        request: ScoutPlaneRequest,
    },
    PredatorMissile {
        player_id: PlayerID,
        request: PredatorMissileRequest,
    },
    EngineBoost {
        player_id: PlayerID,
        request: EngineBoostRequest,
    },
    Torpedo {
        player_id: PlayerID,
        request: TorpedoRequest,
    },
    MultiMissile {
        player_id: PlayerID,
        request: MultiMissileAttackRequest,
    },
}

#[derive(Debug, Clone)]
pub enum ActionExecutionError {
    Validation(ActionValidationError),
    OutOfState(GameState),
    Illegal(String),
    InconsistentState(String),
    ActionNotPossible(String),
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
    pub(crate) async fn apply_on(
        &self,
        game: Arc<RwLock<Game>>,
    ) -> Result<(), ActionExecutionError> {
        // TODO: implement actions below
        // TODO: add tests for all actions
        // TODO: refactor :3

        match self {
            Action::TeamSwitch { player_id } => {
                check_player_exists(&game, *player_id).await?;

                mutate_game(&game, move |g| {
                    match (g.team_a.remove(player_id), g.team_b.remove(player_id)) {
                        (true, false) => g.team_b.insert(*player_id),
                        (false, true) => g.team_a.insert(*player_id),
                        _ => {
                            let msg =
                                format!("found illegal team assignment for player {}", player_id);
                            error!("{}", msg.as_str());
                            return Err(ActionExecutionError::InconsistentState(msg));
                        }
                    };

                    Ok(())
                })
                .await
            }
            Action::SetReady { player_id, request } => {
                check_player_exists(&game, *player_id).await?;

                mutate_game(&game, move |g| {
                    match g.players.get_mut(player_id) {
                        Some(p) => p.is_ready = request.ready_state,
                        None => panic!("player should exist"),
                    }
                    Ok(())
                })
                .await
            }
            // TODO: Action::PlaceShips { .. } => {}
            Action::Move { player_id, request } => {
                check_player_exists(&game, *player_id).await?;
                let ship_id = (*player_id, request.ship_number);

                mutate_game(&game, |g| {
                    let board_bounds = g.board_bounds();
                    let player = g.players.get_mut(player_id).unwrap();

                    let trajectory = match g.ships.move_ship(
                        player,
                        &ship_id,
                        request.direction(),
                        &board_bounds,
                    ) {
                        Ok(trajectory) => trajectory,
                        Err(e) => return Err(ActionExecutionError::Validation(e)),
                    };

                    let _ /*destroyed_ships*/ = g.ships.destroy_colliding_ships_in_envelope(&trajectory);
                    // TODO: find a way to propagate destroyed ships 

                    Ok(())
                })
                .await
            }
            // TODO: Action::Rotate { .. } => {}
            Action::Shoot { player_id, request } => {
                check_player_exists(&game, *player_id).await?;
                let ship_id = (*player_id, request.ship_number);

                let target = [
                    request.target.as_ref().unwrap().x as i32,
                    request.target.as_ref().unwrap().y as i32,
                ];

                match mutate_game(&game, |g| {
                    let bounds = g.board_bounds();
                    let player = g.players.get_mut(player_id).unwrap();

                    g.ships.attack_with_ship(player, &ship_id, &target, &bounds)
                })
                .await
                {
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

async fn check_player_exists(
    game: &Arc<RwLock<Game>>,
    player_id: PlayerID,
) -> Result<(), ActionExecutionError> {
    read_game(game, |g| {
        if !g.players.contains_key(&player_id) {
            let msg = format!("PlayerID {} is unknown", player_id);
            debug!("{}", msg.as_str());
            Err(ActionExecutionError::Illegal(msg))
        } else {
            Ok(())
        }
    })
    .await
}

async fn mutate_game<T, F>(game: &Arc<RwLock<Game>>, mutation: F) -> T
where
    F: FnOnce(&mut Game) -> T,
{
    let mut g = game.write().await;
    (mutation)(&mut g)
}

async fn read_game<T, F>(game: &Arc<RwLock<Game>>, read: F) -> T
where
    F: FnOnce(&Game) -> T,
{
    let g = game.read().await;
    (read)(&g)
}
