use std::collections::{HashMap, HashSet};

use rstar::AABB;

use crate::game::ship_manager::ShipManager;
use crate::game::states::GameState;

pub type PlayerID = u32;

#[derive(Debug, Clone)]
pub struct Game {
    pub(crate) players: HashMap<PlayerID, Player>,
    pub(crate) team_a: HashSet<PlayerID>,
    pub(crate) team_a_size: u32,
    pub(crate) team_b: HashSet<PlayerID>,
    pub(crate) team_b_size: u32,

    pub(crate) ships: ShipManager,
    pub(crate) board_size: u32,

    pub(crate) state: GameState,
}

#[cfg(test)]
impl Default for Game {
    fn default() -> Self {
        Game::new(128, 8, 8)
    }
}

impl Game {
    pub fn new(board_size: u32, team_a_limit: u32, team_b_limit: u32) -> Self {
        Game {
            players: Default::default(),
            team_a: Default::default(),
            team_b: Default::default(),
            ships: Default::default(),
            team_a_size: team_a_limit,
            team_b_size: team_b_limit,
            board_size,
            state: GameState::Lobby,
        }
    }

    pub fn check_game_config(&self) -> Result<(), String> {
        // check that the board is big enough to host all players
        if self.quadrant_size() == 0 {
            let min_board_length = self.quadrant_per_row();
            // TODO Improve: Suggest actual minimal board size including placement of ships
            return Err(format!(
                "board is too small. Requires at least {min_board_length}x{min_board_length}",
            ));
        }

        // TODO Implementation: Implement more config checks

        Ok(())
    }

    pub fn can_start(&self) -> bool {
        self.team_a.len() == self.team_a_size as usize
            && self.team_b.len() == self.team_b_size as usize
            && self.players.iter().all(|(_, p)| p.is_ready)
    }

    pub fn board_bounds(&self) -> AABB<[i32; 2]> {
        AABB::from_corners([0; 2], [(self.board_size - 1) as i32; 2])
    }

    pub fn quadrant_per_row(&self) -> u32 {
        ((self.team_b_size + self.team_b_size) as f64).sqrt().ceil() as u32
    }

    pub fn quadrant_size(&self) -> u32 {
        self.board_size / self.quadrant_per_row()
    }

    pub fn quadrants(&self) -> Vec<(u32, u32)> {
        let quadrant_size = self.quadrant_size();
        let quadrants_per_row = self.quadrant_per_row();
        let initial_game_length = quadrants_per_row * quadrant_size;
        let tile_offset = (self.board_size - initial_game_length) / 2;

        (0..quadrants_per_row)
            .map(|x| {
                (0..quadrants_per_row).map(|y| {
                    (
                        tile_offset + (x * quadrant_size),
                        tile_offset + (y * quadrant_size),
                    )
                })
            })
            .flatten()
            .collect()
    }

    pub fn get_state(&self) -> GameState {
        self.state
    }

    pub(crate) fn unready_players(&mut self) {
        self.players
            .iter_mut()
            .for_each(|(_, player)| player.is_ready = false);
    }

    /// Removes a player from the game.
    /// Returns True when the game should be aborted.
    pub(crate) fn remove_player(&mut self, player_id: PlayerID) -> bool {
        if self.players.remove(&player_id).is_some() {
            self.team_a.remove(&player_id);
            self.team_b.remove(&player_id);

            match self.state {
                GameState::Lobby => false,
                GameState::Preparation => true,
                GameState::InGame => true,
                GameState::End => false,
            }
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Player {
    pub(crate) id: PlayerID,
    pub(crate) name: String,
    pub(crate) action_points: u32,
    pub(crate) is_ready: bool,
    pub(crate) quadrant: Option<(u32, u32)>,
}
