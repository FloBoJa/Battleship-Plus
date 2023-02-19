//noinspection DuplicatedCode
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
