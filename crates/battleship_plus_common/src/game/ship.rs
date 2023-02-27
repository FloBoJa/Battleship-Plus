use std::cmp::max;
use std::sync::Arc;

use rstar::{Envelope, PointDistance, RTreeObject, SelectionFunction, AABB};

use crate::game::{ActionValidationError, PlayerID};
use crate::types::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Ship {
    Carrier {
        balancing: Arc<CarrierBalancing>,
        data: ShipData,
        cooldowns: Vec<Cooldown>,
    },
    Battleship {
        balancing: Arc<BattleshipBalancing>,
        data: ShipData,
        cooldowns: Vec<Cooldown>,
    },
    Cruiser {
        balancing: Arc<CruiserBalancing>,
        data: ShipData,
        cooldowns: Vec<Cooldown>,
    },
    Submarine {
        balancing: Arc<SubmarineBalancing>,
        data: ShipData,
        cooldowns: Vec<Cooldown>,
    },
    Destroyer {
        balancing: Arc<DestroyerBalancing>,
        data: ShipData,
        cooldowns: Vec<Cooldown>,
    },
}

impl Ship {
    pub fn new_from_type(
        ship_type: ShipType,
        ship_id: ShipID,
        position: (u32, u32),
        orientation: Orientation,
        cfg: Arc<Config>,
    ) -> Ship {
        let mut data = ShipData {
            id: ship_id,
            pos_x: position.0 as i32,
            pos_y: position.1 as i32,
            orientation,
            health: 0,
        };

        match ship_type {
            ShipType::Carrier => {
                let balancing = cfg.carrier_balancing.clone().unwrap();
                data.health = balancing.common_balancing.as_ref().unwrap().initial_health;
                Ship::Carrier {
                    balancing: Arc::from(balancing),
                    data,
                    cooldowns: Vec::new(),
                }
            }
            ShipType::Battleship => {
                let balancing = cfg.battleship_balancing.clone().unwrap();
                data.health = balancing.common_balancing.as_ref().unwrap().initial_health;
                Ship::Battleship {
                    balancing: Arc::from(balancing),
                    data,
                    cooldowns: Vec::new(),
                }
            }
            ShipType::Cruiser => {
                let balancing = cfg.cruiser_balancing.clone().unwrap();
                data.health = balancing.common_balancing.as_ref().unwrap().initial_health;
                Ship::Cruiser {
                    balancing: Arc::from(balancing),
                    data,
                    cooldowns: Vec::new(),
                }
            }
            ShipType::Submarine => {
                let balancing = cfg.submarine_balancing.clone().unwrap();
                data.health = balancing.common_balancing.as_ref().unwrap().initial_health;
                Ship::Submarine {
                    balancing: Arc::from(balancing),
                    data,
                    cooldowns: Vec::new(),
                }
            }
            ShipType::Destroyer => {
                let balancing = cfg.destroyer_balancing.clone().unwrap();
                data.health = balancing.common_balancing.as_ref().unwrap().initial_health;
                Ship::Destroyer {
                    balancing: Arc::from(balancing),
                    data,
                    cooldowns: Vec::new(),
                }
            }
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> i32 {
        match self {
            Ship::Carrier { .. } => 5,
            Ship::Battleship { .. } => 4,
            Ship::Cruiser { .. } => 3,
            Ship::Submarine { .. } => 3,
            Ship::Destroyer { .. } => 2,
        }
    }

    pub fn data(&self) -> ShipData {
        match self {
            Ship::Carrier { data, .. }
            | Ship::Battleship { data, .. }
            | Ship::Cruiser { data, .. }
            | Ship::Submarine { data, .. }
            | Ship::Destroyer { data, .. } => *data,
        }
    }

    pub fn get_player_id(&self) -> PlayerID {
        self.data().id.0
    }

    pub fn position(&self) -> (i32, i32) {
        match self {
            Ship::Carrier { data, .. }
            | Ship::Battleship { data, .. }
            | Ship::Cruiser { data, .. }
            | Ship::Submarine { data, .. }
            | Ship::Destroyer { data, .. } => (data.pos_x, data.pos_y),
        }
    }

    pub fn orientation(&self) -> Orientation {
        match self {
            Ship::Carrier { data, .. }
            | Ship::Battleship { data, .. }
            | Ship::Cruiser { data, .. }
            | Ship::Submarine { data, .. }
            | Ship::Destroyer { data, .. } => data.orientation,
        }
    }

    pub fn vision_range(&self) -> u32 {
        match self {
            Ship::Carrier { balancing, .. } => {
                balancing.common_balancing.as_ref().unwrap().vision_range
            }
            Ship::Battleship { balancing, .. } => {
                balancing.common_balancing.as_ref().unwrap().vision_range
            }
            Ship::Cruiser { balancing, .. } => {
                balancing.common_balancing.as_ref().unwrap().vision_range
            }
            Ship::Submarine { balancing, .. } => {
                balancing.common_balancing.as_ref().unwrap().vision_range
            }
            Ship::Destroyer { balancing, .. } => {
                balancing.common_balancing.as_ref().unwrap().vision_range
            }
        }
    }

    pub fn vision_envelope(&self) -> AABB<[i32; 2]> {
        let vision_range = self.vision_range();
        let mut envelope = self.envelope();
        let lower = envelope.lower();
        let upper = envelope.upper();
        envelope.merge(&AABB::from_point([
            lower[0] - vision_range as i32,
            lower[1] - vision_range as i32,
        ]));
        envelope.merge(&AABB::from_point([
            upper[0] + vision_range as i32,
            upper[1] + vision_range as i32,
        ]));
        envelope
    }

    pub fn common_balancing(&self) -> CommonBalancing {
        match self {
            Ship::Carrier { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Battleship { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Cruiser { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Submarine { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Destroyer { balancing, .. } => balancing.common_balancing.clone().unwrap(),
        }
    }

    pub fn cool_downs(&self) -> Vec<Cooldown> {
        match self {
            Ship::Carrier {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Battleship {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Cruiser {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Submarine {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Destroyer {
                cooldowns: cool_downs,
                ..
            } => cool_downs.clone(),
        }
    }

    pub fn cool_downs_mut(&mut self) -> &mut Vec<Cooldown> {
        match self {
            Ship::Carrier {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Battleship {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Cruiser {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Submarine {
                cooldowns: cool_downs,
                ..
            }
            | Ship::Destroyer {
                cooldowns: cool_downs,
                ..
            } => cool_downs,
        }
    }

    pub fn ship_type(&self) -> ShipType {
        match self {
            Ship::Carrier { .. } => ShipType::Carrier,
            Ship::Battleship { .. } => ShipType::Battleship,
            Ship::Cruiser { .. } => ShipType::Cruiser,
            Ship::Submarine { .. } => ShipType::Submarine,
            Ship::Destroyer { .. } => ShipType::Destroyer,
        }
    }

    /// Applies damage to a ship. Returns true whether the ship got destroyed
    pub fn apply_damage(&mut self, damage: u32) -> bool {
        match self {
            Ship::Carrier { data, .. }
            | Ship::Battleship { data, .. }
            | Ship::Cruiser { data, .. }
            | Ship::Submarine { data, .. }
            | Ship::Destroyer { data, .. } => {
                if damage >= data.health {
                    data.health = 0;
                    return true;
                }

                data.health -= damage;
                false
            }
        }
    }

    pub fn do_move(
        &mut self,
        direction: MoveDirection,
        bounds: &AABB<[i32; 2]>,
    ) -> Result<AABB<[i32; 2]>, ActionValidationError> {
        let orientation = self.orientation();
        let movement = match direction {
            MoveDirection::Forward => 1,
            MoveDirection::Backward => -1,
        };

        let (new_x, new_y) = match self {
            Ship::Carrier { data, .. }
            | Ship::Battleship { data, .. }
            | Ship::Cruiser { data, .. }
            | Ship::Submarine { data, .. }
            | Ship::Destroyer { data, .. } => match orientation {
                Orientation::North => (data.pos_x, data.pos_y + movement),
                Orientation::South => (data.pos_x, data.pos_y - movement),
                Orientation::East => (data.pos_x + movement, data.pos_y),
                Orientation::West => (data.pos_x - movement, data.pos_y),
            },
        };

        if bounds.contains_envelope(&self.get_envelope(new_x, new_y)) {
            self.set_position(new_x, new_y);
            Ok(self.envelope())
        } else {
            Err(ActionValidationError::OutOfMap)
        }
    }

    pub fn do_rotation(
        &mut self,
        direction: RotateDirection,
        bounds: &AABB<[i32; 2]>,
    ) -> Result<AABB<[i32; 2]>, ActionValidationError> {
        let (x, y) = self.position();
        let new_orientation = match (direction, self.orientation()) {
            (RotateDirection::Clockwise, Orientation::North) => Orientation::East,
            (RotateDirection::Clockwise, Orientation::West) => Orientation::North,
            (RotateDirection::Clockwise, Orientation::South) => Orientation::West,
            (RotateDirection::Clockwise, Orientation::East) => Orientation::South,
            (RotateDirection::CounterClockwise, Orientation::North) => Orientation::West,
            (RotateDirection::CounterClockwise, Orientation::West) => Orientation::South,
            (RotateDirection::CounterClockwise, Orientation::South) => Orientation::East,
            (RotateDirection::CounterClockwise, Orientation::East) => Orientation::North,
        };

        if bounds.contains_envelope(&self.get_envelope_with_orientation(x, y, new_orientation)) {
            self.set_orientation(new_orientation);
            Ok(self.envelope())
        } else {
            Err(ActionValidationError::OutOfMap)
        }
    }

    fn set_position(&mut self, x: i32, y: i32) {
        match self {
            Ship::Carrier { data, .. }
            | Ship::Battleship { data, .. }
            | Ship::Cruiser { data, .. }
            | Ship::Submarine { data, .. }
            | Ship::Destroyer { data, .. } => {
                data.pos_x = x;
                data.pos_y = y;
            }
        };
    }

    fn set_orientation(&mut self, orientation: Orientation) {
        match self {
            Ship::Carrier { data, .. }
            | Ship::Battleship { data, .. }
            | Ship::Cruiser { data, .. }
            | Ship::Submarine { data, .. }
            | Ship::Destroyer { data, .. } => {
                data.orientation = orientation;
            }
        };
    }

    fn get_envelope(&self, x: i32, y: i32) -> AABB<[i32; 2]> {
        self.get_envelope_with_orientation(x, y, self.orientation())
    }

    fn get_envelope_with_orientation(
        &self,
        x: i32,
        y: i32,
        orientation: Orientation,
    ) -> AABB<[i32; 2]> {
        match orientation {
            Orientation::North => AABB::from_corners([x, y + (self.len() - 1)], [x, y]),
            Orientation::South => AABB::from_corners([x, y], [x, y - (self.len() - 1)]),
            Orientation::East => AABB::from_corners([x, y], [x + (self.len() - 1), y]),
            Orientation::West => AABB::from_corners([x, y], [x - (self.len() - 1), y]),
        }
    }
}

impl GetShipID for Ship {
    fn id(&self) -> ShipID {
        self.data().id
    }
}

impl RTreeObject for Ship {
    type Envelope = AABB<[i32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        let (x, y) = self.position();
        self.get_envelope(x, y)
    }
}

impl PointDistance for Ship {
    fn distance_2(&self, point: &[i32; 2]) -> i32 {
        ship_distance(&self.envelope(), point)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ShipData {
    pub id: ShipID,
    pub pos_x: i32,
    pub pos_y: i32,
    pub orientation: Orientation,
    pub health: u32,
}

impl Default for ShipData {
    fn default() -> Self {
        ShipData {
            id: (0, 0),
            pos_x: 0,
            pos_y: 0,
            orientation: Orientation::North,
            health: 0,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Orientation {
    North,
    South,
    East,
    West,
}

impl From<Orientation> for Direction {
    fn from(orientation: Orientation) -> Self {
        match orientation {
            Orientation::North => Direction::North,
            Orientation::East => Direction::East,
            Orientation::South => Direction::South,
            Orientation::West => Direction::West,
        }
    }
}

impl From<Direction> for Orientation {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::North => Orientation::North,
            Direction::East => Orientation::East,
            Direction::South => Orientation::South,
            Direction::West => Orientation::West,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Cooldown {
    Movement { remaining_rounds: u32 },
    Rotate { remaining_rounds: u32 },
    Cannon { remaining_rounds: u32 },
    Ability { remaining_rounds: u32 },
}

impl Cooldown {
    pub fn remaining_rounds(&self) -> u32 {
        match self {
            Cooldown::Movement { remaining_rounds }
            | Cooldown::Rotate { remaining_rounds }
            | Cooldown::Cannon { remaining_rounds }
            | Cooldown::Ability { remaining_rounds } => *remaining_rounds,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ShipBoundingBox {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

#[derive(Debug, Clone)]
pub struct SelectShipsByIDFunction<T: RTreeObject>(pub Vec<(ShipID, T::Envelope)>);

impl<T: RTreeObject + GetShipID> SelectionFunction<T> for SelectShipsByIDFunction<T> {
    fn should_unpack_parent(&self, envelope: &T::Envelope) -> bool {
        self.0.iter().any(|ship| ship.1.intersects(envelope))
    }

    fn should_unpack_leaf(&self, leaf: &T) -> bool {
        self.0.iter().any(|&ship| leaf.id() == ship.0)
    }
}

pub type ShipID = (PlayerID, u32);

pub trait GetShipID {
    fn id(&self) -> ShipID;
}

pub fn ship_distance(envelope: &AABB<[i32; 2]>, point: &[i32; 2]) -> i32 {
    let p = envelope.min_point(point);
    max((point[0] - p[0]).abs(), (point[1] - p[1]).abs())
}
