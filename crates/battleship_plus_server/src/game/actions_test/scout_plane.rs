//noinspection DuplicatedCode
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use battleship_plus_common::game::ship::{Cooldown, Orientation, Ship, ShipData};
use battleship_plus_common::game::ship_manager::ShipManager;
use battleship_plus_common::types::*;

use crate::game::actions::{Action, ActionResult};
use crate::game::data::{Game, Player, Turn};
use crate::game::states::GameState;

#[tokio::test]
async fn actions_scout_plane() {
    let player = Player::default();
    let ship_id = (player.id, 0);

    let ship = Ship::Carrier {
        balancing: Arc::from(CarrierBalancing {
            scout_plane_radius: 4,
            scout_plane_range: 10,
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
    let scouted = Ship::Destroyer {
        balancing: Arc::from(DestroyerBalancing {
            ..Default::default()
        }),
        data: ShipData {
            id: (42, 42),
            health: 10,
            pos_x: 10,
            pos_y: 10,
            orientation: Orientation::North,
        },
        cooldowns: Default::default(),
    };
    let partial_scouted = Ship::Destroyer {
        balancing: Arc::from(DestroyerBalancing {
            ..Default::default()
        }),
        data: ShipData {
            id: (42, 43),
            health: 10,
            pos_x: 14,
            pos_y: 14,
            orientation: Orientation::North,
        },
        cooldowns: Default::default(),
    };
    let hidden = Ship::Destroyer {
        balancing: Arc::from(DestroyerBalancing {
            ..Default::default()
        }),
        data: ShipData {
            id: (42, 44),
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
        ships: ShipManager::new_with_ships(vec![ship, scouted, partial_scouted, hidden]),
        team_a: HashSet::from([player.id]),
        team_b: HashSet::from([42]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let result = Action::ScoutPlane {
        ship_id,
        properties: ScoutPlaneProperties {
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
        assert!(inflicted_damage_by_ship.is_empty());
        assert!(inflicted_damage_at.is_empty());
        assert!(ships_destroyed.is_empty());
        assert!(lost_vision_at.is_empty());
        assert!(gain_vision_at.is_empty());
        assert!(temp_vision_at.contains(&Coordinate { x: 14, y: 14 }));
        assert!(temp_vision_at.contains(&Coordinate { x: 10, y: 10 }));
        assert!(temp_vision_at.contains(&Coordinate { x: 10, y: 11 }));
        assert_eq!(temp_vision_at.len(), 3);
    }

    let ship = g.ships.get_by_id(&ship_id).unwrap();
    assert_eq!(g.turn.as_ref().unwrap().action_points_left, 32);
    assert_eq!(ship.cool_downs().len(), 1);
    assert!(ship.cool_downs().contains(&Cooldown::Ability {
        remaining_rounds: 10
    }));
}
