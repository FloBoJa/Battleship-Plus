//noinspection DuplicatedCode
mod actions_team_switch {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::{ActionValidationError, PlayerID};

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player};

    #[tokio::test]
    async fn actions_team_switch() {
        let player_id: PlayerID = 42;
        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(
                player_id,
                Player {
                    id: player_id,
                    ..Default::default()
                },
            )]),
            team_a: HashSet::from([player_id]),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // player is in team a
        {
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }

        // switch team a -> b
        assert!(matches!(
            Action::TeamSwitch { player_id }.apply_on(&mut g),
            Ok(None)
        ));
        {
            assert!(!g.team_a.contains(&player_id));
            assert!(g.team_b.contains(&player_id));
        }

        // switch team b -> a
        assert!(matches!(
            Action::TeamSwitch { player_id }.apply_on(&mut g),
            Ok(None)
        ));
        {
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }
    }

    #[tokio::test]
    async fn actions_team_switch_detect_inconsistent_state() {
        let player_id: PlayerID = 42;
        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(
                player_id,
                Player {
                    id: player_id,
                    ..Default::default()
                },
            )]),
            team_a: HashSet::from([player_id]),
            team_b: HashSet::from([player_id]),
            ..Default::default()
        }));
        let mut g = g.write().await;

        let res = Action::TeamSwitch { player_id }.apply_on(&mut g);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::InconsistentState(e) =>
                e == "found illegal team assignment for player 42",
            _ => false,
        })
    }

    #[tokio::test]
    async fn actions_team_switch_unknown_player() {
        let player_id: PlayerID = 42;
        let g = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));
        let mut g = g.write().await;

        let res = Action::TeamSwitch { player_id }.apply_on(&mut g);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }) =>
                id == player_id,
            _ => false,
        })
    }
}

//noinspection DuplicatedCode
mod actions_player_set_ready_state {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::{ActionValidationError, PlayerID};
    use battleship_plus_common::messages::SetReadyStateRequest;

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player};

    #[tokio::test]
    async fn actions_player_set_ready() {
        let player_id: PlayerID = 42;
        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(
                player_id,
                Player {
                    id: player_id,
                    ..Default::default()
                },
            )]),
            team_a: HashSet::from([player_id]),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // set player ready
        assert!(matches!(
            Action::SetReady {
                player_id,
                request: SetReadyStateRequest { ready_state: true },
            }
            .apply_on(&mut g),
            Ok(None)
        ));
        {
            assert!(g.players.get(&player_id).unwrap().is_ready);
        }

        // set player not ready
        assert!(matches!(
            Action::SetReady {
                player_id,
                request: SetReadyStateRequest { ready_state: false },
            }
            .apply_on(&mut g),
            Ok(None)
        ));
        {
            assert!(!g.players.get(&player_id).unwrap().is_ready);
        }
    }

    #[tokio::test]
    async fn actions_set_ready_unknown_player() {
        let player_id: PlayerID = 42;
        let g = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));
        let mut g = g.write().await;

        let res = Action::SetReady {
            player_id,
            request: SetReadyStateRequest { ready_state: true },
        }
        .apply_on(&mut g);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }) =>
                id == player_id,
            _ => false,
        })
    }
}

//noinspection DuplicatedCode
mod actions_shoot {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::ship::{Cooldown, GetShipID, Ship, ShipData};
    use battleship_plus_common::game::ship_manager::ShipManager;
    use battleship_plus_common::game::ActionValidationError;
    use battleship_plus_common::types::*;

