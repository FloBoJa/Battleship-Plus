//noinspection DuplicatedCode
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
    assert!(matches!(result, Ok(ActionResult::Single { .. })));
    if let Ok(ActionResult::Single {
        ships_destroyed,
        inflicted_damage_by_ship,
        inflicted_damage_at,
        gain_vision_at,
        lost_vision_at,
        temp_vision_at,
        ..
    }) = result
    {
        assert!(lost_vision_at.contains(&Coordinate { x: 5, y: 5 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 5, y: 6 }));
        assert!(gain_vision_at.is_empty());
        assert!(ships_destroyed.iter().any(|s| s.id() == ship_target1.id()));
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
    assert!(matches!(result, Ok(ActionResult::Single { .. })));
    if let Ok(ActionResult::Single {
        ships_destroyed,
        inflicted_damage_by_ship,
        inflicted_damage_at,
        gain_vision_at,
        lost_vision_at,
        temp_vision_at,
        ..
    }) = result
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
        Ok(ActionResult::None)
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
