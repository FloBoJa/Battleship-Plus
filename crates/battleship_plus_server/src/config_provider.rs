use std::net::{SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub struct ServerConfig {
    pub game_address_v4: SocketAddrV4,
    pub game_address_v6: SocketAddrV6,
    pub enable_announcements_v4: bool,
    pub enable_announcements_v6: bool,
    pub announcement_address_v4: SocketAddrV4,
    pub announcement_address_v6: SocketAddrV6,
    pub announcement_interval: Duration,
}

pub trait ConfigProvider {
    fn game_config(&self) -> Arc<battleship_plus_common::types::Config>;
    fn server_config(&self) -> Arc<ServerConfig>;
}

mod default {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
    use std::sync::Arc;
    use std::time::Duration;

    use battleship_plus_common::types::{BattleshipBalancing, CarrierBalancing, CommonBalancing, Config, Costs, CruiserBalancing, DestroyerBalancing, ShipType, SubmarineBalancing};

    use crate::config_provider::{ConfigProvider, ServerConfig};

    fn costs(cooldown: u32, action_points: u32) -> Option<Costs> {
        Some(Costs {
            cooldown,
            action_points,
        })
    }

    fn default_ship_set() -> Vec<i32> {
        vec![
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
        ]
    }

    #[derive(Copy, Clone, Debug)]
    pub struct DefaultGameConfig;

    impl ConfigProvider for DefaultGameConfig {
        fn game_config(&self) -> Arc<Config> {
            Arc::from(Config {
                server_name: String::from("Battleship PLUS powered by Rust 🦀"),
                carrier_balancing: Some(CarrierBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 2),
                        shoot_range: 6,
                        shoot_damage: 20,
                        movement_costs: costs(0, 1),
                        movement_speed: 1,
                        ability_costs: costs(2, 5),
                        vision_range: 8,
                        initial_health: 300,
                    }),
                    scout_plane_range: 32,
                    scout_plane_radius: 8,
                }),
                battleship_balancing: Some(BattleshipBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 2),
                        shoot_range: 10,
                        shoot_damage: 33,
                        movement_costs: costs(0, 1),
                        movement_speed: 1,
                        ability_costs: costs(2, 5),
                        vision_range: 12,
                        initial_health: 200,
                    }),
                    predator_missile_range: 20,
                    predator_missile_radius: 6,
                    predator_missile_damage: 34,
                }),
                cruiser_balancing: Some(CruiserBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 2),
                        shoot_range: 8,
                        shoot_damage: 25,
                        movement_costs: costs(0, 1),
                        movement_speed: 2,
                        ability_costs: costs(2, 5),
                        vision_range: 10,
                        initial_health: 100,
                    }),
                    engine_boost_distance: 8,
                }),
                submarine_balancing: Some(SubmarineBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(1, 5),
                        shoot_range: 16,
                        shoot_damage: 33,
                        movement_costs: costs(0, 1),
                        movement_speed: 1,
                        ability_costs: costs(2, 8),
                        vision_range: 32,
                        initial_health: 100,
                    }),
                    torpedo_range: 32,
                    torpedo_damage: 50,
                }),
                destroyer_balancing: Some(DestroyerBalancing {
                    common_balancing: Some(CommonBalancing {
                        shoot_costs: costs(0, 1),
                        shoot_range: 12,
                        shoot_damage: 33,
                        movement_costs: costs(0, 2),
                        movement_speed: 1,
                        ability_costs: costs(2, 5),
                        vision_range: 24,
                        initial_health: 100,
                    }),
                    multi_missile_radius: 16,
                    multi_missile_damage: 44,
                }),
                ship_set_team_a: default_ship_set(),
                ship_set_team_b: default_ship_set(),
                board_size: 128,
                action_point_gain: 1,
                team_size_a: 2,
                team_size_b: 2,
            })
        }

        fn server_config(&self) -> Arc<ServerConfig> {
            Arc::from(ServerConfig {
                game_address_v4: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 30303),
                game_address_v6: SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 30303, 0, 0),
                enable_announcements_v4: true,
                enable_announcements_v6: true,
                announcement_address_v4: SocketAddrV4::new(Ipv4Addr::BROADCAST, 30303),
                announcement_address_v6: SocketAddrV6::new(Ipv6Addr::new(0xff03, 0, 0, 0, 0, 0, 0, 1), 30303, 0, 0),
                announcement_interval: Duration::from_secs(5),
            })
        }
    }
}

pub fn default_config_provider() -> Arc<dyn ConfigProvider> {
    Arc::from(default::DefaultGameConfig)
}