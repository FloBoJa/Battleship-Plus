use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use battleship_plus_common::game::ship::{Cooldown, GetShipID, Orientation};
use battleship_plus_common::game::ship_manager::ShipManager;
use battleship_plus_common::game::ActionValidationError;
use battleship_plus_common::types::{Coordinate, EngineBoostProperties};

use crate::game::actions::{Action, ActionExecutionError, ActionResult};
//noinspection DuplicatedCode
use crate::game::data::{Game, Player, Turn};
use crate::game::ship_builder::{GeneralShipBuilder, ShipBuilder};

#[tokio::test]
#[allow(clippy::needless_range_loop)]
async fn actions_engine_boost() {
    let engine_boost_range: u32 = 6;

    let player = Player {
        id: 0,
        ..Default::default()
    };
    let player2 = Player {
        id: 1,
        ..Default::default()
    };

    let mut ship_numbers = 0..;

    let ship = GeneralShipBuilder::default()
        .owner(player.id)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::East)
        .ability(5, 2)
        .vision(1)
        .position(0, 0)
        .cruiser()
        .engine_boost(engine_boost_range)
        .build();

    let ship2 = GeneralShipBuilder::default()
        .owner(player2.id)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .ability(5, 2)
        .vision(1)
        .position(4, 1)
        .cruiser()
        .build();

    let g = Arc::new(RwLock::new(Game {
        players: HashMap::from([(player.id, player.clone()), (player2.id, player2.clone())]),
        team_a: HashSet::from([player.id]),
        team_b: HashSet::from([player2.id]),
        ships: ShipManager::new_with_ships(vec![ship.clone(), ship2.clone()]),
        turn: Some(Turn::new(player.id, 5)),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Action::EngineBoost {
        ship_id: ship.id(),
        properties: EngineBoostProperties {},
    }
    .apply_on(&mut g);
    assert!(matches!(result, Ok(ActionResult::Multiple { .. })));
    if let Ok(ActionResult::Multiple(results)) = result {
        assert_eq!(results.len(), engine_boost_range as usize);

        if let Ok(ActionResult::Single {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        }) = results[0].clone()
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.contains(&Coordinate {
                x: ship2.data().pos_x as u32,
                y: ship2.data().pos_y as u32,
            }));
            assert_eq!(gain_vision_at.len(), 1);
            assert!(temp_vision_at.is_empty());
        }

        for i in 1..(engine_boost_range - 1) {
            if let Ok(ActionResult::Single {
                ships_destroyed,
                inflicted_damage_by_ship,
                inflicted_damage_at,
                gain_vision_at,
                lost_vision_at,
                temp_vision_at,
            }) = results[i as usize].clone()
            {
                assert!(ships_destroyed.is_empty());
                assert!(inflicted_damage_by_ship.is_empty());
                assert!(inflicted_damage_at.is_empty());
                assert!(lost_vision_at.is_empty());
                assert!(gain_vision_at.is_empty());
                assert!(temp_vision_at.is_empty());
            }
        }

        if let Ok(ActionResult::Single {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        }) = results[(engine_boost_range - 1) as usize].clone()
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.contains(&Coordinate {
                x: ship2.data().pos_x as u32,
                y: ship2.data().pos_y as u32,
            }));
            assert_eq!(lost_vision_at.len(), 1);
            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.is_empty());
        }
    }

    // check ship's new position
    {
        assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (6, 0));
        assert_eq!(g.ships.get_by_id(&ship2.id()).unwrap().position(), (4, 1));
        assert!(g
            .ships
            .get_by_id(&ship.id())
            .unwrap()
            .cool_downs()
            .contains(&Cooldown::Ability {
                remaining_rounds: 2
            }));
        assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().cool_downs().len(), 1);
        assert_eq!(g.turn.as_ref().unwrap().action_points_left, 0);
    }
}

