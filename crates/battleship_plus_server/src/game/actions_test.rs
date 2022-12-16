mod actions_team_switch {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player, PlayerID};

    #[tokio::test]
    async fn actions_team_switch() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(
            Game {
                players: HashMap::from([
                    (player_id, Player {
                        id: player_id,
                        ..Default::default()
                    })
                ]),
                team_a: HashSet::from([player_id]),
                ..Default::default()
            }
        ));

        // player is in team a
        {
            let g = game.read().await;
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }

        // switch team a -> b
        assert!(Action::TeamSwitch { player_id }.apply_on(game.clone()).await.is_ok());
        {
            let g = game.read().await;
            assert!(!g.team_a.contains(&player_id));
            assert!(g.team_b.contains(&player_id));
        }

        // switch team b -> a
        assert!(Action::TeamSwitch { player_id }.apply_on(game.clone()).await.is_ok());
        {
            let g = game.read().await;
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }
    }

    #[tokio::test]
    async fn actions_team_switch_detect_inconsistent_state() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(
            Game {
                players: HashMap::from([
                    (player_id, Player {
                        id: player_id,
                        ..Default::default()
                    })
                ]),
                team_a: HashSet::from([player_id]),
                team_b: HashSet::from([player_id]),
                ..Default::default()
            }
        ));

        let res = Action::TeamSwitch { player_id }.apply_on(game.clone()).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::InconsistentState(e) => e == "found illegal team assignment for player 42",
            _ => false
        })
    }

    #[tokio::test]
    async fn actions_team_switch_unknown_player() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(
            Game {
                ..Default::default()
            }
        ));

        let res = Action::TeamSwitch { player_id }.apply_on(game.clone()).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false
        })
    }
}

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
        let game = Arc::new(RwLock::new(
            Game {
                players: HashMap::from([
                    (player_id, Player {
                        id: player_id,
                        ..Default::default()
                    })
                ]),
                team_a: HashSet::from([player_id]),
                ..Default::default()
            }
        ));

        // set player ready
        assert!(Action::SetReady { player_id, request: SetReadyStateRequest { ready_state: true } }
            .apply_on(game.clone()).await.is_ok());
        {
            let g = game.read().await;
            assert_eq!(g.players.get(&player_id).unwrap().is_ready, true);
        }

        // set player not ready
        assert!(Action::SetReady { player_id, request: SetReadyStateRequest { ready_state: false } }
            .apply_on(game.clone()).await.is_ok());
        {
            let g = game.read().await;
            assert_eq!(g.players.get(&player_id).unwrap().is_ready, false);
        }
    }

    #[tokio::test]
    async fn actions_set_ready_unknown_player() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(
            Game {
                ..Default::default()
            }
        ));

        let res = Action::SetReady { player_id, request: SetReadyStateRequest { ready_state: true } }
            .apply_on(game.clone()).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false
        })
    }
}

mod actions_shoot {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use rstar::RTree;
    use tokio::sync::RwLock;