    use crate::game::actions::ActionResult;
    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player, Turn};

    #[tokio::test]
    async fn actions_shoot() {
        let player = Player::default();
        let ship_src = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    shoot_damage: 10,
                    shoot_range: 128,
                    shoot_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: Default::default(),
            cool_downs: Default::default(),
        };
        let ship_target1 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    ..ship_src.common_balancing()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (2, 2),
                health: 10,
                pos_x: 5,
                pos_y: 5,
                ..ship_src.data()
            },
            cool_downs: Default::default(),
        };
        let ship_target2 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    ..ship_src.common_balancing()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (2, 3),
                health: 11,
                pos_x: 10,
                pos_y: 10,
                ..ship_src.data()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![
                ship_src.clone(),
                ship_target1.clone(),
                ship_target2.clone(),
            ]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // shoot ship_target1
        let result = Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate {
                    x: ship_target1.data().pos_x as u32,
                    y: ship_target1.data().pos_y as u32,
                }),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            inflicted_damage_by_ship,
            inflicted_damage_at,
            ships_destroyed,
            lost_vision_at,
            gain_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(lost_vision_at.contains(&Coordinate { x: 5, y: 5 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 5, y: 6 }));
            assert!(gain_vision_at.is_empty());
            assert!(ships_destroyed.contains(&ship_target1.id()));
            assert!(inflicted_damage_by_ship.contains_key(&ship_target1.id()));
            assert!(inflicted_damage_at.contains_key(&Coordinate {
                x: ship_target1.data().pos_x as u32,
                y: ship_target1.data().pos_y as u32,
            }));
            assert!(temp_vision_at.is_empty());
        }

        // check ship_target1 destroyed and ship_target2 untouched
        {
            assert!(g.ships.get_by_id(&ship_target1.id()).is_none());
            assert!(g.ships.get_by_id(&ship_target2.id()).is_some());
            assert_eq!(
                g.ships.get_by_id(&ship_target2.id()).unwrap().data().health,
                11
            );
        }

        // shoot ship_target2
        let result = Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate {
                    x: ship_target2.data().pos_x as u32,
                    y: ship_target2.data().pos_y as u32,
                }),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            inflicted_damage_by_ship,
            inflicted_damage_at,
            ships_destroyed,
            lost_vision_at,
            gain_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.is_empty());
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.contains_key(&ship_target2.id()));
            assert!(inflicted_damage_at.contains_key(&Coordinate {
                x: ship_target2.data().pos_x as u32,
                y: ship_target2.data().pos_y as u32,
            }));
            assert!(temp_vision_at.is_empty());
        }

        // check ship_target1 destroyed and ship_target2 health reduced
        {
            assert!(g.ships.get_by_id(&ship_target1.id()).is_none());
            assert!(g.ships.get_by_id(&ship_target2.id()).is_some());
            assert_eq!(
                g.ships.get_by_id(&ship_target2.id()).unwrap().data().health,
                1
            );
        }

        // missed shot
        assert!(matches!(
            Action::Shoot {
                ship_id: (player.id, 0),
                properties: ShootProperties {
                    target: Some(Coordinate { x: 20, y: 20 }),
                },
            }
            .apply_on(&mut g),
            Ok(None)
        ));

        // board untouched
        {
            assert!(g.ships.get_by_id(&ship_target1.id()).is_none());
            assert!(g.ships.get_by_id(&ship_target2.id()).is_some());
            assert_eq!(
                g.ships.get_by_id(&ship_target2.id()).unwrap().data().health,
                1
            );
        }
    }

    #[tokio::test]
    async fn actions_shoot_action_points() {
        let player = Player {
            ..Default::default()
        };
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    shoot_damage: 10,
                    shoot_range: 128,
                    shoot_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 4,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: Default::default(),
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship]),
            turn: Some(Turn::new(player.id, 5)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // first shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(&mut g)
        .is_ok());

        // action points reduced
        {
            assert_eq!(g.turn.as_ref().unwrap().action_points_left, 1);
        }

        // deny second shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // board untouched
        {
            assert_eq!(g.turn.as_ref().unwrap().action_points_left, 1);
        }
    }

    #[tokio::test]
    async fn actions_shoot_cooldown() {
        let player = Player {
            ..Default::default()
        };
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    shoot_damage: 10,
                    shoot_range: 128,
                    shoot_costs: Some(Costs {
                        cooldown: 2,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: Default::default(),
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id, 5)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // first shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(&mut g)
        .is_ok());

        // check cooldown
        {
            assert!(g
                .ships
                .get_by_id(&ship.id())
                .unwrap()
                .cool_downs()
                .contains(&Cooldown::Cannon {
                    remaining_rounds: 2
                }));
        }

        // deny second shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // board untouched
        {
            assert!(g
                .ships
                .get_by_id(&ship.id())
                .unwrap()
                .cool_downs()
                .contains(&Cooldown::Cannon {
                    remaining_rounds: 2
                }));
        }
    }

    #[tokio::test]
    async fn actions_shoot_unknown_player() {
        let g = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));
        let mut g = g.write().await;

        let res = Action::Shoot {
            ship_id: (42, 1),
            properties: ShootProperties {
                target: Some(Coordinate { x: 0, y: 0 }),
            },
        }
        .apply_on(&mut g);

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }) =>
                id == 42,
            _ => false,
        })
    }

    #[tokio::test]
    async fn actions_shoot_reject_shot_into_oblivion() {
        let player = Player {
            id: 42,
            ..Default::default()
        };
        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        let res = Action::Shoot {
            ship_id: (42, 1),
            properties: ShootProperties {
                target: Some(Coordinate {
                    x: 9999999,
                    y: 9999999,
                }),
            },
        }
        .apply_on(&mut g);

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(
            err,
            ActionExecutionError::Validation(ActionValidationError::OutOfMap)
        ));
    }

    #[tokio::test]
    async fn actions_shoot_not_players_turn() {
        let player = Player::default();
        let ship_src = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    shoot_damage: 10,
                    shoot_range: 128,
                    shoot_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: Default::default(),
            cool_downs: Default::default(),
        };
        let ship_target1 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    ..ship_src.common_balancing()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (2, 2),
                health: 10,
                pos_x: 5,
                pos_y: 5,
                ..ship_src.data()
            },
            cool_downs: Default::default(),
        };
        let ship_target2 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    ..ship_src.common_balancing()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (2, 3),
                health: 11,
                pos_x: 10,
                pos_y: 10,
                ..ship_src.data()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![
                ship_src.clone(),
                ship_target1.clone(),
                ship_target2.clone(),
            ]),
            turn: Some(Turn::new(player.id + 1, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // shoot ship_target1
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate {
                    x: ship_target1.data().pos_x as u32,
                    y: ship_target1.data().pos_y as u32,
                }),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // check ship_target1 and ship_target2 untouched
        {
            assert!(g.ships.get_by_id(&ship_target1.id()).is_some());
            assert_eq!(
                g.ships.get_by_id(&ship_target1.id()).unwrap().data().health,
                10
            );
            assert!(g.ships.get_by_id(&ship_target2.id()).is_some());
            assert_eq!(
                g.ships.get_by_id(&ship_target2.id()).unwrap().data().health,
                11
            );
        }
    }
}

//noinspection DuplicatedCode
mod actions_move {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::ship::{Cooldown, GetShipID, Orientation, Ship, ShipData};
    use battleship_plus_common::game::ship_manager::ShipManager;
    use battleship_plus_common::game::ActionValidationError;
    use battleship_plus_common::types::*;