#[tokio::test]
#[allow(clippy::needless_range_loop)]
async fn actions_engine_boost_collision() {
    let engine_boost_range: u32 = 6;

    let player = Player {
        id: 0,
        ..Default::default()
    };
    let player2 = Player {
        id: 1,
        ..Default::default()
    };

    let mut ship_numbers = 0..;

    let ship = GeneralShipBuilder::default()
        .owner(player.id)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::East)
        .ability(5, 2)
        .vision(1)
        .position(0, 0)
        .cruiser()
        .engine_boost(engine_boost_range)
        .build();

    let ship2 = GeneralShipBuilder::default()
        .owner(player2.id)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .ability(5, 2)
        .vision(1)
        .position(4, 0)
        .cruiser()
        .build();

    let g = Arc::new(RwLock::new(Game {
        players: HashMap::from([(player.id, player.clone()), (player2.id, player2.clone())]),
        team_a: HashSet::from([player.id]),
        team_b: HashSet::from([player2.id]),
        ships: ShipManager::new_with_ships(vec![ship.clone(), ship2.clone()]),
        turn: Some(Turn::new(player.id, 5)),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Action::EngineBoost {
        ship_id: ship.id(),
        properties: EngineBoostProperties {},
    }
    .apply_on(&mut g);
    assert!(matches!(result, Ok(ActionResult::Multiple { .. })));
    if let Ok(ActionResult::Multiple(results)) = result {
        assert_eq!(results.len(), 3);

        if let Ok(ActionResult::Single {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        }) = results[0].clone()
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.contains(&Coordinate { x: 4, y: 0 }));
            assert!(gain_vision_at.contains(&Coordinate { x: 4, y: 1 }));
            assert_eq!(gain_vision_at.len(), 2);
            assert!(temp_vision_at.is_empty());
        }

        if let Ok(ActionResult::Single {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        }) = results[1].clone()
        {
            assert!(ships_destroyed.contains(&ship.id()));
            assert!(ships_destroyed.contains(&ship2.id()));
            assert_eq!(ships_destroyed.len(), 2);
            assert!(inflicted_damage_by_ship.contains_key(&ship.id()));
            assert!(inflicted_damage_by_ship.contains_key(&ship2.id()));
            assert_eq!(inflicted_damage_by_ship.len(), 2);
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 2, y: 0 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 3, y: 0 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 4, y: 0 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 4, y: 1 }));
            assert!(inflicted_damage_at.contains_key(&Coordinate { x: 4, y: 2 }));
            assert_eq!(inflicted_damage_at.len(), 5);
            assert!(lost_vision_at.contains(&Coordinate { x: 2, y: 0 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 3, y: 0 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 4, y: 0 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 4, y: 1 }));
            assert!(lost_vision_at.contains(&Coordinate { x: 4, y: 2 }));
            assert_eq!(lost_vision_at.len(), 5);
            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.is_empty());
        }

        assert!(
            matches!(results[2], Err(ActionExecutionError::Validation(ActionValidationError::NonExistentShip {id})) if id == ship.id())
        );
    }

    // check ship's new position
    {
        assert_eq!(g.ships.get_by_id(&ship.id()), None);
        assert_eq!(g.ships.get_by_id(&ship2.id()), None);
        assert_eq!(g.turn.as_ref().unwrap().action_points_left, 0);
    }
}

#[tokio::test]
#[allow(clippy::needless_range_loop)]
async fn actions_engine_boost_respect_world_border() {
    let engine_boost_range: u32 = 6;

    let player = Player {
        id: 0,
        ..Default::default()
    };

    let mut ship_numbers = 0..;

    let ship = GeneralShipBuilder::default()
        .owner(player.id)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::West)
        .ability(5, 2)
        .vision(1)
        .position(4, 0)
        .cruiser()
        .engine_boost(engine_boost_range)
        .build();

    let g = Arc::new(RwLock::new(Game {
        players: HashMap::from([(player.id, player.clone())]),
        team_a: HashSet::from([player.id]),
        ships: ShipManager::new_with_ships(vec![ship.clone()]),
        turn: Some(Turn::new(player.id, 5)),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Action::EngineBoost {
        ship_id: ship.id(),
        properties: EngineBoostProperties {},
    }
    .apply_on(&mut g);
    assert!(matches!(result, Ok(ActionResult::Multiple { .. })));
    if let Ok(ActionResult::Multiple(results)) = result {
        assert_eq!(results.len(), 3);

        if let Ok(ActionResult::Single {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        }) = results[0].clone()
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.is_empty());
        }

        if let Ok(ActionResult::Single {
            ships_destroyed,
            inflicted_damage_by_ship,
            inflicted_damage_at,
            gain_vision_at,
            lost_vision_at,
            temp_vision_at,
        }) = results[1].clone()
        {
            assert!(ships_destroyed.is_empty());
            assert!(inflicted_damage_by_ship.is_empty());
            assert!(inflicted_damage_at.is_empty());
            assert!(lost_vision_at.is_empty());
            assert!(gain_vision_at.is_empty());
            assert!(temp_vision_at.is_empty());
        }

        assert!(matches!(
            results[2],
            Err(ActionExecutionError::Validation(
                ActionValidationError::OutOfMap
            ))
        ));
    }

    // check ship's new position
    {
        assert_eq!(g.ships.get_by_id(&ship.id()).unwrap().position(), (2, 0));
        assert_eq!(g.turn.as_ref().unwrap().action_points_left, 0);
    }
}
