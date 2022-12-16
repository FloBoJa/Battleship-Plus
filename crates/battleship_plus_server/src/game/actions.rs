use std::sync::Arc;

use log::{debug, error};
use rstar::{AABB, Envelope, PointDistance, RTreeObject};
use tokio::sync::RwLock;

use battleship_plus_common::messages::*;

use crate::game::data::{Cooldown, Game, PlayerID, SelectShipsByIDFunction, Ship, ShipID};
use crate::game::states::GameState;

#[derive(Debug, Clone)]
pub enum Action {
    // Lobby actions
    TeamSwitch { player_id: PlayerID },
    SetReady { player_id: PlayerID, request: SetReadyStateRequest },

    // Preparation actions
    PlaceShips { player_id: PlayerID, request: SetPlacementRequest },

    // Game actions
    Move { player_id: PlayerID, request: MoveRequest },
    Rotate { player_id: PlayerID, request: RotateRequest },
    Shoot { player_id: PlayerID, request: ShootRequest },
    ScoutPlane { player_id: PlayerID, request: ScoutPlaneRequest },
    PredatorMissile { player_id: PlayerID, request: PredatorMissileRequest },
    EngineBoost { player_id: PlayerID, request: EngineBoostRequest },
    Torpedo { player_id: PlayerID, request: TorpedoRequest },
    MultiMissile { player_id: PlayerID, request: MultiMissileAttackRequest },
}

#[derive(Debug, Clone)]
pub enum ActionExecutionError {
    OutOfState(GameState),
    Illegal(String),
    InconsistentState(String),
    ActionNotAllowed(String),
}

impl Action {
    pub(crate) async fn apply_on(&self, game: Arc<RwLock<Game>>) -> Result<(), ActionExecutionError> {
        // TODO: implement actions below
        // TODO: add tests for all actions
        // TODO: refactor :3

        match self {
            Action::TeamSwitch { player_id } => {
                let err = check_player_exists(&game, *player_id).await;
                if err.is_err() {
                    return err;
                }

                mutate_game(&game, move |g| {
                    match (g.team_a.remove(&player_id), g.team_b.remove(player_id)) {
                        (true, false) => g.team_b.insert(*player_id),
                        (false, true) => g.team_a.insert(*player_id),
                        _ => {
                            let msg = format!("found illegal team assignment for player {}", player_id);
                            error!("{}", msg.as_str());
                            return Err(ActionExecutionError::InconsistentState(msg));
                        }
                    };

                    Ok(())
                }).await
            }
            Action::SetReady { player_id, request } => {
                let err = check_player_exists(&game, *player_id).await;
                if err.is_err() {
                    return err;
                }

                mutate_game(&game, move |g| {
                    match g.players.get_mut(player_id) {
                        Some(p) => p.is_ready = request.ready_state,
                        None => panic!("player should exist")
                    }
                    Ok(())
                }).await
            }
            // TODO: Action::PlaceShips { .. } => {}
            // TODO: Action::Move { .. } => {}
            // TODO: Action::Rotate { .. } => {}
            Action::Shoot { player_id, request } => {
                let err = check_player_exists(&game, *player_id).await;
                if err.is_err() {
                    return err;
                }

                let target = [request.target.as_ref().unwrap().x as i32, request.target.as_ref().unwrap().y as i32];
                let err = check_target_on_board(&game, &AABB::from_point(target)).await;
                if err.is_err() {
                    return err;
                }

                let ship_id = (*player_id, request.ship_number as u32);
                let res = check_player_operates_ship(&game, ship_id).await;
                if res.is_err() {
                    return res;
                }

                mutate_game(&game, |g| {
                    let player = g.players.get(&player_id).unwrap();
                    let ship = g.ships.get(&ship_id).unwrap();
                    let ship_balancing = ship.common_balancing();

                    // shoot cooldown
                    if ship.cool_downs().iter()
                        .any(|cd| {
                            match cd {
                                Cooldown::Cannon { .. } => true,
                                _ => false,
                            }
                        }) {
                        return Err(ActionExecutionError::ActionNotAllowed(format!("the cannon of ship {} of player {} is on cooldown",
                                                                                  ship_id.1, player_id)));
                    }

                    // action points
                    if ship_balancing.shoot_costs.as_ref().unwrap().action_points > player.action_points {
                        return Err(ActionExecutionError::ActionNotAllowed(format!("player {} needs {} action points for {:?} action but has only {}",
                                                                                  player_id, &ship_balancing.shoot_costs.unwrap().action_points,
                                                                                  self, player.action_points)));
                    }

                    // target in range
                    if ship.distance_2(&target) > ship_balancing.shoot_range as i32 {
                        return Err(ActionExecutionError::ActionNotAllowed(format!("target is out of range for ship {}", ship_id.0)));
                    }

                    apply_damage(g, &AABB::from_point(target), ship_balancing.shoot_damage as u32);

                    let costs = ship_balancing.shoot_costs.unwrap();
                    g.players.get_mut(player_id).unwrap().action_points -= costs.action_points as u32;
                    if costs.cooldown > 0 {
                        g.ships.get_mut(&ship_id).unwrap().cool_downs_mut().push(
                            Cooldown::Cannon { remaining_rounds: costs.cooldown as u32 }
                        );
                    }

                    Ok(())
                }).await
            }
            // TODO: Action::ScoutPlane { .. } => {}
            // TODO: Action::PredatorMissile { .. } => {}
            // TODO: Action::EngineBoost { .. } => {}
            // TODO: Action::Torpedo { .. } => {}
            // TODO: Action::MultiMissile { .. } => {}
            _ => todo!()
        }

        // TODO: find a good way to return Action Results
    }
}