    use crate::config_provider::default_config_provider;
    use crate::game::actions::{Action, ActionExecutionError, ActionResult};
    use crate::game::data::{Game, Player, Turn};

    #[tokio::test]
    async fn actions_move() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    vision_range: 2,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // move ship forward
        let result = Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.is_empty());
        }

        // check ship's new position
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1))
        }

        // move ship backward
        let result = Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.is_empty());
        }

        // check ship's new position
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 0))
        }
    }

    #[tokio::test]
    async fn actions_move_vision() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    vision_range: 2,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (player.id, 0),
                pos_x: 10,
                pos_y: 10,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let ship1 = Ship::Destroyer {
            data: ShipData {
                id: (player.id, 1),
                pos_x: 10,
                pos_y: 7,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
            balancing: Default::default(),
        };
        let ship2 = Ship::Destroyer {
            data: ShipData {
                id: (player.id, 2),
                pos_x: 10,
                pos_y: 14,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
            balancing: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone(), ship1.clone(), ship2.clone()]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // move ship forward
        let result = Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.contains(&Coordinate { x: 10, y: 8 }));
            assert!(gain_vision_at.contains(&Coordinate { x: 10, y: 14 }));
            assert!(temp_vision_at.is_empty());
        }

        // check ship's new position
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (10, 11))
        }

        // move ship forward
        let result = Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.contains(&Coordinate { x: 10, y: 15 }));
            assert!(temp_vision_at.is_empty());
        }

        // check ship's new position
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (10, 12))
        }
    }

    #[tokio::test]
    async fn actions_move_action_points() {
        let player = Player {
            ..Default::default()
        };
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 3,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id, 5)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // move ship forward
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(&mut g)
        .is_ok());

        // check action points
        {
            assert_eq!(g.turn.as_ref().unwrap().action_points_left, 2);
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1));
        }

        // try to move ship backwards and fail
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // check board untouched
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1));
            assert_eq!(g.turn.as_ref().unwrap().action_points_left, 2);
        }
    }

    #[tokio::test]
    async fn actions_move_cooldown() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 2,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // move ship forward
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(&mut g)
        .is_ok());

        // check ship's new position and cooldown
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1));
            assert!(!g
                .ships
                .get_by_id(&ship.id())
                .unwrap()
                .cool_downs()
                .is_empty());
            assert!(matches!(
                g.ships
                    .get_by_id(&ship.id())
                    .unwrap()
                    .cool_downs()
                    .first()
                    .unwrap(),
                Cooldown::Movement { .. }
            ));
        }

        // try to move ship backwards and fail
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // check board untouched
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1));
            assert!(!g
                .ships
                .get_by_id(&ship.id())
                .unwrap()
                .cool_downs()
                .is_empty());
            assert_eq!(
                g.ships
                    .get_by_id(&ship.id())
                    .unwrap()
                    .cool_downs()
                    .first()
                    .unwrap()
                    .clone(),
                Cooldown::Movement {
                    remaining_rounds: 2
                }
            );
        }
    }

    #[tokio::test]
    async fn actions_move_unknown_player() {
        let g = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));
        let mut g = g.write().await;

        let res = Action::Move {
            ship_id: (42, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(&mut g);

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }) =>
                id == 42,
            _ => false,
        })
    }

    #[tokio::test]
    async fn actions_move_deny_out_of_bounds() {
        let config = default_config_provider().game_config();

        let player = Player::default();
        let ship1 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (player.id, 0),
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let ship2 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (player.id, 1),
                pos_x: (config.board_size - 1) as i32,
                pos_y: (config.board_size - 1) as i32,
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship1, ship2]),
            turn: Some(Turn::new(player.id, 0)),
            config,
            ..Default::default()
        }));
        let mut g = g.write().await;

        // move ship1 backwards
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // move ship2 backward
        assert!(Action::Move {
            ship_id: (player.id, 1),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(&mut g)
        .is_err());
    }

    #[tokio::test]
    async fn actions_move_destroy_on_collision() {
        let player = Player::default();
        let ship1 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 10,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let ship2 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (0, 1),
                pos_x: 0,
                pos_y: 11,
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship1.clone(), ship2.clone()]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // move ship1 backwards into ship2
        let result = Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            inflicted_damage_by_ship,
            inflicted_damage_at,
            ships_destroyed,
            lost_vision_at,
            gain_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(lost_vision_at.contains(&Coordinate { x: 0, y: 10 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 0, y: 11 }));
            assert!(gain_vision_at.is_empty());
            assert!(ships_destroyed.contains(&ship1.id()));
            assert!(ships_destroyed.contains(&ship2.id()));
            assert!(inflicted_damage_by_ship.contains_key(&ship1.id()));
            assert!(inflicted_damage_by_ship.contains_key(&ship2.id()));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 0, y: 11 }));
            assert!(temp_vision_at.is_empty());
        }

        // check both ships destroyed
        {
            assert!(g.ships.get_by_id(&ship1.id()).is_none());
            assert!(g.ships.get_by_id(&ship2.id()).is_none());
        }
    }

    #[tokio::test]
    async fn actions_move_not_players_turn() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id + 1, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // move ship forward
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // check ship's position
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 0))
        }
    }
}

//noinspection DuplicatedCode
mod actions_rotate {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::ship::{Cooldown, GetShipID, Orientation, Ship, ShipData};
    use battleship_plus_common::game::ship_manager::ShipManager;
    use battleship_plus_common::game::ActionValidationError;
    use battleship_plus_common::types::*;

