use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use rstar::{Envelope, PointDistance, RTree, RTreeObject, AABB};

use crate::game::ship::{ship_distance, Cooldown, GetShipID, Ship, ShipID};
use crate::game::{ActionValidationError, PlayerID};
use crate::types::{Coordinate, CruiserBalancing, Direction, MoveDirection, RotateDirection};

#[derive(Debug, Clone, Default)]
pub struct ShipManager {
    ships: HashMap<ShipID, Ship>,
    ships_geo_lookup: RTree<ShipTreeNode>,
}

impl From<ShipManager> for HashMap<ShipID, Ship> {
    fn from(ship_manager: ShipManager) -> Self {
        ship_manager.ships
    }
}

impl ShipManager {
    pub fn new() -> ShipManager {
        ShipManager {
            ships: Default::default(),
            ships_geo_lookup: Default::default(),
        }
    }

    pub fn new_with_ships(ships: Vec<Ship>) -> ShipManager {
        ShipManager {
            ships: HashMap::from_iter(ships.iter().cloned().map(|ship| (ship.id(), ship))),
            ships_geo_lookup: RTree::bulk_load(
                ships
                    .iter()
                    .map(|ship| ShipTreeNode::new(ship.id(), ship.envelope()))
                    .collect(),
            ),
        }
    }

    pub fn get_ship_parts_seen_by(&self, ships_ids: &[ShipID]) -> Vec<Coordinate> {
        ships_ids
            .iter()
            .flat_map(|ship_id| {
                if let Some(ship) = self.get_by_id(ship_id) {
                    let vision_envelope = ship.vision_envelope();

                    self.ships_geo_lookup
                        .locate_in_envelope_intersecting(&vision_envelope)
                        .filter(|ship| ship.ship_id != *ship_id)
                        .flat_map(|ship| envelope_to_points(ship.envelope))
                        .filter(move |Coordinate { x, y }| {
                            vision_envelope.contains_point(&[*x as i32, *y as i32])
                        })
                        .collect()
                } else {
                    vec![]
                }
            })
            .collect()
    }

    pub fn iter_ships(&self) -> impl Iterator<Item = (&ShipID, &Ship)> {
        self.ships.iter()
    }

    pub fn get_by_id(&self, ship_id: &ShipID) -> Option<&Ship> {
        self.ships.get(ship_id)
    }

    pub fn get_by_position(&self, position: Coordinate) -> Option<&Ship> {
        let position = [position.x as i32, position.y as i32];
        let ship = self.ships_geo_lookup.locate_at_point(&position);
        if let Some(ShipTreeNode { ship_id, .. }) = ship {
            self.get_by_id(ship_id)
        } else {
            None
        }
    }

    pub fn destroy_colliding_ships_in_envelope(
        &mut self,
        envelope: &AABB<[i32; 2]>,
    ) -> Option<Vec<Ship>> {
        let colliding_ships: Vec<_> = self
            .ships_geo_lookup
            .locate_in_envelope_intersecting(envelope)
            .map(|node| node.ship_id)
            .collect();

        if colliding_ships.len() > 1 {
            // there is more than one ship in the new position of the moved ship
            // therefore there has been a collision

            let destroyed_ships: Vec<_> = colliding_ships
                .iter()
                .map(|id| self.ships.remove(id).unwrap())
                .collect();
            destroyed_ships.iter().for_each(|ship| {
                let _ = self.ships_geo_lookup.remove(&ShipTreeNode::from(&ship));
            });

            Some(destroyed_ships)
        } else {
            None
        }
    }

    pub fn destroy_ships(&mut self, ships: Vec<&Ship>) {
        ships.iter().for_each(|ship| {
            self.ships.remove(&ship.id());
            self.ships_geo_lookup.remove(&ShipTreeNode::from(ship));
        });
    }

    pub fn place_ship(&mut self, ship_id: ShipID, ship: Ship) -> Result<(), ShipPlacementError> {
        if self.ships.get(&ship_id).is_some() {
            return Err(ShipPlacementError::IdAlreadyPlaced);
        }

        if self
            .ships_geo_lookup
            .locate_in_envelope_intersecting(&ship.envelope())
            .any(|_| true)
        {
            return Err(ShipPlacementError::Collision);
        }

        self.ships_geo_lookup.insert(ShipTreeNode::from(&ship));
        self.ships.insert(ship_id, ship);

        Ok(())
    }

