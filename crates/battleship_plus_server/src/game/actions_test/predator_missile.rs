//noinspection DuplicatedCode
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
        cooldowns: Default::default(),
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