    use crate::game::actions::{Action, ActionExecutionError, ActionResult};
    use crate::game::data::{Game, Player, Turn};

    #[tokio::test]
    async fn actions_rotate() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 10,
                pos_y: 10,
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut game = g.write().await;

        let mut rotate: Box<dyn FnMut(&mut Game, RotateDirection)> = Box::new(|game, d| {
            // rotate ship counter clockwise
            assert!(Action::Rotate {
                ship_id: (player.id, 0),
                properties: RotateProperties {
                    direction: i32::from(d),
                },
            }
            .apply_on(game)
            .is_ok());
        });

        (rotate)(&mut game, RotateDirection::CounterClockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::East
        );
        (rotate)(&mut game, RotateDirection::CounterClockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::North
        );
        (rotate)(&mut game, RotateDirection::CounterClockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::West
        );
        (rotate)(&mut game, RotateDirection::CounterClockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::South
        );

        (rotate)(&mut game, RotateDirection::Clockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::West
        );
        (rotate)(&mut game, RotateDirection::Clockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::North
        );
        (rotate)(&mut game, RotateDirection::Clockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::East
        );
        (rotate)(&mut game, RotateDirection::Clockwise);
        assert_eq!(
            game.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::South
        );
    }

    #[tokio::test]
    async fn actions_rotate_action_points() {
        let player = Player {
            ..Default::default()
        };
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 3,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::East,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id, 5)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // rotate ship
        assert!(Action::Rotate {
            ship_id: (player.id, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::CounterClockwise),
            },
        }
        .apply_on(&mut g)
        .is_ok());

        // check action points
        {
            assert_eq!(g.turn.as_ref().unwrap().action_points_left, 2);
            assert_eq!(
                g.ships.get_by_id(&ship.id()).unwrap().orientation(),
                Orientation::North
            );
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 0));
        }

        // try to rotate ship back and fail
        assert!(Action::Rotate {
            ship_id: (player.id, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::Clockwise),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // check board untouched
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 0));
            assert_eq!(
                g.ships.get_by_id(&ship.id()).unwrap().orientation(),
                Orientation::North
            );
            assert_eq!(g.turn.as_ref().unwrap().action_points_left, 2);
        }
    }

    #[tokio::test]
    async fn actions_rotate_cooldown() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 2,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::East,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // rotate ship
        assert!(Action::Rotate {
            ship_id: (player.id, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::CounterClockwise),
            },
        }
        .apply_on(&mut g)
        .is_ok());

        // check ship's new rotation and cooldown
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 0));
            assert_eq!(
                g.ships.get_by_id(&ship.id()).unwrap().orientation(),
                Orientation::North
            );
            assert!(!g
                .ships
                .get_by_id(&ship.id())
                .unwrap()
                .cool_downs()
                .is_empty());
            assert!(matches!(
                g.ships
                    .get_by_id(&ship.id())
                    .unwrap()
                    .cool_downs()
                    .first()
                    .unwrap(),
                Cooldown::Movement { .. }
            ));
        }

        // try to rotate ship back and fail
        assert!(Action::Rotate {
            ship_id: (player.id, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::Clockwise),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // check board untouched
        {
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 0));
            assert_eq!(
                g.ships.get_by_id(&ship.id()).unwrap().orientation(),
                Orientation::North
            );
            assert!(!g
                .ships
                .get_by_id(&ship.id())
                .unwrap()
                .cool_downs()
                .is_empty());
            assert_eq!(
                g.ships
                    .get_by_id(&ship.id())
                    .unwrap()
                    .cool_downs()
                    .first()
                    .unwrap()
                    .clone(),
                Cooldown::Movement {
                    remaining_rounds: 2
                }
            );
        }
    }

    #[tokio::test]
    async fn actions_rotate_unknown_player() {
        let g = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));
        let mut g = g.write().await;

        let res = Action::Rotate {
            ship_id: (42, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::CounterClockwise),
            },
        }
        .apply_on(&mut g);

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }) =>
                id == 42,
            _ => false,
        })
    }

    #[tokio::test]
    async fn actions_rotate_deny_out_of_bounds() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::East,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // rotate ship out of bounds
        assert!(Action::Rotate {
            ship_id: (player.id, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::Clockwise),
            },
        }
        .apply_on(&mut g)
        .is_err());
    }

    #[tokio::test]
    async fn actions_rotate_destroy_on_collision() {
        let player = Player::default();
        let rotating_ship = Ship::Carrier {
            balancing: Arc::from(CarrierBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    vision_range: 2,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let ship_to_be_destroyed = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (0, 1),
                pos_x: 2,
                pos_y: 0,
                orientation: Orientation::East,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let ship_to_stay_intact = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                id: (0, 2),
                pos_x: 1,
                pos_y: 1,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![
                rotating_ship.clone(),
                ship_to_be_destroyed.clone(),
                ship_to_stay_intact.clone(),
            ]),
            turn: Some(Turn::new(player.id, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        // rotate ship
        let result = Action::Rotate {
            ship_id: (player.id, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::Clockwise),
            },
        }
        .apply_on(&mut g);
        if let Ok(Some(ActionResult {
            inflicted_damage_by_ship,
            inflicted_damage_at,
            ships_destroyed,
            lost_vision_at,
            gain_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(lost_vision_at.contains(&Coordinate { x: 2, y: 0 }));
            assert!(gain_vision_at.is_empty());
            assert!(ships_destroyed.contains(&rotating_ship.id()));
            assert!(ships_destroyed.contains(&ship_to_be_destroyed.id()));
            assert!(inflicted_damage_by_ship.contains_key(&rotating_ship.id()));
            assert!(inflicted_damage_by_ship.contains_key(&ship_to_be_destroyed.id()));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 2, y: 0 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 3, y: 0 }));
            assert!(temp_vision_at.is_empty());
        }

        // check results
        {
            assert!(g.ships.get_by_id(&rotating_ship.id()).is_none());
            assert!(g.ships.get_by_id(&ship_to_be_destroyed.id()).is_none());
            assert!(g.ships.get_by_id(&ship_to_stay_intact.id()).is_some());
        }
    }

    #[tokio::test]
    async fn actions_rotate_not_players_turn() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_costs: Some(Costs {
                        cooldown: 0,
                        action_points: 0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 10,
                pos_y: 10,
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            turn: Some(Turn::new(player.id + 1, 0)),
            ..Default::default()
        }));
        let mut g = g.write().await;

        assert!(Action::Rotate {
            ship_id: (player.id, 0),
            properties: RotateProperties {
                direction: i32::from(RotateDirection::Clockwise),
            },
        }
        .apply_on(&mut g)
        .is_err());

        // check for no rotation
        assert_eq!(
            g.ships.get_by_id(&ship.id()).unwrap().orientation(),
            Orientation::South
        );
    }
}

