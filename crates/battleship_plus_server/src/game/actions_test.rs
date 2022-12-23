//noinspection DuplicatedCode
mod actions_team_switch {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player, PlayerID};

    #[tokio::test]
    async fn actions_team_switch() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(Game {
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

        // player is in team a
        {
            let g = game.read().await;
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }

        // switch team a -> b
        assert!(Action::TeamSwitch { player_id }
            .apply_on(game.clone())
            .await
            .is_ok());
        {
            let g = game.read().await;
            assert!(!g.team_a.contains(&player_id));
            assert!(g.team_b.contains(&player_id));
        }

        // switch team b -> a
        assert!(Action::TeamSwitch { player_id }
            .apply_on(game.clone())
            .await
            .is_ok());
        {
            let g = game.read().await;
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }
    }

    #[tokio::test]
    async fn actions_team_switch_detect_inconsistent_state() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(Game {
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

        let res = Action::TeamSwitch { player_id }
            .apply_on(game.clone())
            .await;
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
        let game = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));

        let res = Action::TeamSwitch { player_id }
            .apply_on(game.clone())
            .await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false,
        })
    }
}

//noinspection DuplicatedCode
mod actions_player_set_ready_state {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::messages::SetReadyStateRequest;

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player, PlayerID};

    #[tokio::test]
    async fn actions_player_set_ready() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(Game {
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

        // set player ready
        assert!(Action::SetReady {
            player_id,
            request: SetReadyStateRequest { ready_state: true },
        }
        .apply_on(game.clone())
        .await
        .is_ok());
        {
            let g = game.read().await;
            assert!(g.players.get(&player_id).unwrap().is_ready);
        }

        // set player not ready
        assert!(Action::SetReady {
            player_id,
            request: SetReadyStateRequest { ready_state: false },
        }
        .apply_on(game.clone())
        .await
        .is_ok());
        {
            let g = game.read().await;
            assert!(!g.players.get(&player_id).unwrap().is_ready);
        }
    }

    #[tokio::test]
    async fn actions_set_ready_unknown_player() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));

        let res = Action::SetReady {
            player_id,
            request: SetReadyStateRequest { ready_state: true },
        }
        .apply_on(game.clone())
        .await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false,
        })
    }
}

//noinspection DuplicatedCode
mod actions_shoot {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::types::*;

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player};
    use crate::game::ship::{Cooldown, GetShipID, Ship, ShipData};
    use crate::game::ship_manager::ShipManager;

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

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![
                ship_src.clone(),
                ship_target1.clone(),
                ship_target2.clone(),
            ]),
            ..Default::default()
        }));

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
        .apply_on(game.clone())
        .await
        .is_ok());

        // check ship_target1 destroyed and ship_target2 untouched
        {
            let g = game.read().await;
            assert!(g.ships.get_by_id(&ship_target1.id()).is_none());
            assert!(g.ships.get_by_id(&ship_target2.id()).is_some());
            assert_eq!(
                g.ships.get_by_id(&ship_target2.id()).unwrap().data().health,
                11
            );
        }

        // shoot ship_target2
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate {
                    x: ship_target2.data().pos_x as u32,
                    y: ship_target2.data().pos_y as u32,
                }),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // check ship_target1 destroyed and ship_target2 health reduced
        {
            let g = game.read().await;
            assert!(g.ships.get_by_id(&ship_target1.id()).is_none());
            assert!(g.ships.get_by_id(&ship_target2.id()).is_some());
            assert_eq!(
                g.ships.get_by_id(&ship_target2.id()).unwrap().data().health,
                1
            );
        }

        // missed shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // board untouched
        {
            let g = game.read().await;
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
            action_points: 5,
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

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship]),
            ..Default::default()
        }));

        // first shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // action points reduced
        {
            let g = game.read().await;
            assert_eq!(g.players.get(&player.id).unwrap().action_points, 1);
        }

        // deny second shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(game.clone())
        .await
        .is_err());

        // board untouched
        {
            let g = game.read().await;
            assert_eq!(g.players.get(&player.id).unwrap().action_points, 1);
        }
    }

    #[tokio::test]
    async fn actions_shoot_cooldown() {
        let player = Player {
            action_points: 5,
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

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            ..Default::default()
        }));

        // first shot
        assert!(Action::Shoot {
            ship_id: (player.id, 0),
            properties: ShootProperties {
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // check cooldown
        {
            let g = game.read().await;
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
        .apply_on(game.clone())
        .await
        .is_err());

        // board untouched
        {
            let g = game.read().await;
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
        let game = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));

        let res = Action::Shoot {
            ship_id: (42, 1),
            properties: ShootProperties {
                target: Some(Coordinate { x: 0, y: 0 }),
            },
        }
        .apply_on(game.clone())
        .await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false,
        })
    }

    #[tokio::test]
    async fn actions_shoot_reject_shot_into_oblivion() {
        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            ..Default::default()
        }));

        let res = Action::Shoot {
            ship_id: (42, 1),
            properties: ShootProperties {
                target: Some(Coordinate { x: 9999, y: 9999 }),
            },
        }
        .apply_on(game.clone())
        .await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false,
        })
    }
}

