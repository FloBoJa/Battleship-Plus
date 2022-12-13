trait ConfigProvider {
    fn get_config(&self) -> battleship_plus_common::messages::Config;
}

mod default {
    use battleship_plus_common::messages::{BattleshipBalancing, CarrierBalancing, CommonBalancing, Config, Costs, CruiserBalancing, DestroyerBalancing, ShipType, SubmarineBalancing, TeamShipSet};

    use crate::config_provider::ConfigProvider;

    fn costs(cooldown: i32, action_points: i32) -> Option<Costs> {
        Some(Costs {
            cooldown,
            action_points,
        })
    }

    fn default_ship_set() -> Option<TeamShipSet> {
        Some(TeamShipSet {
            ships: vec![
                ShipType::Carrier as i32,
                ShipType::Battleship as i32,
                ShipType::Battleship as i32,
                ShipType::Cruiser as i32,
                ShipType::Cruiser as i32,
                ShipType::Cruiser as i32,
                ShipType::Submarine as i32,
                ShipType::Submarine as i32,
                ShipType::Submarine as i32,
                ShipType::Submarine as i32,
                ShipType::Destroyer as i32,
                ShipType::Destroyer as i32,
            ],
        })
    }

    pub struct DefaultGameConfig;

    impl ConfigProvider for DefaultGameConfig {
        fn get_config(&self) -> Config {
            Config {
                server_name: String::from("Battleship PLUS powered by Rust 🦀"),
                carrier_balancing: Some(CarrierBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 2),
                        shoot_range: 6,
                        movement_costs: costs(0, 1),
                        movement_speed: 1,
                        ability_costs: costs(2, 5),
                        vision_range: 8,
                        initial_health: 300,
                    }),
                    range: 32,
                    radius: 8,
                }),
                battleship_balancing: Some(BattleshipBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 2),
                        shoot_range: 10,
                        movement_costs: costs(0, 1),
                        movement_speed: 1,
                        ability_costs: costs(2, 5),
                        vision_range: 12,
                        initial_health: 200,
                    }),
                    radius: 16,
                    damage: 34,
                }),
                cruiser_balancing: Some(CruiserBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 2),
                        shoot_range: 8,
                        movement_costs: costs(0, 1),
                        movement_speed: 2,
                        ability_costs: costs(2, 5),
                        vision_range: 10,
                        initial_health: 100,
                    }),
                    distance: 8,
                }),
                submarine_balancing: Some(SubmarineBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(2, 5),
                        shoot_range: 16,
                        movement_costs: costs(0, 1),
                        movement_speed: 1,
                        ability_costs: costs(2, 8),
                        vision_range: 32,
                        initial_health: 100,
                    }),
                    range: 32,
                    damage: 50,
                }),
                destroyer_balancing: Some(DestroyerBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 1),
                        shoot_range: 12,
                        movement_costs: costs(0, 2),
                        movement_speed: 1,
                        ability_costs: costs(2, 5),
                        vision_range: 24,
                        initial_health: 100,
                    }),
                    range: 20,
                    radius: 6,
                    damage: 34,
                }),
                ship_set_team_a: default_ship_set(),
                ship_set_team_b: default_ship_set(),
                board_size: 128,
                action_point_gain: 1,
                team_size_a: 2,
                team_size_b: 2,
            }
        }
    }
}

pub fn default_config_provider() -> Box<dyn ConfigProvider> {
    Box::new(default::DefaultGameConfig {})
}