//noinspection DuplicatedCode
mod actions_place_ships {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::ship::{GetShipID, Orientation, Ship};
    use battleship_plus_common::types::*;

    use crate::game::actions::Action;
    use crate::game::data::{Game, Player};
    use crate::game::states::GameState;

    #[tokio::test]
    async fn actions_place_ships() {
        let player = Player::default();

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ..Default::default()
        }));
        let mut g = g.write().await;
        g.state = GameState::Preparation;
        g.players.get_mut(&player.id).unwrap().quadrant = g.quadrants().first().cloned();
        let player = g.players.get(&player.id).unwrap().clone();

        let ship_positions_orientation = vec![
            (0, 0, Orientation::East),  // Carrier
            (0, 1, Orientation::East),  // Battleship
            (0, 2, Orientation::East),  // Battleship
            (0, 3, Orientation::East),  // Cruiser
            (0, 4, Orientation::East),  // Cruiser
            (0, 5, Orientation::East),  // Cruiser
            (0, 6, Orientation::East),  // Submarine
            (0, 7, Orientation::East),  // Submarine
            (0, 8, Orientation::East),  // Submarine
            (0, 9, Orientation::East),  // Submarine
            (0, 10, Orientation::East), // Destroyer
            (0, 11, Orientation::East), // Destroyer
        ];

        let ships_to_be_placed: Vec<_> = g
            .config
            .ship_set_team_a
            .iter()
            .enumerate()
            .map(|(ship_number, &ship_id)| (ship_number, ShipType::from_i32(ship_id).unwrap()))
            .zip(ship_positions_orientation)
            .map(|((ship_number, ship_type), (x, y, orientation))| {
                Ship::new_from_type(
                    ship_type,
                    (player.id, ship_number as u32),
                    (x, y),
                    orientation,
                    g.config.clone(),
                )
            })
            .collect();

        let ship_assignments: Vec<_> = ships_to_be_placed
            .iter()
            .map(|ship| ShipAssignment {
                coordinate: Some(Coordinate {
                    x: ship.position().0 as u32,
                    y: ship.position().1 as u32,
                }),
                direction: <Orientation as Into<Direction>>::into(ship.orientation()) as i32,
            })
            .collect();

        // place ships
        assert!(Action::PlaceShips {
            player_id: player.id,
            ship_placements: ship_assignments,
        }
        .apply_on(&mut g)
        .is_ok());

        // check game
        ships_to_be_placed.iter().for_each(|ship_expected| {
            let ship_actual = g.ships.get_by_id(&ship_expected.id());
            assert!(ship_actual.is_some());
            let ship_actual = ship_actual.unwrap();

            assert_eq!(ship_actual, ship_expected);
        })
    }

    #[tokio::test]
    async fn actions_place_ships_outside_quadrant() {
        let player = Player::default();

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ..Default::default()
        }));
        let mut g = g.write().await;
        g.state = GameState::Preparation;
        g.players.get_mut(&player.id).unwrap().quadrant = g.quadrants().first().cloned();
        let player = g.players.get(&player.id).unwrap().clone();

        let ship_positions_orientation = vec![
            (0, 0, Orientation::East),  // Carrier
            (0, 1, Orientation::East),  // Battleship
            (0, 2, Orientation::East),  // Battleship
            (0, 3, Orientation::East),  // Cruiser
            (0, 4, Orientation::East),  // Cruiser
            (64, 5, Orientation::East), // Cruiser
            (0, 6, Orientation::East),  // Submarine
            (0, 7, Orientation::East),  // Submarine
            (0, 8, Orientation::East),  // Submarine
            (0, 9, Orientation::East),  // Submarine
            (0, 10, Orientation::East), // Destroyer
            (0, 11, Orientation::East), // Destroyer
        ];

        let ships_to_be_placed: Vec<_> = g
            .config
            .ship_set_team_a
            .iter()
            .enumerate()
            .map(|(ship_number, &ship_id)| (ship_number, ShipType::from_i32(ship_id).unwrap()))
            .zip(ship_positions_orientation)
            .map(|((ship_number, ship_type), (x, y, orientation))| {
                Ship::new_from_type(
                    ship_type,
                    (player.id, ship_number as u32),
                    (x, y),
                    orientation,
                    g.config.clone(),
                )
            })
            .collect();

        let ship_assignments: Vec<_> = ships_to_be_placed
            .iter()
            .map(|ship| ShipAssignment {
                coordinate: Some(Coordinate {
                    x: ship.position().0 as u32,
                    y: ship.position().1 as u32,
                }),
                direction: <Orientation as Into<Direction>>::into(ship.orientation()) as i32,
            })
            .collect();

        // place ships
        assert!(Action::PlaceShips {
            player_id: player.id,
            ship_placements: ship_assignments,
        }
        .apply_on(&mut g)
        .is_err());

        // check game
        ships_to_be_placed.iter().for_each(|ship_expected| {
            let ship_actual = g.ships.get_by_id(&ship_expected.id());
            assert!(ship_actual.is_none());
        })
    }

    #[tokio::test]
    async fn actions_place_ships_wrong_ship_set_missing_ships() {
        let player = Player::default();

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ..Default::default()
        }));
        let mut g = g.write().await;
        g.state = GameState::Preparation;
        g.players.get_mut(&player.id).unwrap().quadrant = g.quadrants().first().cloned();
        let player = g.players.get(&player.id).unwrap().clone();

        let ship_positions_orientation = vec![
            (0, 0, Orientation::East), // Carrier
            (0, 1, Orientation::East), // Battleship
            (0, 2, Orientation::East), // Battleship
            (0, 3, Orientation::East), // Cruiser
            (0, 4, Orientation::East), // Cruiser
            (0, 5, Orientation::East), // Cruiser
            (0, 6, Orientation::East), // Submarine
            (0, 7, Orientation::East), // Submarine
        ];
        /*
            missing: (0, 8, Orientation::East),  // Submarine
            missing: (0, 9, Orientation::East),  // Submarine
            missing: (0, 10, Orientation::East), // Destroyer
            missing: (0, 11, Orientation::East), // Destroyer
        */

        let ships_to_be_placed: Vec<_> = g
            .config
            .ship_set_team_a
            .iter()
            .enumerate()
            .map(|(ship_number, &ship_id)| (ship_number, ShipType::from_i32(ship_id).unwrap()))
            .zip(ship_positions_orientation)
            .map(|((ship_number, ship_type), (x, y, orientation))| {
                Ship::new_from_type(
                    ship_type,
                    (player.id, ship_number as u32),
                    (x, y),
                    orientation,
                    g.config.clone(),
                )
            })
            .collect();

        let ship_assignments: Vec<_> = ships_to_be_placed
            .iter()
            .map(|ship| ShipAssignment {
                coordinate: Some(Coordinate {
                    x: ship.position().0 as u32,
                    y: ship.position().1 as u32,
                }),
                direction: <Orientation as Into<Direction>>::into(ship.orientation()) as i32,
            })
            .collect();

        // place ships
        assert!(Action::PlaceShips {
            player_id: player.id,
            ship_placements: ship_assignments,
        }
        .apply_on(&mut g)
        .is_err());

        // check game
        ships_to_be_placed.iter().for_each(|ship_expected| {
            let ship_actual = g.ships.get_by_id(&ship_expected.id());
            assert!(ship_actual.is_none());
        })
    }

    #[tokio::test]
    async fn actions_place_ships_wrong_ship_set_too_many_ships() {
        let player = Player::default();

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ..Default::default()
        }));
        let mut g = g.write().await;
        g.state = GameState::Preparation;
        g.players.get_mut(&player.id).unwrap().quadrant = g.quadrants().first().cloned();
        let player = g.players.get(&player.id).unwrap().clone();

        let ship_positions_orientation = vec![
            (0, 0, Orientation::East),  // Carrier
            (0, 1, Orientation::East),  // Battleship
            (0, 2, Orientation::East),  // Battleship
            (0, 3, Orientation::East),  // Cruiser
            (0, 4, Orientation::East),  // Cruiser
            (0, 5, Orientation::East),  // Cruiser
            (0, 6, Orientation::East),  // Submarine
            (0, 7, Orientation::East),  // Submarine
            (0, 8, Orientation::East),  // Submarine
            (0, 9, Orientation::East),  // Submarine
            (0, 10, Orientation::East), // Destroyer
            (0, 11, Orientation::East), // Destroyer
        ];

        let ships_to_be_placed: Vec<_> = g
            .config
            .ship_set_team_a
            .iter()
            .enumerate()
            .map(|(ship_number, &ship_id)| (ship_number, ShipType::from_i32(ship_id).unwrap()))
            .zip(ship_positions_orientation)
            .map(|((ship_number, ship_type), (x, y, orientation))| {
                Ship::new_from_type(
                    ship_type,
                    (player.id, ship_number as u32),
                    (x, y),
                    orientation,
                    g.config.clone(),
                )
            })
            .chain(vec![
                // add another Carrier
                Ship::new_from_type(
                    ShipType::Carrier,
                    (player.id, 12),
                    (0, 12),
                    Orientation::East,
                    g.config.clone(),
                ),
            ])
            .collect();

        let ship_assignments: Vec<_> = ships_to_be_placed
            .iter()
            .map(|ship| ShipAssignment {
                coordinate: Some(Coordinate {
                    x: ship.position().0 as u32,
                    y: ship.position().1 as u32,
                }),
                direction: <Orientation as Into<Direction>>::into(ship.orientation()) as i32,
            })
            .collect();

        // place ships
        assert!(Action::PlaceShips {
            player_id: player.id,
            ship_placements: ship_assignments,
        }
        .apply_on(&mut g)
        .is_err());

        // check game
        ships_to_be_placed.iter().for_each(|ship_expected| {
            let ship_actual = g.ships.get_by_id(&ship_expected.id());
            assert!(ship_actual.is_none());
        })
    }

    #[tokio::test]
    async fn actions_place_ships_colliding() {
        let player = Player::default();

        let g = Arc::new(RwLock::new(Game {
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ..Default::default()
        }));
        let mut g = g.write().await;
        g.state = GameState::Preparation;
        g.players.get_mut(&player.id).unwrap().quadrant = g.quadrants().first().cloned();
        let player = g.players.get(&player.id).unwrap().clone();

        let ship_positions_orientation = vec![
            (0, 0, Orientation::East), // Carrier
            (0, 0, Orientation::East), // Battleship
            (0, 0, Orientation::East), // Battleship
            (0, 0, Orientation::East), // Cruiser
            (0, 0, Orientation::East), // Cruiser
            (0, 0, Orientation::East), // Cruiser
            (0, 0, Orientation::East), // Submarine
            (0, 0, Orientation::East), // Submarine
            (0, 0, Orientation::East), // Submarine
            (0, 0, Orientation::East), // Submarine
            (0, 0, Orientation::East), // Destroyer
            (0, 0, Orientation::East), // Destroyer
        ];

        let ships_to_be_placed: Vec<_> = g
            .config
            .ship_set_team_a
            .iter()
            .enumerate()
            .map(|(ship_number, &ship_id)| (ship_number, ShipType::from_i32(ship_id).unwrap()))
            .zip(ship_positions_orientation)
            .map(|((ship_number, ship_type), (x, y, orientation))| {
                Ship::new_from_type(
                    ship_type,
                    (player.id, ship_number as u32),
                    (x, y),
                    orientation,
                    g.config.clone(),
                )
            })
            .collect();

        let ship_assignments: Vec<_> = ships_to_be_placed
            .iter()
            .map(|ship| ShipAssignment {
                coordinate: Some(Coordinate {
                    x: ship.position().0 as u32,
                    y: ship.position().1 as u32,
                }),
                direction: <Orientation as Into<Direction>>::into(ship.orientation()) as i32,
            })
            .collect();

        // place ships
        assert!(Action::PlaceShips {
            player_id: player.id,
            ship_placements: ship_assignments,
        }
        .apply_on(&mut g)
        .is_err());

        // check game
        ships_to_be_placed.iter().for_each(|ship_expected| {
            let ship_actual = g.ships.get_by_id(&ship_expected.id());
            assert!(ship_actual.is_none());
        })
    }
}

