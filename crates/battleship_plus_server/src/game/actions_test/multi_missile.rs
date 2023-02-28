//noinspection DuplicatedCode
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use battleship_plus_common::game::ship::{Cooldown, GetShipID, Orientation};
use battleship_plus_common::game::ship_manager::ShipManager;
use battleship_plus_common::types::*;

use crate::game::actions::Action::MultiMissile;
use crate::game::actions::ActionResult;
use crate::game::data::{Game, Player, Turn};
use crate::game::ship_builder::{GeneralShipBuilder, ShipBuilder};
use crate::game::states::GameState;

#[tokio::test]
async fn actions_multi_missile() {
    const MISSILE_RADIUS: u32 = 1;

    let player = Player::default();
    let ship_id = (player.id, 0);

    let ship = GeneralShipBuilder::default()
        .ability(10, 10)
        .owner(player.id)
        .number(0)
        .position(0, 0)
        .orientation(Orientation::North)
        .destroyer()
        .multi_missile(MISSILE_RADIUS, 9)
        .build();

    let mut ship_numbers = 0..;

    let destroyed1 = GeneralShipBuilder::default()
        .health(5)
        .position(20, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();
    let hit1 = GeneralShipBuilder::default()
        .health(10)
        .position(21, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();
    let missed1 = GeneralShipBuilder::default()
        .health(5)
        .position(22, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();

    let destroyed2 = GeneralShipBuilder::default()
        .health(5)
        .position(30, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();
    let hit2 = GeneralShipBuilder::default()
        .health(10)
        .position(31, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();
    let missed2 = GeneralShipBuilder::default()
        .health(5)
        .position(32, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();

    let destroyed_overlapping = GeneralShipBuilder::default()
        .health(10)
        .position(29, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::West)
        .destroyer()
        .build();

    let g = Arc::new(RwLock::new(Game {
        turn: Some(Turn {
            action_points_left: 42,
            player_id: player.id,
            temp_vision: Default::default(),
        }),
        state: GameState::InGame,
        players: HashMap::from([(player.id, player.clone())]),
        ships: ShipManager::new_with_ships(vec![
            ship.clone(),
            destroyed1.clone(),
            hit1.clone(),
            missed1.clone(),
            destroyed2.clone(),
            hit2.clone(),
            missed2.clone(),
            destroyed_overlapping.clone(),
        ]),
        team_a: HashSet::from([player.id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = MultiMissile {
        ship_id,
        properties: MultiMissileProperties {
            position_a: Some(Coordinate { x: 20, y: 0 }),
            position_b: Some(Coordinate { x: 30, y: 0 }),
            position_c: Some(Coordinate { x: 27, y: 0 }),
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
        assert!(inflicted_damage_by_ship.contains_key(&destroyed1.id()));
        assert!(inflicted_damage_by_ship.contains_key(&hit1.id()));
        assert!(inflicted_damage_by_ship.contains_key(&destroyed2.id()));
        assert!(inflicted_damage_by_ship.contains_key(&hit2.id()));
        assert!(inflicted_damage_by_ship.contains_key(&destroyed_overlapping.id()));
        assert_eq!(inflicted_damage_by_ship.len(), 5);

        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 0 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 1 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 21, y: 0 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 21, y: 1 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 30, y: 0 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 30, y: 1 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 31, y: 0 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 31, y: 1 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 28, y: 0 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 29, y: 0 }));
        assert_eq!(inflicted_damage_at.len(), 10);

        assert!(ships_destroyed.iter().any(|s| s.id() == destroyed1.id()));
        assert!(ships_destroyed.iter().any(|s| s.id() == destroyed2.id()));
        assert!(ships_destroyed
            .iter()
            .any(|s| s.id() == destroyed_overlapping.id()));
        assert_eq!(ships_destroyed.len(), 3);

        assert!(lost_vision_at.contains(&Coordinate { x: 20, y: 0 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 20, y: 1 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 30, y: 0 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 30, y: 1 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 29, y: 0 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 28, y: 0 }));
        assert_eq!(lost_vision_at.len(), 6);

        assert!(gain_vision_at.is_empty());
        assert!(temp_vision_at.is_empty());
    }

    let ship = g.ships.get_by_id(&ship_id).unwrap();
    assert_eq!(g.turn.as_ref().unwrap().action_points_left, 32);
    assert_eq!(ship.cool_downs().len(), 1);
    assert!(ship.cool_downs().contains(&Cooldown::Ability {
        remaining_rounds: 10
    }));

    assert_eq!(g.ships.get_by_id(&destroyed1.id()), None);
    assert_eq!(g.ships.get_by_id(&destroyed2.id()), None);
    assert_eq!(g.ships.get_by_id(&destroyed_overlapping.id()), None);
    assert!(g.ships.get_by_id(&hit1.id()).is_some());
    assert!(g.ships.get_by_id(&hit2.id()).is_some());
    assert!(g.ships.get_by_id(&missed1.id()).is_some());
    assert!(g.ships.get_by_id(&missed2.id()).is_some());
    assert_eq!(g.ships.get_by_id(&hit1.id()).unwrap().data().health, 1);
    assert_eq!(g.ships.get_by_id(&hit2.id()).unwrap().data().health, 1);
    assert_eq!(g.ships.get_by_id(&missed1.id()).unwrap().data().health, 5);
    assert_eq!(g.ships.get_by_id(&missed2.id()).unwrap().data().health, 5);
}

#[tokio::test]
async fn actions_multi_missile_same_spot() {
    const MISSILE_RADIUS: u32 = 0;

    let player = Player::default();
    let ship_id = (player.id, 0);

    let ship = GeneralShipBuilder::default()
        .ability(10, 10)
        .owner(player.id)
        .number(0)
        .position(0, 0)
        .orientation(Orientation::North)
        .destroyer()
        .multi_missile(MISSILE_RADIUS, 10)
        .build();

    let mut ship_numbers = 0..;

    let destroyed = GeneralShipBuilder::default()
        .health(30)
        .position(2, 0)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();

    let g = Arc::new(RwLock::new(Game {
        turn: Some(Turn {
            action_points_left: 42,
            player_id: player.id,
            temp_vision: Default::default(),
        }),
        state: GameState::InGame,
        players: HashMap::from([(player.id, player.clone())]),
        ships: ShipManager::new_with_ships(vec![ship.clone(), destroyed.clone()]),
        team_a: HashSet::from([player.id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = MultiMissile {
        ship_id,
        properties: MultiMissileProperties {
            position_a: Some(Coordinate { x: 2, y: 0 }),
            position_b: Some(Coordinate { x: 2, y: 0 }),
            position_c: Some(Coordinate { x: 2, y: 0 }),
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
        assert_eq!(inflicted_damage_by_ship.len(), 1);
        assert!(inflicted_damage_by_ship.contains_key(&destroyed.id()));
        assert_eq!(
            inflicted_damage_by_ship.get(&destroyed.id()).cloned(),
            Some(30)
        );

        assert_eq!(inflicted_damage_at.len(), 1);
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 2, y: 0 }));
        assert_eq!(
            inflicted_damage_at.get(&Coordinate { x: 2, y: 0 }).cloned(),
            Some(30)
        );

        assert_eq!(ships_destroyed.len(), 1);
        assert!(ships_destroyed.iter().any(|s| s.id() == destroyed.id()));

        assert_eq!(lost_vision_at.len(), 2);
        assert!(lost_vision_at.contains(&Coordinate { x: 2, y: 0 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 2, y: 1 }));

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
}