    pub fn attack_with_ship(
        &mut self,
        action_points: &mut u32,
        ship_id: &ShipID,
        target: &Coordinate,
        bounds: &AABB<[i32; 2]>,
    ) -> Result<ShotResult, ActionValidationError> {
        let target = [target.x as i32, target.y as i32];
        if !bounds.contains_point(&target) {
            // shot out of map
            return Err(ActionValidationError::OutOfMap);
        }

        let ship = match self.ships.get_mut(ship_id) {
            Some(ship) => ship,
            None => return Err(ActionValidationError::NonExistentShip { id: *ship_id }),
        };

        // cooldown check
        let remaining_rounds = ship.cool_downs().iter().find_map(|cd| match cd {
            Cooldown::Cannon { remaining_rounds } => Some(*remaining_rounds),
            _ => None,
        });
        if let Some(remaining_rounds) = remaining_rounds {
            return Err(ActionValidationError::Cooldown { remaining_rounds });
        }

        // check action points of player
        let balancing = ship.common_balancing();
        let costs = balancing.shoot_costs.unwrap();
        if *action_points < costs.action_points {
            return Err(ActionValidationError::InsufficientPoints {
                required: costs.action_points,
            });
        }

        // check range
        if ship.distance_2(&target) > balancing.shoot_range as i32 {
            return Err(ActionValidationError::Unreachable);
        }

        // enforce costs
        *action_points -= costs.action_points;
        if costs.cooldown > 0 {
            ship.cool_downs_mut().push(Cooldown::Cannon {
                remaining_rounds: costs.cooldown,
            });
        }

        Ok(self
            .ships_geo_lookup
            .locate_at_point(&target)
            .cloned()
            .map_or(ShotResult::Miss, |ship_node| {
                if self
                    .ships
                    .get_mut(&ship_node.ship_id)
                    .unwrap()
                    .apply_damage(balancing.shoot_damage)
                {
                    // ship got destroyed
                    let destroyed_node = self
                        .ships_geo_lookup
                        .remove(&ShipTreeNode::from(
                            &self.ships.remove(&ship_node.ship_id).unwrap(),
                        ))
                        .unwrap();

                    let l = destroyed_node.envelope.lower();
                    let u = destroyed_node.envelope.upper();
                    let parts = (l[0]..=u[0])
                        .flat_map(move |x| {
                            (l[1]..=u[1]).map(move |y| Coordinate {
                                x: x as u32,
                                y: y as u32,
                            })
                        })
                        .collect();

                    ShotResult::Destroyed(ship_node.ship_id, balancing.shoot_damage, parts)
                } else {
                    ShotResult::Hit(ship_node.ship_id, balancing.shoot_damage)
                }
            }))
    }

    /// Checks for all conditions required for a ship movement and executes a move.
    /// Returns the area that has to be checked for collision.
    pub fn move_ship(
        &mut self,
        action_points: &mut u32,
        handle_costs: bool,
        ship_id: &ShipID,
        direction: MoveDirection,
        bounds: &AABB<[i32; 2]>,
    ) -> Result<AABB<[i32; 2]>, ActionValidationError> {
        self.mutate_ship_by_id(
            ship_id,
            true,
            ActionValidationError::NonExistentShip { id: *ship_id },
            |ship| {
                let costs;
                if handle_costs {
                    costs = ship.common_balancing().movement_costs;
                    let action_point_costs = costs.as_ref().unwrap().action_points;

                    // cooldown check
                    let remaining_rounds = ship.cool_downs().iter().find_map(|cd| match cd {
                        Cooldown::Movement { remaining_rounds } => Some(*remaining_rounds),
                        _ => None,
                    });
                    if let Some(remaining_rounds) = remaining_rounds {
                        return Err(ActionValidationError::Cooldown { remaining_rounds });
                    }

                    // check action points
                    if *action_points < action_point_costs {
                        return Err(ActionValidationError::InsufficientPoints {
                            required: action_point_costs,
                        });
                    }
                } else {
                    costs = None
                }

                match ship.do_move(direction, bounds) {
                    Err(e) => Err(e),
                    Ok(new_position) => {
                        // enforce costs
                        let new_position = new_position;

                        if let Some(costs) = costs {
                            *action_points -= costs.action_points;
                            if costs.cooldown > 0 {
                                ship.cool_downs_mut().push(Cooldown::Movement {
                                    remaining_rounds: costs.cooldown,
                                });
                            }
                        }

                        Ok(new_position)
                    }
                }
            },
        )
    }

