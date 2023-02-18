//noinspection DuplicatedCode
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