    use battleship_plus_common::messages::{CommonBalancing, Coordinate, Costs, DestroyerBalancing, ShootRequest};

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Cooldown, Game, GetShipID, Player, Ship, ShipData, ShipRef};

    #[tokio::test]
    async fn actions_shoot() {
        let player = Player::default();
        let ship_src = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    shoot_damage: 10,
                    shoot_range: 128,
                    shoot_costs: Some(Costs { cooldown: 0, action_points: 0 }),
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
                player_id: 2,
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
                player_id: 2,
                health: 11,
                pos_x: 10,
                pos_y: 10,
                ..ship_src.data()
            },
            cool_downs: Default::default(),
        };

        let game = Arc::new(RwLock::new(
            Game {
                board_size: 24,
                players: HashMap::from([
                    (player.id, player.clone())
                ]),
                team_a: HashSet::from([player.id]),
                ships: HashMap::from([
                    (ship_src.id(), ship_src.clone()),
                    (ship_target1.id(), ship_target1.clone()),
                    (ship_target2.id(), ship_target2.clone()),
                ]),
                ships_geo_lookup: RTree::bulk_load(vec![
                    ShipRef(Arc::from(ship_src)),
                    ShipRef(Arc::from(ship_target1.clone())),
                    ShipRef(Arc::from(ship_target2.clone())),
                ]),
                ..Default::default()
            }
        ));


        // shoot ship_target1
        assert!(Action::Shoot {
            player_id: player.id,
            request: ShootRequest {
                ship_number: 0,
                target: Some(Coordinate { x: ship_target1.data().pos_x as u32, y: ship_target1.data().pos_y as u32 }),
            },
        }.apply_on(game.clone()).await.is_ok());

        // check ship_target1 destroyed and ship_target2 untouched
        {
            let g = game.read().await;
            assert_eq!(g.ships.contains_key(&ship_target1.id()), false);
            assert_eq!(g.ships.contains_key(&ship_target2.id()), true);
            assert_eq!(g.ships.get(&ship_target2.id()).unwrap().data().health, 11);
        }

        // shoot ship_target2
        assert!(Action::Shoot {
            player_id: player.id,
            request: ShootRequest {
                ship_number: 0,
                target: Some(Coordinate { x: ship_target2.data().pos_x as u32, y: ship_target2.data().pos_y as u32 }),
            },
        }.apply_on(game.clone()).await.is_ok());

        // check ship_target1 destroyed and ship_target2 health reduced
        {
            let g = game.read().await;
            assert_eq!(g.ships.contains_key(&ship_target1.id()), false);
            assert_eq!(g.ships.contains_key(&ship_target2.id()), true);
            assert_eq!(g.ships.get(&ship_target2.id()).unwrap().data().health, 1);
        }

        // missed shot
        assert!(Action::Shoot {
            player_id: player.id,
            request: ShootRequest {
                ship_number: 0,
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }.apply_on(game.clone()).await.is_ok());

        // board untouched
        {
            let g = game.read().await;
            assert_eq!(g.ships.contains_key(&ship_target1.id()), false);
            assert_eq!(g.ships.contains_key(&ship_target2.id()), true);
            assert_eq!(g.ships.get(&ship_target2.id()).unwrap().data().health, 1);
        }
    }

    #[tokio::test]
    async fn actions_shoot_costs() {
        let player = Player {
            action_points: 5,
            ..Default::default()
        };
        let ship = Ship::Destroyer {
            balancing: Arc::from(DestroyerBalancing {
                common_balancing: Some(CommonBalancing {
                    shoot_damage: 10,
                    shoot_range: 128,
                    shoot_costs: Some(Costs { cooldown: 0, action_points: 4 }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: Default::default(),
            cool_downs: Default::default(),
        };

        let game = Arc::new(RwLock::new(
            Game {
                board_size: 24,
                players: HashMap::from([
                    (player.id, player.clone())
                ]),
                team_a: HashSet::from([player.id]),
                ships: HashMap::from([
                    (ship.id(), ship.clone()),
                ]),
                ships_geo_lookup: RTree::bulk_load(vec![
                    ShipRef(Arc::from(ship)),
                ]),
                ..Default::default()
            }
        ));

        // first shot
        assert!(Action::Shoot {
            player_id: player.id,
            request: ShootRequest {
                ship_number: 0,
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }.apply_on(game.clone()).await.is_ok());

        // action points reduced
        {
            let g = game.read().await;
            assert_eq!(g.players.get(&player.id).unwrap().action_points, 1);
        }

        // deny second shot
        assert!(Action::Shoot {
            player_id: player.id,
            request: ShootRequest {
                ship_number: 0,
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }.apply_on(game.clone()).await.is_err());

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
                    shoot_costs: Some(Costs { cooldown: 2, action_points: 0 }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            data: Default::default(),
            cool_downs: Default::default(),
        };

        let game = Arc::new(RwLock::new(
            Game {
                board_size: 24,
                players: HashMap::from([
                    (player.id, player.clone())
                ]),
                team_a: HashSet::from([player.id]),
                ships: HashMap::from([
                    (ship.id(), ship.clone()),
                ]),
                ships_geo_lookup: RTree::bulk_load(vec![
                    ShipRef(Arc::from(ship.clone())),
                ]),
                ..Default::default()
            }
        ));

        // first shot
        assert!(Action::Shoot {
            player_id: player.id,
            request: ShootRequest {
                ship_number: 0,
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }.apply_on(game.clone()).await.is_ok());

        // action points reduced
        {
            let g = game.read().await;
            assert!(g.ships.get(&ship.id()).unwrap().cool_downs()
                .contains(&Cooldown::Cannon { remaining_rounds: 2 }));
        }

        // deny second shot
        assert!(Action::Shoot {
            player_id: player.id,
            request: ShootRequest {
                ship_number: 0,
                target: Some(Coordinate { x: 20, y: 20 }),
            },
        }.apply_on(game.clone()).await.is_err());

        // board untouched
        {
            let g = game.read().await;
            assert!(g.ships.get(&ship.id()).unwrap().cool_downs()
                .contains(&Cooldown::Cannon { remaining_rounds: 2 }));
        }
    }

    #[tokio::test]
    async fn actions_shoot_unknown_player() {
        let game = Arc::new(RwLock::new(
            Game {
                ..Default::default()
            }
        ));

        let res = Action::Shoot {
            player_id: 42,
            request: ShootRequest {
                ship_number: 1,
                target: Some(Coordinate { x: 0, y: 0 }),
            },
        }.apply_on(game.clone()).await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false
        })
    }

    #[tokio::test]
    async fn actions_shoot_reject_shot_into_oblivion() {
        let game = Arc::new(RwLock::new(
            Game {
                board_size: 24,
                ..Default::default()
            }
        ));

        let res = Action::Shoot {
            player_id: 42,
            request: ShootRequest {
                ship_number: 1,
                target: Some(Coordinate { x: 9999, y: 9999 }),
            },
        }.apply_on(game.clone()).await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false
        })
    }
}
