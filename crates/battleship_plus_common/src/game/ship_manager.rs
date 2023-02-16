use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};

use rstar::{Envelope, PointDistance, RTree, RTreeObject, AABB};

use crate::game::ship::{ship_distance, Cooldown, GetShipID, Ship, ShipID};
use crate::game::ActionValidationError;
use crate::types::{Coordinate, MoveDirection, RotateDirection};

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
        ship_id: &ShipID,
        direction: MoveDirection,
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
                match ship.do_move(direction, bounds) {
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