    pub fn engine_boost<R, F>(
        &mut self,
        action_points: &mut u32,
        ship_id: &ShipID,
        do_movement: F,
    ) -> Result<R, ActionValidationError>
    where
        F: FnOnce(&mut ShipManager, Arc<CruiserBalancing>) -> Result<R, ActionValidationError>,
    {
        let balancing;
        let costs;
        {
            let ship = self.ships.get(ship_id).cloned();
            let cruiser = match self.ships.get_mut(ship_id) {
                None => return Err(ActionValidationError::NonExistentShip { id: *ship_id }),
                Some(Ship::Cruiser {
                    balancing,
                    cooldowns,
                    data,
                }) => (balancing, cooldowns, data),
                _ => return Err(ActionValidationError::InvalidShipType),
            };
            let ship = ship.unwrap();

            // check action points of player
            balancing = cruiser.0.clone();
            costs = balancing
                .common_balancing
                .as_ref()
                .unwrap()
                .ability_costs
                .as_ref()
                .unwrap();

            // cooldown check
            let remaining_rounds = ship.cool_downs().iter().find_map(|cd| match cd {
                Cooldown::Ability { remaining_rounds } => Some(*remaining_rounds),
                _ => None,
            });
            if let Some(remaining_rounds) = remaining_rounds {
                return Err(ActionValidationError::Cooldown { remaining_rounds });
            }

            // check action points
            if *action_points < costs.action_points {
                return Err(ActionValidationError::InsufficientPoints {
                    required: costs.action_points,
                });
            }
        }

        let result = (do_movement)(self, balancing.clone())?;

        // enforce action point costs
        *action_points -= balancing
            .as_ref()
            .common_balancing
            .as_ref()
            .unwrap()
            .ability_costs
            .as_ref()
            .unwrap()
            .action_points;

        match self.ships.get_mut(ship_id) {
            // ship got destroyed
            None => Ok(result),
            Some(Ship::Cruiser { cooldowns, .. }) => {
                // enforce cooldown costs
                if costs.cooldown > 0 {
                    cooldowns.push(Cooldown::Ability {
                        remaining_rounds: costs.cooldown,
                    });
                }

                Ok(result)
            }
            _ => unreachable!(),
        }
    }

    /// Checks for all conditions required for a ship rotation and executes a rotation.
    /// Returns the area that has to be checked for collision.
    pub fn rotate_ship(
        &mut self,
        action_points: &mut u32,
        ship_id: &ShipID,
        direction: RotateDirection,
        bounds: &AABB<[i32; 2]>,
    ) -> Result<AABB<[i32; 2]>, ActionValidationError> {
        self.mutate_ship_by_id(
            ship_id,
            true,
            ActionValidationError::NonExistentShip { id: *ship_id },
            |ship| {
                // cooldown check
                let remaining_rounds = ship.cool_downs().iter().find_map(|cd| match cd {
                    Cooldown::Movement { remaining_rounds } => Some(*remaining_rounds),
                    _ => None,
                });
                if let Some(remaining_rounds) = remaining_rounds {
                    return Err(ActionValidationError::Cooldown { remaining_rounds });
                }

                // check action points of player
                let costs = ship.common_balancing().movement_costs.unwrap();
                if *action_points < costs.action_points {
                    return Err(ActionValidationError::InsufficientPoints {
                        required: costs.action_points,
                    });
                }

                //let old_envelope = ship.envelope();
                match ship.do_rotation(direction, bounds) {
                    Err(e) => Err(e),
                    Ok(new_position) => {
                        // enforce costs
                        let new_position = new_position;
                        *action_points -= costs.action_points;
                        if costs.cooldown > 0 {
                            ship.cool_downs_mut().push(Cooldown::Movement {
                                remaining_rounds: costs.cooldown,
                            });
                        }

                        Ok(new_position)
                    }
                }
            },
        )
    }

