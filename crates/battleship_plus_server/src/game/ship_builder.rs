use std::sync::Arc;

use battleship_plus_common::game::ship::{Cooldown, Orientation, Ship, ShipData, ShipID};
use battleship_plus_common::game::PlayerID;
use battleship_plus_common::types::{
    BattleshipBalancing, CarrierBalancing, CommonBalancing, Costs, CruiserBalancing,
    DestroyerBalancing, SubmarineBalancing,
};

pub trait ShipBuilder<T: ShipBuilder<T>> {
    fn base_builder(&mut self) -> &mut dyn ShipBuilder<T>;
    fn build(&mut self) -> Ship;

    fn owner(&mut self, player_id: PlayerID) -> &mut T {
        self.base_builder().owner(player_id)
    }
    fn number(&mut self, number: u32) -> &mut T {
        self.base_builder().number(number)
    }
    fn id(&mut self, ship_id: ShipID) -> &mut T {
        self.owner(ship_id.0).number(ship_id.1)
    }

    fn health(&mut self, health: u32) -> &mut T {
        self.base_builder().health(health)
    }

    fn position(&mut self, x: i32, y: i32) -> &mut T {
        self.base_builder().position(x, y)
    }

    fn orientation(&mut self, orientation: Orientation) -> &mut T {
        self.base_builder().orientation(orientation)
    }

    fn cooldown(&mut self, cooldown: Cooldown) -> &mut T {
        self.base_builder().cooldown(cooldown)
    }

    fn vision(&mut self, range: u32) -> &mut T {
        self.base_builder().vision(range)
    }

    fn cannon(&mut self, damage: u32, range: u32, action_points: u32, cooldown: u32) -> &mut T {
        self.base_builder()
            .cannon(damage, range, action_points, cooldown)
    }

    fn ability(&mut self, action_points: u32, cooldown: u32) -> &mut T {
        self.base_builder().ability(action_points, cooldown)
    }

