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
async fn actions_movement() {
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
async fn actions_movement_vision() {
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
        cooldowns: Default::default(),
    };
    let ship1 = Ship::Destroyer {
        data: ShipData {
            id: (player.id, 1),
            pos_x: 10,
            pos_y: 7,
            orientation: Orientation::North,
            ..Default::default()
        },
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
async fn actions_movement_action_points() {
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
async fn actions_movement_cooldown() {
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
async fn actions_movement_unknown_player() {
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
async fn actions_movement_deny_out_of_bounds() {
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
async fn actions_movement_destroy_on_collision() {
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        assert!(inflicted_damage_at.contains(&Coordinate { x: 0, y: 11 }));
        assert!(temp_vision_at.is_empty());
    }

    // check both ships destroyed
    {
        assert!(g.ships.get_by_id(&ship1.id()).is_none());
        assert!(g.ships.get_by_id(&ship2.id()).is_none());
    }
}

#[tokio::test]
async fn actions_movement_not_players_turn() {
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
