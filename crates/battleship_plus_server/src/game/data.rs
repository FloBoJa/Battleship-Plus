use std::cmp::max;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rstar::{AABB, Envelope, PointDistance, RTree, RTreeObject, SelectionFunction};

use battleship_plus_common::messages::{BattleshipBalancing, CarrierBalancing, CommonBalancing, CruiserBalancing, DestroyerBalancing, SubmarineBalancing};

pub type PlayerID = u32;
pub type ShipID = (PlayerID, u32);

#[derive(Debug, Clone, Default)]
pub struct Game {
    pub(crate) players: HashMap<PlayerID, Player>,
    pub(crate) team_a: HashSet<PlayerID>,
    pub(crate) team_a_limit: u32,
    pub(crate) team_b: HashSet<PlayerID>,
    pub(crate) team_b_limit: u32,

    pub(crate) ships: HashMap<ShipID, Ship>,
    pub(crate) ships_geo_lookup: RTree<ShipRef>,
    pub(crate) board_size: u32,
}

impl Game {
    pub fn can_start(&self) -> bool {
        self.team_a.len() <= self.team_a_limit as usize
            && self.team_b.len() <= self.team_b_limit as usize
            && self.players.iter().all(|(_, p)| p.is_ready)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Player {
    pub(crate) id: PlayerID,
    pub(crate) name: String,
    pub(crate) action_points: u32,
    pub(crate) is_ready: bool,
}

#[derive(Debug, Copy, Clone)]
pub enum Orientation {
    North,
    South,
    East,
    West,
}

#[derive(Debug, Copy, Clone)]
pub enum Cooldown {
    Movement { remaining_rounds: u32 },
    Cannon { remaining_rounds: u32 },
    Ability { remaining_rounds: u32 },
}

#[derive(Debug, Copy, Clone)]
pub struct ShipData {
    pub(crate) id: ShipID,
    pub(crate) player_id: PlayerID,
    pub(crate) pos_x: i32,
    pub(crate) pos_y: i32,
    pub(crate) orientation: Orientation,
    pub(crate) health: u32,
}

impl Default for ShipData {
    fn default() -> Self {
        ShipData {
            id: (0, 0),
            player_id: 0,
            pos_x: 0,
            pos_y: 0,
            orientation: Orientation::North,
            health: 0,
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

pub(crate) trait GetShipID {
    fn id(&self) -> ShipID;
}

#[derive(Debug, Clone)]
pub enum Ship {
    Carrier { balancing: Arc<CarrierBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Battleship { balancing: Arc<BattleshipBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Cruiser { balancing: Arc<CruiserBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Submarine { balancing: Arc<SubmarineBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Destroyer { balancing: Arc<DestroyerBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
}

impl GetShipID for Ship {
    fn id(&self) -> ShipID {
        self.data().id
    }
}

impl GetShipID for ShipRef {
    fn id(&self) -> ShipID {
        self.0.data().id
    }
}

impl Ship {
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
            Ship::Carrier { data, .. } |
            Ship::Battleship { data, .. } |
            Ship::Cruiser { data, .. } |
            Ship::Submarine { data, .. } |
            Ship::Destroyer { data, .. } => *data
        }
    }

    /// Applies damage to a ship. Returns true whether the ship got destroyed
    pub fn apply_damage(&mut self, damage: u32) -> bool {
        match self {
            Ship::Carrier { data, .. } |
            Ship::Battleship { data, .. } |
            Ship::Cruiser { data, .. } |
            Ship::Submarine { data, .. } |
            Ship::Destroyer { data, .. } => {
                if damage >= data.health {
                    data.health = 0;
                    return true;
                }

                data.health -= damage;
                false
            }
        }
    }

    pub fn position(&self) -> (i32, i32) {
        match self {
            Ship::Carrier { data, .. } |
            Ship::Battleship { data, .. } |
            Ship::Cruiser { data, .. } |
            Ship::Submarine { data, .. } |
            Ship::Destroyer { data, .. } => (data.pos_x, data.pos_y)
        }
    }

    pub fn orientation(&self) -> Orientation {
        match self {
            Ship::Carrier { data, .. } |
            Ship::Battleship { data, .. } |
            Ship::Cruiser { data, .. } |
            Ship::Submarine { data, .. } |
            Ship::Destroyer { data, .. } => data.orientation
        }
    }

    pub fn common_balancing(&self) -> CommonBalancing {
        match self {
            Ship::Carrier { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Battleship { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Cruiser { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Submarine { balancing, .. } => balancing.common_balancing.clone().unwrap(),
            Ship::Destroyer { balancing, .. } => balancing.common_balancing.clone().unwrap(),
        }.clone()
    }

    pub fn cool_downs(&self) -> Vec<Cooldown> {
        match self {
            Ship::Carrier { cool_downs, .. } |
            Ship::Battleship { cool_downs, .. } |
            Ship::Cruiser { cool_downs, .. } |
            Ship::Submarine { cool_downs, .. } |
            Ship::Destroyer { cool_downs, .. } => cool_downs.clone()
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShipRef(pub Arc<Ship>);

impl RTreeObject for Ship {
    type Envelope = AABB<[i32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        let (x, y) = self.position();

        match self.orientation() {
            Orientation::North => AABB::from_corners([x, y - (self.len() - 1)], [x, y]),
            Orientation::South => AABB::from_corners([x, y], [x, y + (self.len() - 1)]),
            Orientation::East => AABB::from_corners([x, y], [x + (self.len() - 1), y]),
            Orientation::West => AABB::from_corners([x, y], [x - (self.len() - 1), y]),
        }
    }
}

impl RTreeObject for ShipRef {
    type Envelope = AABB<[i32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.0.envelope()
    }
}

impl PointDistance for Ship {
    fn distance_2(&self, point: &[i32; 2]) -> i32 {
        let p = self.envelope().min_point(point);
        max((point[0] - p[0]).abs(), (point[1] - p[1]).abs())
    }
}

impl PointDistance for ShipRef {
    fn distance_2(&self, point: &[i32; 2]) -> i32 {
        self.envelope().distance_2(point)
    }
}