//noinspection DuplicatedCode
mod actions_move {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use battleship_plus_common::types::*;

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player};
    use crate::game::ship::{Cooldown, GetShipID, Orientation, Ship, ShipData};
    use crate::game::ship_manager::ShipManager;

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
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: ShipData {
                pos_x: 0,
                pos_y: 0,
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            ..Default::default()
        }));

        // move ship forward
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // check ship's new position
        {
            let g = game.read().await;
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1))
        }

        // move ship backward
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // check ship's new position
        {
            let g = game.read().await;
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 0))
        }
    }

    #[tokio::test]
    async fn actions_move_action_points() {
        let player = Player {
            action_points: 5,
            ..Default::default()
        };
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_speed: 2,
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
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            ..Default::default()
        }));

        // move ship forward
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // check action points
        {
            let g = game.read().await;
            assert_eq!(g.players.get(&player.id).unwrap().action_points, 2);
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1));
        }

        // try to move ship backwards and fail
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_err());

        // check board untouched
        {
            let g = game.read().await;
            assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (0, 1));
            assert_eq!(g.players.get(&player.id).unwrap().action_points, 2);
        }
    }

    #[tokio::test]
    async fn actions_move_cooldown() {
        let player = Player::default();
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_speed: 2,
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
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship.clone()]),
            ..Default::default()
        }));

        // move ship forward
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // check ship's new position and cooldown
        {
            let g = game.read().await;
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
        .apply_on(game.clone())
        .await
        .is_err());

        // check board untouched
        {
            let g = game.read().await;
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
        let game = Arc::new(RwLock::new(Game {
            ..Default::default()
        }));

        let res = Action::Move {
            ship_id: (42, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Forward),
            },
        }
        .apply_on(game.clone())
        .await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false,
        })
    }

    #[tokio::test]
    async fn actions_move_deny_out_of_bounds() {
        let player = Player::default();
        let ship1 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_speed: 2,
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
                orientation: Orientation::South,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };
        let ship2 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_speed: 2,
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
                pos_x: 23,
                pos_y: 23,
                orientation: Orientation::North,
                ..Default::default()
            },
            cool_downs: Default::default(),
        };

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship1, ship2]),
            ..Default::default()
        }));

        // move ship1 backwards
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_err());

        // move ship2 backward
        assert!(Action::Move {
            ship_id: (player.id, 1),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_err());
    }

    #[tokio::test]
    async fn actions_move_destroy_on_collision() {
        let player = Player::default();
        let ship1 = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    movement_speed: 2,
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
                    movement_speed: 2,
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

        let game = Arc::new(RwLock::new(Game {
            board_size: 24,
            players: HashMap::from([(player.id, player.clone())]),
            team_a: HashSet::from([player.id]),
            ships: ShipManager::new_with_ships(vec![ship1.clone(), ship2.clone()]),
            ..Default::default()
        }));

        // move ship1 backwards into ship2
        assert!(Action::Move {
            ship_id: (player.id, 0),
            properties: MoveProperties {
                direction: i32::from(MoveDirection::Backward),
            },
        }
        .apply_on(game.clone())
        .await
        .is_ok());

        // check both ships destroyed
        {
            let g = game.read().await;
            assert!(g.ships.get_by_id(&ship1.id()).is_none());
            assert!(g.ships.get_by_id(&ship2.id()).is_none());
        }
    }
}
