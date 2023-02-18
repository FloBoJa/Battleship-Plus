//noinspection DuplicatedCode
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use battleship_plus_common::game::ship::{Cooldown, GetShipID, Orientation};
use battleship_plus_common::game::ship_manager::ShipManager;
use battleship_plus_common::types::*;

use crate::game::actions::Action::Torpedo;
use crate::game::actions::ActionResult;
use crate::game::data::{Game, Player, Turn};
use crate::game::ship_builder::{GeneralShipBuilder, ShipBuilder};
use crate::game::states::GameState;

#[tokio::test]
async fn actions_torpedo_north() {
    const TORPEDO_RANGE: u32 = 10;

    let player = Player::default();
    let ship_id = (player.id, 0);

    let ship = GeneralShipBuilder::default()
        .ability(10, 10)
        .owner(player.id)
        .number(0)
        .position(20, 20)
        .orientation(Orientation::North)
        .submarine()
        .torpedo(TORPEDO_RANGE, 9)
        .build();

    let mut ship_numbers = 0..;

    let destroyed = GeneralShipBuilder::default()
        .health(5)
        .position(20, 25)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::West)
        .destroyer()
        .build();
    let hit = GeneralShipBuilder::default()
        .health(10)
        .position(20, 26)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();
    let missed = GeneralShipBuilder::default()
        .health(10)
        .position(
            20,
            ship.data().pos_y + ship.len() + 1 + TORPEDO_RANGE as i32,
        )
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::West)
        .destroyer()
        .build();

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
            hit.clone(),
            missed.clone(),
        ]),
        team_a: HashSet::from([player.id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Torpedo {
        ship_id,
        properties: TorpedoProperties {
            direction: Direction::North.into(),
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
        assert_eq!(inflicted_damage_by_ship.len(), 2);
        assert!(inflicted_damage_by_ship.contains_key(&hit.id()));
        assert!(inflicted_damage_by_ship.contains_key(&destroyed.id()));

        assert_eq!(inflicted_damage_at.len(), 3);
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 25 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 26 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 27 }));

        assert_eq!(ships_destroyed.len(), 1);
        assert!(ships_destroyed.contains(&destroyed.id()));

        assert_eq!(lost_vision_at.len(), 2);
        assert!(lost_vision_at.contains(&Coordinate { x: 20, y: 25 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 19, y: 25 }));

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
    assert!(g.ships.get_by_id(&hit.id()).is_some());
    assert_eq!(g.ships.get_by_id(&hit.id()).unwrap().data().health, 1);
    assert_eq!(g.ships.get_by_id(&missed.id()).unwrap().data().health, 10);
}

#[tokio::test]
async fn actions_torpedo_south() {
    const TORPEDO_RANGE: u32 = 10;

    let player = Player::default();
    let ship_id = (player.id, 0);

    let ship = GeneralShipBuilder::default()
        .ability(10, 10)
        .owner(player.id)
        .number(0)
        .position(20, 20)
        .orientation(Orientation::North)
        .submarine()
        .torpedo(TORPEDO_RANGE, 9)
        .build();

    let mut ship_numbers = 0..;

    let destroyed = GeneralShipBuilder::default()
        .health(5)
        .position(20, 15)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::West)
        .destroyer()
        .build();
    let hit = GeneralShipBuilder::default()
        .health(10)
        .position(20, 14)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::South)
        .destroyer()
        .build();
    let missed = GeneralShipBuilder::default()
        .health(10)
        .position(20, ship.data().pos_y - 2 - TORPEDO_RANGE as i32)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::West)
        .destroyer()
        .build();

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
            hit.clone(),
            missed.clone(),
        ]),
        team_a: HashSet::from([player.id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Torpedo {
        ship_id,
        properties: TorpedoProperties {
            direction: Direction::South.into(),
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
        assert_eq!(inflicted_damage_by_ship.len(), 2);
        assert!(inflicted_damage_by_ship.contains_key(&hit.id()));
        assert!(inflicted_damage_by_ship.contains_key(&destroyed.id()));

        assert_eq!(inflicted_damage_at.len(), 3);
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 15 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 14 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 20, y: 13 }));

        assert_eq!(ships_destroyed.len(), 1);
        assert!(ships_destroyed.contains(&destroyed.id()));

        assert_eq!(lost_vision_at.len(), 2);
        assert!(lost_vision_at.contains(&Coordinate { x: 20, y: 15 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 19, y: 15 }));

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
    assert!(g.ships.get_by_id(&hit.id()).is_some());
    assert_eq!(g.ships.get_by_id(&hit.id()).unwrap().data().health, 1);
    assert_eq!(g.ships.get_by_id(&missed.id()).unwrap().data().health, 10);
}