//noinspection DuplicatedCode
mod actions_scout_plane {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::ship::{Cooldown, Orientation, Ship, ShipData};
    use battleship_plus_common::game::ship_manager::ShipManager;
    use battleship_plus_common::types::*;

    use crate::game::actions::{Action, ActionResult};
    use crate::game::data::{Game, Player, Turn};
    use crate::game::states::GameState;

    #[tokio::test]
    async fn actions_scout_plane() {
        let player = Player::default();
        let ship_id = (player.id, 0);

        let ship = Ship::Carrier {
            balancing: Arc::from(CarrierBalancing {
                scout_plane_radius: 4,
                scout_plane_range: 10,
                common_balancing: Some(CommonBalancing {
                    ability_costs: Some(Costs {
                        cooldown: 10,
                        action_points: 10,
                    }),
                    ..Default::default()
                }),
            }),
            data: ShipData {
                id: ship_id,
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let scouted = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 42),
                health: 10,
                pos_x: 10,
                pos_y: 10,
                orientation: Orientation::North,
            },
            cool_downs: Default::default(),
        };
        let partial_scouted = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 43),
                health: 10,
                pos_x: 14,
                pos_y: 14,
                orientation: Orientation::North,
            },
            cool_downs: Default::default(),
        };
        let hidden = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 44),
                health: 10,
                pos_x: 8,
                pos_y: 15,
                orientation: Orientation::West,
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            turn: Some(Turn {
                action_points_left: 42,
                player_id: player.id,
            }),
            state: GameState::InGame,
            players: HashMap::from([(player.id, player.clone())]),
            ships: ShipManager::new_with_ships(vec![ship, scouted, partial_scouted, hidden]),
            team_a: HashSet::from([player.id]),
            team_b: HashSet::from([42]),
            ..Default::default()
        }));
        let mut g = g.write().await;

        let result = Action::ScoutPlane {
            ship_id,
            properties: ScoutPlaneProperties {
                center: Some(Coordinate { x: 10, y: 10 }),
            },
        }
        .apply_on(&mut g);
        assert!(matches!(result, Ok(Some(ActionResult { .. }))));
        if let Ok(Some(ActionResult {
            inflicted_damage_by_ship,
            inflicted_damage_at,
            ships_destroyed,
            lost_vision_at,
            gain_vision_at,
            temp_vision_at,
        })) = result
        {
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(ships_destroyed.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.contains(&Coordinate { x: 14, y: 14 }));
            assert!(temp_vision_at.contains(&Coordinate { x: 10, y: 10 }));
            assert!(temp_vision_at.contains(&Coordinate { x: 10, y: 11 }));
            assert_eq!(temp_vision_at.len(), 3);
        }

        let ship = g.ships.get_by_id(&ship_id).unwrap();
        assert_eq!(g.turn.as_ref().unwrap().action_points_left, 32);
        assert_eq!(ship.cool_downs().len(), 1);
        assert!(ship.cool_downs().contains(&Cooldown::Ability {
            remaining_rounds: 10
        }));
    }
}

