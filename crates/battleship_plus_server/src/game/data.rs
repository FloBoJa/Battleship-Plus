use std::collections::{HashMap, HashSet};

use rstar::AABB;

use crate::game::ship_manager::ShipManager;

pub type PlayerID = u32;

#[derive(Debug, Clone, Default)]
pub struct Game {
    pub(crate) players: HashMap<PlayerID, Player>,
    pub(crate) team_a: HashSet<PlayerID>,
    pub(crate) team_a_limit: u32,
    pub(crate) team_b: HashSet<PlayerID>,
    pub(crate) team_b_limit: u32,

    pub(crate) ships: ShipManager,
    pub(crate) board_size: u32,
}

impl Game {
    pub fn can_start(&self) -> bool {
        self.team_a.len() <= self.team_a_limit as usize
            && self.team_b.len() <= self.team_b_limit as usize
            && self.players.iter().all(|(_, p)| p.is_ready)
    }

    pub fn board_bounds(&self) -> AABB<[i32; 2]> {
        AABB::from_corners([0; 2], [(self.board_size - 1) as i32; 2])
    }
}

#[derive(Debug, Clone, Default)]
pub struct Player {
    pub(crate) id: PlayerID,
    pub(crate) name: String,
    pub(crate) action_points: u32,
    pub(crate) is_ready: bool,
}