    fn mutate_ship_by_id<F, T, E>(
        &mut self,
        ship_id: &ShipID,
        invalidates_tree: bool,
        non_existent_ship_error: E,
        mutation: F,
    ) -> Result<T, E>
    where
        F: FnOnce(&mut Ship) -> Result<T, E>,
    {
        match self.ships.get_mut(ship_id) {
            None => Err(non_existent_ship_error),
            Some(ship) => {
                let old_node = ShipTreeNode::from(&ship);
                let res = (mutation)(ship);

                if res.is_ok() && invalidates_tree {
                    let _ = self.ships_geo_lookup.remove(&old_node);
                    self.ships_geo_lookup.insert(ShipTreeNode::from(&ship));
                }

                res
            }
        }
    }

    pub fn torpedo(
        &mut self,
        action_points: &mut u32,
        ship_id: &ShipID,
        direction: Direction,
    ) -> Result<AreaOfEffect, ActionValidationError> {
        let ship = self.ships.get(ship_id).cloned();
        let submarine = match self.ships.get_mut(ship_id) {
            None => return Err(ActionValidationError::NonExistentShip { id: *ship_id }),
            Some(Ship::Submarine {
                balancing,
                cooldowns: cool_downs,
                data,
            }) => (balancing, cool_downs, data),
            _ => return Err(ActionValidationError::InvalidShipType),
        };
        let ship = ship.unwrap();

        // cooldown check
        let remaining_rounds = submarine.1.iter().find_map(|cd| match cd {
            Cooldown::Ability { remaining_rounds } => Some(*remaining_rounds),
            _ => None,
        });
        if let Some(remaining_rounds) = remaining_rounds {
            return Err(ActionValidationError::Cooldown { remaining_rounds });
        }

        // check action points of player
        let balancing = submarine.0.clone();
        let costs = balancing
            .common_balancing
            .as_ref()
            .unwrap()
            .ability_costs
            .as_ref()
            .unwrap();
        if *action_points < costs.action_points {
            return Err(ActionValidationError::InsufficientPoints {
                required: costs.action_points,
            });
        }

        // enforce costs
        *action_points -= costs.action_points;
        if costs.cooldown > 0 {
            submarine.1.push(Cooldown::Ability {
                remaining_rounds: costs.cooldown,
            });
        }

        let origin = [submarine.2.pos_x, submarine.2.pos_y];
        let origin_offset = if direction == submarine.2.orientation.into() {
            ship.len() - 1
        } else {
            0
        };
        let trajectory = match direction {
            Direction::North => AABB::from_corners(
                [origin[0], origin[1] + origin_offset],
                [
                    origin[0],
                    origin[1] + origin_offset + balancing.torpedo_range as i32,
                ],
            ),
            Direction::East => AABB::from_corners(
                [origin[0] + origin_offset, origin[1]],
                [
                    origin[0] + origin_offset + balancing.torpedo_range as i32,
                    origin[1],
                ],
            ),
            Direction::South => AABB::from_corners(
                [origin[0], origin[1] - origin_offset],
                [
                    origin[0],
                    origin[1] - origin_offset - balancing.torpedo_range as i32,
                ],
            ),
            Direction::West => AABB::from_corners(
                [origin[0] - origin_offset, origin[1]],
                [
                    origin[0] - origin_offset - balancing.torpedo_range as i32,
                    origin[1],
                ],
            ),
        };

        let hit_ships = self
            .ships_geo_lookup
            .locate_in_envelope_intersecting(&trajectory)
            .filter_map(|node| {
                if node.ship_id != *ship_id {
                    Some(node.ship_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let destroyed_ships = hit_ships
            .iter()
            .filter_map(|id| {
                let ship = self.ships.get_mut(id).unwrap();
                if ship.apply_damage(balancing.torpedo_damage) {
                    self.ships.remove(id)
                } else {
                    None
                }
            })
            .collect();

        Ok(AreaOfEffect {
            hit_ships: hit_ships
                .iter()
                .filter_map(|id| self.ships.get(id).cloned())
                .collect::<Vec<_>>(),
            destroyed_ships,
            damage_per_hit: balancing.torpedo_damage,
            area: trajectory,
        })
    }

    pub fn predator_missile(
        &mut self,
        action_points: &mut u32,
        ship_id: &ShipID,
        center: &[i32; 2],
        bounds: &AABB<[i32; 2]>,
    ) -> Result<AreaOfEffect, ActionValidationError> {
        if !bounds.contains_point(center) {
            // missile out of map
            return Err(ActionValidationError::OutOfMap);
        }

        let ship = self.ships.get(ship_id).cloned();
        let battleship = match self.ships.get_mut(ship_id) {
            None => return Err(ActionValidationError::NonExistentShip { id: *ship_id }),
            Some(Ship::Battleship {
                balancing,
                cooldowns: cool_downs,
                data,
            }) => (balancing, cool_downs, data),
            _ => return Err(ActionValidationError::InvalidShipType),
        };
        let ship = ship.unwrap();

        // cooldown check
        let remaining_rounds = battleship.1.iter().find_map(|cd| match cd {
            Cooldown::Ability { remaining_rounds } => Some(*remaining_rounds),
            _ => None,
        });
        if let Some(remaining_rounds) = remaining_rounds {
            return Err(ActionValidationError::Cooldown { remaining_rounds });
        }

        // check action points of player
        let balancing = battleship.0.clone();
        let costs = balancing
            .common_balancing
            .as_ref()
            .unwrap()
            .ability_costs
            .as_ref()
            .unwrap();
        if *action_points < costs.action_points {
            return Err(ActionValidationError::InsufficientPoints {
                required: costs.action_points,
            });
        }

        // check range
        if ship.distance_2(center) > balancing.predator_missile_range as i32 {
            return Err(ActionValidationError::Unreachable);
        }

        // enforce costs
        *action_points -= costs.action_points;
        if costs.cooldown > 0 {
            battleship.1.push(Cooldown::Ability {
                remaining_rounds: costs.cooldown,
            });
        }

        let blast_area = AABB::from_corners(
            [
                center[0] + balancing.predator_missile_radius as i32,
                center[1] + balancing.predator_missile_radius as i32,
            ],
            [
                center[0] - balancing.predator_missile_radius as i32,
                center[1] - balancing.predator_missile_radius as i32,
            ],
        );

        let hit_ships = self
            .ships_geo_lookup
            .locate_in_envelope_intersecting(&blast_area)
            .map(|node| node.ship_id)
            .collect::<Vec<_>>();

        let destroyed_ships = hit_ships
            .iter()
            .filter_map(|id| {
                let ship = self.ships.get_mut(id).unwrap();
                if ship.apply_damage(balancing.predator_missile_damage) {
                    self.ships.remove(id)
                } else {
                    None
                }
            })
            .collect();

        Ok(AreaOfEffect {
            hit_ships: hit_ships
                .iter()
                .filter_map(|id| self.ships.get(id).cloned())
                .collect::<Vec<_>>(),
            destroyed_ships,
            damage_per_hit: balancing.predator_missile_damage,
            area: blast_area,
        })
    }

    pub fn scout_plane(
        &mut self,
        action_points: &mut u32,
        ship_id: &ShipID,
        center: &[i32; 2],
        bounds: &AABB<[i32; 2]>,
        enemy_team: HashSet<PlayerID>,
    ) -> Result<HashSet<Coordinate>, ActionValidationError> {
        if !bounds.contains_point(center) {
            // scout plane out of map
            return Err(ActionValidationError::OutOfMap);
        }

        let ship = self.ships.get(ship_id).cloned();
        let carrier = match self.ships.get_mut(ship_id) {
            None => return Err(ActionValidationError::NonExistentShip { id: *ship_id }),
            Some(Ship::Carrier {
                balancing,
                cooldowns: cool_downs,
                data,
            }) => (balancing, cool_downs, data),
            _ => return Err(ActionValidationError::InvalidShipType),
        };
        let ship = ship.unwrap();

        // cooldown check
        let remaining_rounds = carrier.1.iter().find_map(|cd| match cd {
            Cooldown::Ability { remaining_rounds } => Some(*remaining_rounds),
            _ => None,
        });
        if let Some(remaining_rounds) = remaining_rounds {
            return Err(ActionValidationError::Cooldown { remaining_rounds });
        }

        // check action points of player
        let balancing = carrier.0;
        let costs = balancing
            .common_balancing
            .as_ref()
            .unwrap()
            .ability_costs
            .as_ref()
            .unwrap();
        if *action_points < costs.action_points {
            return Err(ActionValidationError::InsufficientPoints {
                required: costs.action_points,
            });
        }

        // check range
        if ship.distance_2(center) > balancing.scout_plane_range as i32 {
            return Err(ActionValidationError::Unreachable);
        }

        // enforce costs
        *action_points -= costs.action_points;
        if costs.cooldown > 0 {
            carrier.1.push(Cooldown::Ability {
                remaining_rounds: costs.cooldown,
            });
        }

        let scout_area = AABB::from_corners(
            [
                center[0] + balancing.scout_plane_radius as i32,
                center[1] + balancing.scout_plane_radius as i32,
            ],
            [
                center[0] - balancing.scout_plane_radius as i32,
                center[1] - balancing.scout_plane_radius as i32,
            ],
        );

        Ok(self
            .ships_geo_lookup
            .locate_in_envelope_intersecting(&scout_area)
            .filter(|node| enemy_team.contains(&node.ship_id.0))
            .flat_map(|node| envelope_to_points(node.envelope))
            .filter(|p| scout_area.contains_point(&[p.x as i32, p.y as i32]))
            .collect())
    }

    pub fn multi_missile(
        &mut self,
        action_points: &mut u32,
        bounds: &AABB<[i32; 2]>,
        ship_id: &ShipID,
        positions: Vec<Coordinate>,
    ) -> Result<Vec<AreaOfEffect>, ActionValidationError> {
        if positions
            .iter()
            .any(|p| !bounds.contains_point(&[p.x as i32, p.y as i32]))
        {
            // at least one shot out of map
            return Err(ActionValidationError::OutOfMap);
        }

        let destroyer = match self.ships.get_mut(ship_id) {
            None => return Err(ActionValidationError::NonExistentShip { id: *ship_id }),
            Some(Ship::Destroyer {
                balancing,
                cooldowns: cool_downs,
                data,
            }) => (balancing, cool_downs, data),
            _ => return Err(ActionValidationError::InvalidShipType),
        };

        // cooldown check
        let remaining_rounds = destroyer.1.iter().find_map(|cd| match cd {
            Cooldown::Ability { remaining_rounds } => Some(*remaining_rounds),
            _ => None,
        });
        if let Some(remaining_rounds) = remaining_rounds {
            return Err(ActionValidationError::Cooldown { remaining_rounds });
        }

        // check action points of player
        let balancing = destroyer.0.clone();
        let costs = balancing
            .common_balancing
            .as_ref()
            .unwrap()
            .ability_costs
            .as_ref()
            .unwrap();
        if *action_points < costs.action_points {
            return Err(ActionValidationError::InsufficientPoints {
                required: costs.action_points,
            });
        }

        // enforce costs
        *action_points -= costs.action_points;
        if costs.cooldown > 0 {
            destroyer.1.push(Cooldown::Ability {
                remaining_rounds: costs.cooldown,
            });
        }

        Ok(positions
            .iter()
            .map(|p| {
                let blast_area = AABB::from_corners(
                    [
                        p.x as i32 - balancing.multi_missile_radius as i32,
                        p.y as i32 - balancing.multi_missile_radius as i32,
                    ],
                    [
                        p.x as i32 + balancing.multi_missile_radius as i32,
                        p.y as i32 + balancing.multi_missile_radius as i32,
                    ],
                );

                (
                    self.ships_geo_lookup
                        .locate_in_envelope_intersecting(&blast_area)
                        .map(|node| node.ship_id)
                        .collect::<HashSet<_>>(),
                    blast_area,
                )
            })
            .map(|(hit_ships, blast_area)| {
                let destroyed_ships = hit_ships
                    .iter()
                    .filter_map(|id| match self.ships.get_mut(id) {
                        Some(ship) => {
                            if ship.apply_damage(balancing.multi_missile_damage) {
                                self.ships.remove(id)
                            } else {
                                None
                            }
                        }
                        None => None,
                    })
                    .collect();

                AreaOfEffect {
                    hit_ships: hit_ships
                        .iter()
                        .filter_map(|id| self.ships.get(id).cloned())
                        .collect::<Vec<_>>(),
                    destroyed_ships,
                    damage_per_hit: balancing.multi_missile_damage,
                    area: blast_area,
                }
            })
            .collect::<Vec<_>>())
    }
}

#[derive(Debug, Clone)]
pub enum ShotResult {
    Miss,
    Hit(ShipID, u32),
    Destroyed(ShipID, u32, HashSet<Coordinate>),
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct ShipTreeNode {
    envelope: AABB<[i32; 2]>,
    ship_id: ShipID,
}

impl ShipTreeNode {
    pub fn new(ship_id: ShipID, envelope: AABB<[i32; 2]>) -> ShipTreeNode {
        ShipTreeNode { ship_id, envelope }
    }
}

impl From<&&mut Ship> for ShipTreeNode {
    fn from(ship: &&mut Ship) -> Self {
        ShipTreeNode::new(ship.id(), ship.envelope())
    }
}

impl From<&&Ship> for ShipTreeNode {
    fn from(ship: &&Ship) -> Self {
        ShipTreeNode::new(ship.id(), ship.envelope())
    }
}

impl From<&Ship> for ShipTreeNode {
    fn from(ship: &Ship) -> Self {
        ShipTreeNode::new(ship.id(), ship.envelope())
    }
}

impl RTreeObject for ShipTreeNode {
    type Envelope = AABB<[i32; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

impl PointDistance for ShipTreeNode {
    fn distance_2(&self, point: &[i32; 2]) -> i32 {
        ship_distance(&self.envelope(), point)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ShipPlacementError {
    Collision,
    IdAlreadyPlaced,
    InvalidShipNumber,
    InvalidShipSet,
    InvalidShipType,
    InvalidShipDirection,
    InvalidShipPosition,
    PlayerNotInGame,
    ShipOutOfQuadrant,
    PlayerHasAlreadyPlacedShips,
}

impl Display for ShipPlacementError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ShipPlacementError::Collision => "Ships colliding",
            ShipPlacementError::IdAlreadyPlaced => "Ship with the same id already exists",
            ShipPlacementError::InvalidShipNumber => "Ship number is not valid",
            ShipPlacementError::InvalidShipSet => "provided ship set is invalid",
            ShipPlacementError::InvalidShipType => "Ship type is invalid",
            ShipPlacementError::InvalidShipDirection => "Ship direction is invalid",
            ShipPlacementError::InvalidShipPosition => "Ship position is invalid",
            ShipPlacementError::PlayerNotInGame => "Player is not in game",
            ShipPlacementError::ShipOutOfQuadrant => "Ship is placed outside the provided quadrant",
            ShipPlacementError::PlayerHasAlreadyPlacedShips => {
                "A player can only place their ships once"
            }
        })
    }
}

pub fn envelope_to_points(envelope: AABB<[i32; 2]>) -> impl Iterator<Item = Coordinate> + 'static {
    (envelope.lower()[0]..=envelope.upper()[0]).flat_map(move |x| {
        (envelope.lower()[1]..=envelope.upper()[1]).map(move |y| Coordinate {
            x: x as u32,
            y: y as u32,
        })
    })
}

#[derive(Debug, Clone)]
pub struct AreaOfEffect {
    pub hit_ships: Vec<Ship>,
    pub destroyed_ships: Vec<Ship>,
    pub damage_per_hit: u32,
    pub area: AABB<[i32; 2]>,
}