//noinspection DuplicatedCode
mod actions_predator_missile {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::game::ship::{Cooldown, GetShipID, Orientation, Ship, ShipData};
    use battleship_plus_common::game::ship_manager::ShipManager;
    use battleship_plus_common::types::*;

    use crate::game::actions::Action::PredatorMissile;
    use crate::game::actions::ActionResult;
    use crate::game::data::{Game, Player, Turn};
    use crate::game::states::GameState;

    #[tokio::test]
    async fn actions_predator_missile() {
        let player = Player::default();
        let ship_id = (player.id, 0);

        let ship = Ship::Battleship {
            balancing: Arc::from(BattleshipBalancing {
                predator_missile_radius: 4,
                predator_missile_range: 10,
                predator_missile_damage: 9,
                common_balancing: Some(CommonBalancing {
                    ability_costs: Some(Costs {
                        cooldown: 10,
                        action_points: 10,
                    }),
                    ..Default::default()
                }),
            }),
            data: ShipData {
                id: ship_id,
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let destroyed = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 42),
                health: 5,
                pos_x: 9,
                pos_y: 9,
                orientation: Orientation::South,
            },
            cool_downs: Default::default(),
        };
        let partial_hit_destroyed = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 43),
                health: 5,
                pos_x: 6,
                pos_y: 6,
                orientation: Orientation::South,
            },
            cool_downs: Default::default(),
        };
        let hit = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 44),
                health: 10,
                pos_x: 10,
                pos_y: 10,
                orientation: Orientation::North,
            },
            cool_downs: Default::default(),
        };
        let partial_hit = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 45),
                health: 10,
                pos_x: 14,
                pos_y: 14,
                orientation: Orientation::North,
            },
            cool_downs: Default::default(),
        };
        let not_hit = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                ..Default::default()
            }),
            data: ShipData {
                id: (42, 46),
                health: 10,
                pos_x: 8,
                pos_y: 15,
                orientation: Orientation::West,
            },
            cool_downs: Default::default(),
        };

        let g = Arc::new(RwLock::new(Game {
            turn: Some(Turn {
                action_points_left: 42,
                player_id: player.id,
            }),
            state: GameState::InGame,
            players: HashMap::from([(player.id, player.clone())]),
            ships: ShipManager::new_with_ships(vec![
                ship.clone(),
                destroyed.clone(),
                partial_hit_destroyed.clone(),
                hit.clone(),
                partial_hit.clone(),
                not_hit.clone(),
            ]),
            team_a: HashSet::from([player.id]),
            ..Default::default()
        }));
        let mut g = g.write().await;

        let result = PredatorMissile {
            ship_id,
            properties: PredatorMissileProperties {
                center: Some(Coordinate { x: 10, y: 10 }),
            },
        }
        .apply_on(&mut g);
        assert!(matches!(result, Ok(Some(ActionResult { .. }))));
        if let Ok(Some(ActionResult {
            inflicted_damage_by_ship,
            inflicted_damage_at,
            ships_destroyed,
            lost_vision_at,
            gain_vision_at,
            temp_vision_at,
        })) = result
        {
            assert_eq!(inflicted_damage_by_ship.len(), 4);
            assert!(inflicted_damage_by_ship.contains_key(&partial_hit_destroyed.id()));
            assert!(inflicted_damage_by_ship.contains_key(&destroyed.id()));
            assert!(inflicted_damage_by_ship.contains_key(&hit.id()));
            assert!(inflicted_damage_by_ship.contains_key(&partial_hit.id()));

            assert_eq!(inflicted_damage_at.len(), 6);
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 14, y: 14 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 10, y: 10 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 10, y: 11 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 9, y: 9 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 9, y: 8 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 6, y: 6 }));

            assert_eq!(ships_destroyed.len(), 2);
            assert!(ships_destroyed.contains(&partial_hit_destroyed.id()));
            assert!(ships_destroyed.contains(&destroyed.id()));

            assert_eq!(lost_vision_at.len(), 4);
            assert!(lost_vision_at.contains(&Coordinate { x: 9, y: 9 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 9, y: 8 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 6, y: 6 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 6, y: 5 }));

            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.is_empty());
        }

        let ship = g.ships.get_by_id(&ship_id).unwrap();
        assert_eq!(g.turn.as_ref().unwrap().action_points_left, 32);
        assert_eq!(ship.cool_downs().len(), 1);
        assert!(ship.cool_downs().contains(&Cooldown::Ability {
            remaining_rounds: 10
        }));

        assert_eq!(g.ships.get_by_id(&destroyed.id()), None);
        assert_eq!(g.ships.get_by_id(&partial_hit_destroyed.id()), None);
        assert!(g.ships.get_by_id(&hit.id()).is_some());
        assert_eq!(g.ships.get_by_id(&hit.id()).unwrap().data().health, 1);
        assert_eq!(
            g.ships.get_by_id(&partial_hit.id()).unwrap().data().health,
            1
        );
        assert_eq!(g.ships.get_by_id(&not_hit.id()).unwrap().data().health, 10);
    }
}