/// Deals damage to all ship in the envelope and returns all destroyed ships
fn apply_damage(game: &mut Game, target: &AABB<[i32; 2]>, damage: u32) -> Vec<Ship> {
    let destroyed_ships: Vec<_> = game.ships_geo_lookup
        .locate_in_envelope_intersecting(&target)
        .filter(|ship_ref| {
            let target_ship = game.ships.get_mut(&ship_ref.0.data().id).unwrap();
            target_ship.apply_damage(damage)
        })
        .map(|s| (s.0.data().id, s.envelope()))
        .collect();

    game.ships_geo_lookup.remove_with_selection_function(SelectShipsByIDFunction(destroyed_ships.clone()));

    destroyed_ships
        .iter().map(|(id, _)| game.ships.remove(id).unwrap())
        .collect()
}

async fn check_target_on_board(game: &Arc<RwLock<Game>>, target: &AABB<[i32; 2]>) -> Result<(), ActionExecutionError> {
    read_game(&game, |g| {
        let board: &AABB<[i32; 2]> = &AABB::from_corners([0, 0], [g.board_size as i32, g.board_size as i32]);

        if !board.contains_envelope(target) {
            let msg = format!("specified target out of bounds");
            debug!("{}", msg.as_str());
            return Err(ActionExecutionError::Illegal(msg));
        } else {
            Ok(())
        }
    }).await
}

async fn check_player_exists(game: &Arc<RwLock<Game>>, player_id: PlayerID) -> Result<(), ActionExecutionError> {
    read_game(&game, |g| {
        if !g.players.contains_key(&player_id) {
            let msg = format!("PlayerID {} is unknown", player_id);
            debug!("{}", msg.as_str());
            return Err(ActionExecutionError::Illegal(msg));
        } else {
            Ok(())
        }
    }).await
}

async fn check_player_operates_ship(game: &Arc<RwLock<Game>>, ship_id: ShipID) -> Result<(), ActionExecutionError> {
    read_game(&game, |g| {
        match g.ships.get(&ship_id) {
            None => {
                let msg = format!("Player {} does not have the ship {}", ship_id.0, ship_id.1);
                debug!("{}", msg.as_str());
                return Err(ActionExecutionError::Illegal(msg));
            }
            Some(_) => Ok(())
        }
    }).await
}

async fn mutate_game<T, F>(game: &Arc<RwLock<Game>>, mutation: F) -> T
    where F: FnOnce(&mut Game) -> T {
    let mut g = game.write().await;
    (mutation)(&mut g)
}

async fn read_game<T, F>(game: &Arc<RwLock<Game>>, read: F) -> T
    where F: FnOnce(&Game) -> T {
    let g = game.read().await;
    (read)(&g)
}
