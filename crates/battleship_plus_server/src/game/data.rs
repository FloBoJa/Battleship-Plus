use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rstar::{AABB, PointDistance, RTree, RTreeObject};

use battleship_plus_common::messages::{BattleshipBalancing, CarrierBalancing, CruiserBalancing, DestroyerBalancing, SubmarineBalancing};

pub type PlayerID = u32;
pub type ShipID = u32;

#[derive(Debug, Clone, Default)]
pub struct Game {
    pub(crate) players: HashMap<PlayerID, Player>,
    pub(crate) team_a: HashSet<PlayerID>,
    pub(crate) team_a_limit: u32,
    pub(crate) team_b: HashSet<PlayerID>,
    pub(crate) team_b_limit: u32,

    pub(crate) ships: HashMap<u32, Ship>,
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
    id: ShipID,
    pos_x: i32,
    pos_y: i32,
    orientation: Orientation,
    health: u32,
}

#[derive(Debug, Copy, Clone)]
pub struct ShipBoundingBox {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

#[derive(Debug, Clone)]
pub enum Ship {
    Carrier { balancing: Arc<CarrierBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Battleship { balancing: Arc<BattleshipBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Cruiser { balancing: Arc<CruiserBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Submarine { balancing: Arc<SubmarineBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
    Destroyer { balancing: Arc<DestroyerBalancing>, data: ShipData, cool_downs: Vec<Cooldown> },
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
}

#[derive(Debug, Clone)]
pub struct ShipRef(Arc<Ship>);

impl RTreeObject for ShipRef {
    type Envelope = AABB<[i32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        let (x, y) = self.0.position();


        match self.0.orientation() {
            Orientation::North => AABB::from_corners([x, y - (self.0.len() - 1)], [x, y]),
            Orientation::South => AABB::from_corners([x, y], [x, y + (self.0.len() - 1)]),
            Orientation::East => AABB::from_corners([x, y], [x + (self.0.len() - 1), y]),
            Orientation::West => AABB::from_corners([x, y], [x - (self.0.len() - 1), y]),
        }
    }
}

impl PointDistance for ShipRef {
    fn distance_2(&self, point: &[i32; 2]) -> i32 {
        self.envelope().distance_2(point)
    }
}
