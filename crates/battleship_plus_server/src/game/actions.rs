use std::sync::Arc;

use log::{debug, error};
use rstar::{Envelope, PointDistance, RTreeObject, AABB};
use tokio::sync::RwLock;

use battleship_plus_common::messages::*;

use crate::game::data::{
    Cooldown, Game, GetShipID, PlayerID, SelectShipsByIDFunction, Ship, ShipID, ShipRef,
};
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
    OutOfState(GameState),
    Illegal(String),
    InconsistentState(String),
    ActionNotPossible(String),
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
                check_player_operates_ship(&game, ship_id).await?;

                mutate_game(&game, |g| {
                    let board_bounds = g.board_bounds();
                    let ship = g.ships.get_mut(&ship_id).unwrap();

                    // move cooldown
                    if ship
                        .cool_downs()
                        .iter()
                        .any(|cd| matches!(cd, Cooldown::Movement { .. }))
                    {
                        return Err(ActionExecutionError::ActionNotPossible(format!(
                            "the engines of ship {} of player {} is on cooldown",
                            ship_id.1, player_id
                        )));
                    }

                    // action points
                    let ship_balancing = ship.common_balancing();
                    let point_costs = ship_balancing
                        .movement_costs
                        .as_ref()
                        .unwrap()
                        .action_points;
                    let player = g.players.get_mut(player_id).unwrap();
                    if point_costs > player.action_points {
                        return Err(ActionExecutionError::ActionNotPossible(format!(
                            "player {} needs {} action points for {:?} action but has only {}",
                            player_id, &point_costs, self, player.action_points
                        )));
                    }

                    let new_position = ship.do_move(request.direction(), board_bounds);
                    if new_position.is_err() {
                        return Err(ActionExecutionError::Illegal(format!(
                            "player {} tried an invalid move with ship {}",
                            player_id, ship_id.1,
                        )));
                    }
                    let new_position = new_position.unwrap();

                    g.ships_geo_lookup
                        .remove_with_selection_function(SelectShipsByIDFunction(vec![(
                            ship.id(),
                            new_position,
                        )]));

                    let new_ship = ship.clone();
                    g.ships_geo_lookup.insert(ShipRef(Arc::from(s)));
                    g.ships.insert(ship_id, s);

                    player.action_points -= point_costs;

                    let colliding_ships: Vec<_> = g
                        .ships_geo_lookup
                        .locate_in_envelope_intersecting(&new_position)
                        .cloned()
                        .collect();

                    if colliding_ships.len() > 1 {
                        // remove colliding ships
                        g.ships_geo_lookup
                            .remove_with_selection_function(SelectShipsByIDFunction(
                                colliding_ships
                                    .iter()
                                    .map(|s| (s.0.data().id, s.envelope()))
                                    .collect(),
                            ));

                        colliding_ships.iter().for_each(|s| {
                            g.ships.remove(&s.id());
                        });
                    }

                    Ok(())
                })
                .await
            }
            // TODO: Action::Rotate { .. } => {}
            Action::Shoot { player_id, request } => {
                check_player_exists(&game, *player_id).await?;

                let target = [
                    request.target.as_ref().unwrap().x as i32,
                    request.target.as_ref().unwrap().y as i32,
                ];
                check_target_on_board(&game, &AABB::from_point(target)).await?;

                let ship_id = (*player_id, request.ship_number);
                check_player_operates_ship(&game, ship_id).await?;

                mutate_game(&game, |g| {
                    let player = g.players.get(player_id).unwrap();
                    let ship = g.ships.get(&ship_id).unwrap();
                    let ship_balancing = ship.common_balancing();

                    // shoot cooldown
                    if ship
                        .cool_downs()
                        .iter()
                        .any(|cd| matches!(cd, Cooldown::Cannon { .. }))
                    {
                        return Err(ActionExecutionError::ActionNotPossible(format!(
                            "the cannon of ship {} of player {} is on cooldown",
                            ship_id.1, player_id
                        )));
                    }

                    // action points
                    if ship_balancing.shoot_costs.as_ref().unwrap().action_points
                        > player.action_points
                    {
                        return Err(ActionExecutionError::ActionNotPossible(format!(
                            "player {} needs {} action points for {:?} action but has only {}",
                            player_id,
                            &ship_balancing.shoot_costs.unwrap().action_points,
                            self,
                            player.action_points
                        )));
                    }

                    // target in range
                    if ship.distance_2(&target) > ship_balancing.shoot_range as i32 {
                        return Err(ActionExecutionError::ActionNotPossible(format!(
                            "target is out of range for ship {}",
                            ship_id.0
                        )));
                    }

                    apply_damage(g, &AABB::from_point(target), ship_balancing.shoot_damage);

                    let costs = ship_balancing.shoot_costs.unwrap();
                    g.players.get_mut(player_id).unwrap().action_points -= costs.action_points;
                    if costs.cooldown > 0 {
                        g.ships.get_mut(&ship_id).unwrap().cool_downs_mut().push(
                            Cooldown::Cannon {
                                remaining_rounds: costs.cooldown,
                            },
                        );
                    }

                    Ok(())
                })
                .await
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

/// Deals damage to all ship in the envelope and returns all destroyed ships
fn apply_damage(game: &mut Game, target: &AABB<[i32; 2]>, damage: u32) -> Vec<Ship> {
    let destroyed_ships: Vec<_> = game
        .ships_geo_lookup
        .locate_in_envelope_intersecting(target)
        .filter(|ship_ref| {
            let target_ship = game.ships.get_mut(&ship_ref.0.data().id).unwrap();
            target_ship.apply_damage(damage)
        })
        .map(|s| (s.0.data().id, s.envelope()))
        .collect();

    game.ships_geo_lookup
        .remove_with_selection_function(SelectShipsByIDFunction(destroyed_ships.clone()));

    destroyed_ships
        .iter()
        .map(|(id, _)| game.ships.remove(id).unwrap())
        .collect()
}

async fn check_target_on_board(
    game: &Arc<RwLock<Game>>,
    target: &AABB<[i32; 2]>,
) -> Result<(), ActionExecutionError> {
    read_game(game, |g| {
        let board: &AABB<[i32; 2]> =
            &AABB::from_corners([0, 0], [g.board_size as i32, g.board_size as i32]);

        if !board.contains_envelope(target) {
            let msg = "specified target out of bounds";
            debug!("{}", msg);
            Err(ActionExecutionError::Illegal(String::from(msg)))
        } else {
            Ok(())
        }
    })
    .await
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

async fn check_player_operates_ship(
    game: &Arc<RwLock<Game>>,
    ship_id: ShipID,
) -> Result<(), ActionExecutionError> {
    read_game(game, |g| match g.ships.get(&ship_id) {
        None => {
            let msg = format!(
                "Player {} does not control the ship {}",
                ship_id.0, ship_id.1
            );
            debug!("{}", msg.as_str());
            Err(ActionExecutionError::Illegal(msg))
        }
        Some(_) => Ok(()),
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