    fn movement(
        &mut self,
        movement_action_points: u32,
        movement_cooldown: u32,
        rotation_action_points: u32,
        rotation_cooldown: u32,
    ) -> &mut T {
        self.base_builder().movement(
            movement_action_points,
            movement_cooldown,
            rotation_action_points,
            rotation_cooldown,
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct GeneralShipBuilder {
    common_balancing: CommonBalancing,
    data: ShipData,
    cooldowns: Vec<Cooldown>,
}

impl GeneralShipBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn carrier(&mut self) -> CarrierBuilder {
        CarrierBuilder {
            balancing: CarrierBalancing {
                common_balancing: Some(self.common_balancing.clone()),
                ..Default::default()
            },
            base: self.clone(),
        }
    }

    pub fn battleship(&mut self) -> BattleshipBuilder {
        BattleshipBuilder {
            balancing: BattleshipBalancing {
                common_balancing: Some(self.common_balancing.clone()),
                ..Default::default()
            },
            base: self.clone(),
        }
    }

    pub fn cruiser(&mut self) -> CruiserBuilder {
        CruiserBuilder {
            balancing: CruiserBalancing {
                common_balancing: Some(self.common_balancing.clone()),
                ..Default::default()
            },
            base: self.clone(),
        }
    }

    pub fn submarine(&mut self) -> SubmarineBuilder {
        SubmarineBuilder {
            balancing: SubmarineBalancing {
                common_balancing: Some(self.common_balancing.clone()),
                ..Default::default()
            },
            base: self.clone(),
        }
    }

    pub fn destroyer(&mut self) -> DestroyerBuilder {
        DestroyerBuilder {
            balancing: DestroyerBalancing {
                common_balancing: Some(self.common_balancing.clone()),
                ..Default::default()
            },
            base: self.clone(),
        }
    }
}

impl ShipBuilder<GeneralShipBuilder> for GeneralShipBuilder {
    fn base_builder(&mut self) -> &mut dyn ShipBuilder<GeneralShipBuilder> {
        unreachable!()
    }

    fn build(&mut self) -> Ship {
        panic!("unable to build a general type ship")
    }

    fn owner(&mut self, player_id: PlayerID) -> &mut Self {
        self.data.id.0 = player_id;
        self
    }
    fn number(&mut self, number: u32) -> &mut Self {
        self.data.id.1 = number;
        self
    }

    fn id(&mut self, ship_id: ShipID) -> &mut Self {
        self.owner(ship_id.0).number(ship_id.1)
    }

    fn health(&mut self, health: u32) -> &mut Self {
        self.data.health = health;
        self
    }

    fn position(&mut self, x: i32, y: i32) -> &mut Self {
        self.data.pos_x = x;
        self.data.pos_y = y;
        self
    }

    fn orientation(&mut self, orientation: Orientation) -> &mut Self {
        self.data.orientation = orientation;
        self
    }

    fn cooldown(&mut self, cooldown: Cooldown) -> &mut Self {
        self.cooldowns.push(cooldown);
        self
    }

    fn vision(&mut self, range: u32) -> &mut Self {
        self.common_balancing.vision_range = range;
        self
    }

    fn cannon(&mut self, damage: u32, range: u32, action_points: u32, cooldown: u32) -> &mut Self {
        self.common_balancing.shoot_damage = damage;
        self.common_balancing.shoot_range = range;
        self.common_balancing.shoot_costs = Some(Costs {
            cooldown,
            action_points,
        });
        self
    }

    fn ability(&mut self, action_points: u32, cooldown: u32) -> &mut Self {
        self.common_balancing.ability_costs = Some(Costs {
            cooldown,
            action_points,
        });
        self
    }

    fn movement(
        &mut self,
        movement_action_points: u32,
        movement_cooldown: u32,
        rotation_action_points: u32,
        rotation_cooldown: u32,
    ) -> &mut Self {
        self.common_balancing.movement_costs = Some(Costs {
            cooldown: movement_cooldown,
            action_points: movement_action_points,
        });
        self.common_balancing.rotation_costs = Some(Costs {
            cooldown: rotation_cooldown,
            action_points: rotation_action_points,
        });
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct CarrierBuilder {
    base: GeneralShipBuilder,
    balancing: CarrierBalancing,
}

impl ShipBuilder<GeneralShipBuilder> for CarrierBuilder {
    fn base_builder(&mut self) -> &mut dyn ShipBuilder<GeneralShipBuilder> {
        &mut self.base
    }

    fn build(&mut self) -> Ship {
        Ship::Carrier {
            data: self.base.data,
            cooldowns: self.base.cooldowns.clone(),
            balancing: Arc::new(CarrierBalancing {
                common_balancing: Some(self.base.common_balancing.clone()),
                ..self.balancing
            }),
        }
    }
}

impl CarrierBuilder {
    pub fn scout_plane(&mut self, range: u32, radius: u32) -> &mut Self {
        self.balancing.scout_plane_range = range;
        self.balancing.scout_plane_radius = radius;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct BattleshipBuilder {
    base: GeneralShipBuilder,
    balancing: BattleshipBalancing,
}

impl ShipBuilder<GeneralShipBuilder> for BattleshipBuilder {
    fn base_builder(&mut self) -> &mut dyn ShipBuilder<GeneralShipBuilder> {
        &mut self.base
    }

    fn build(&mut self) -> Ship {
        Ship::Battleship {
            data: self.base.data,
            cooldowns: self.base.cooldowns.clone(),
            balancing: Arc::new(BattleshipBalancing {
                common_balancing: Some(self.base.common_balancing.clone()),
                ..self.balancing
            }),
        }
    }
}

impl BattleshipBuilder {
    pub fn predator_missile(&mut self, range: u32, radius: u32, damage: u32) -> &mut Self {
        self.balancing.predator_missile_range = range;
        self.balancing.predator_missile_radius = radius;
        self.balancing.predator_missile_damage = damage;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct CruiserBuilder {
    base: GeneralShipBuilder,
    balancing: CruiserBalancing,
}

impl ShipBuilder<GeneralShipBuilder> for CruiserBuilder {
    fn base_builder(&mut self) -> &mut dyn ShipBuilder<GeneralShipBuilder> {
        &mut self.base
    }

    fn build(&mut self) -> Ship {
        Ship::Cruiser {
            data: self.base.data,
            cooldowns: self.base.cooldowns.clone(),
            balancing: Arc::new(CruiserBalancing {
                common_balancing: Some(self.base.common_balancing.clone()),
                ..self.balancing
            }),
        }
    }
}

impl CruiserBuilder {
    pub fn engine_boost(&mut self, distance: u32) -> &mut Self {
        self.balancing.engine_boost_distance = distance;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct SubmarineBuilder {
    base: GeneralShipBuilder,
    balancing: SubmarineBalancing,
}

impl ShipBuilder<GeneralShipBuilder> for SubmarineBuilder {
    fn base_builder(&mut self) -> &mut dyn ShipBuilder<GeneralShipBuilder> {
        &mut self.base
    }

    fn build(&mut self) -> Ship {
        Ship::Submarine {
            data: self.base.data,
            cooldowns: self.base.cooldowns.clone(),
            balancing: Arc::new(SubmarineBalancing {
                common_balancing: Some(self.base.common_balancing.clone()),
                ..self.balancing
            }),
        }
    }
}

impl SubmarineBuilder {
    pub fn torpedo(&mut self, range: u32, damage: u32) -> &mut Self {
        self.balancing.torpedo_range = range;
        self.balancing.torpedo_damage = damage;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct DestroyerBuilder {
    base: GeneralShipBuilder,
    balancing: DestroyerBalancing,
}

impl ShipBuilder<GeneralShipBuilder> for DestroyerBuilder {
    fn base_builder(&mut self) -> &mut dyn ShipBuilder<GeneralShipBuilder> {
        &mut self.base
    }

    fn build(&mut self) -> Ship {
        Ship::Destroyer {
            data: self.base.data,
            cooldowns: self.base.cooldowns.clone(),
            balancing: Arc::new(DestroyerBalancing {
                common_balancing: Some(self.base.common_balancing.clone()),
                ..self.balancing
            }),
        }
    }
}

impl DestroyerBuilder {
    pub fn multi_missile(&mut self, radius: u32, damage: u32) -> &mut Self {
        self.balancing.multi_missile_radius = radius;
        self.balancing.multi_missile_damage = damage;
        self
    }
}