#[tokio::test]
async fn actions_torpedo_east() {
    const TORPEDO_RANGE: u32 = 10;

    let player = Player::default();
    let ship_id = (player.id, 0);

    let ship = GeneralShipBuilder::default()
        .ability(10, 10)
        .owner(player.id)
        .number(0)
        .position(20, 20)
        .orientation(Orientation::North)
        .submarine()
        .torpedo(TORPEDO_RANGE, 9)
        .build();

    let mut ship_numbers = 0..;

    let destroyed = GeneralShipBuilder::default()
        .health(5)
        .position(25, 20)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();
    let hit = GeneralShipBuilder::default()
        .health(10)
        .position(26, 20)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::East)
        .destroyer()
        .build();
    let missed = GeneralShipBuilder::default()
        .health(10)
        .position(ship.data().pos_x + 2 + TORPEDO_RANGE as i32, 20)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();

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
            hit.clone(),
            missed.clone(),
        ]),
        team_a: HashSet::from([player.id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Torpedo {
        ship_id,
        properties: TorpedoProperties {
            direction: Direction::East.into(),
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
        assert_eq!(inflicted_damage_by_ship.len(), 2);
        assert!(inflicted_damage_by_ship.contains_key(&hit.id()));
        assert!(inflicted_damage_by_ship.contains_key(&destroyed.id()));

        assert_eq!(inflicted_damage_at.len(), 3);
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 25, y: 20 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 26, y: 20 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 27, y: 20 }));

        assert_eq!(ships_destroyed.len(), 1);
        assert!(ships_destroyed.contains(&destroyed.id()));

        assert_eq!(lost_vision_at.len(), 2);
        assert!(lost_vision_at.contains(&Coordinate { x: 25, y: 20 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 25, y: 21 }));

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
    assert!(g.ships.get_by_id(&hit.id()).is_some());
    assert_eq!(g.ships.get_by_id(&hit.id()).unwrap().data().health, 1);
    assert_eq!(g.ships.get_by_id(&missed.id()).unwrap().data().health, 10);
}

#[tokio::test]
async fn actions_torpedo_west() {
    const TORPEDO_RANGE: u32 = 10;

    let player = Player::default();
    let ship_id = (player.id, 0);

    let ship = GeneralShipBuilder::default()
        .ability(10, 10)
        .owner(player.id)
        .number(0)
        .position(20, 20)
        .orientation(Orientation::North)
        .submarine()
        .torpedo(TORPEDO_RANGE, 9)
        .build();

    let mut ship_numbers = 0..;

    let destroyed = GeneralShipBuilder::default()
        .health(5)
        .position(15, 20)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();
    let hit = GeneralShipBuilder::default()
        .health(10)
        .position(14, 20)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::West)
        .destroyer()
        .build();
    let missed = GeneralShipBuilder::default()
        .health(10)
        .position(ship.data().pos_x - 2 - TORPEDO_RANGE as i32, 20)
        .owner(42)
        .number(ship_numbers.next().unwrap())
        .orientation(Orientation::North)
        .destroyer()
        .build();

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
            hit.clone(),
            missed.clone(),
        ]),
        team_a: HashSet::from([player.id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Torpedo {
        ship_id,
        properties: TorpedoProperties {
            direction: Direction::West.into(),
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
        assert_eq!(inflicted_damage_by_ship.len(), 2);
        assert!(inflicted_damage_by_ship.contains_key(&hit.id()));
        assert!(inflicted_damage_by_ship.contains_key(&destroyed.id()));

        assert_eq!(inflicted_damage_at.len(), 3);
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 15, y: 20 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 14, y: 20 }));
        assert!(inflicted_damage_at.contains_key(&Coordinate { x: 13, y: 20 }));

        assert_eq!(ships_destroyed.len(), 1);
        assert!(ships_destroyed.contains(&destroyed.id()));

        assert_eq!(lost_vision_at.len(), 2);
        assert!(lost_vision_at.contains(&Coordinate { x: 15, y: 20 }));
        assert!(lost_vision_at.contains(&Coordinate { x: 15, y: 21 }));

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
    assert!(g.ships.get_by_id(&hit.id()).is_some());
    assert_eq!(g.ships.get_by_id(&hit.id()).unwrap().data().health, 1);
    assert_eq!(g.ships.get_by_id(&missed.id()).unwrap().data().health, 10);
}
