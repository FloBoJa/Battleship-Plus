use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rstar::{Envelope, RTreeObject, AABB};

use battleship_plus_common::{
    types::{Config, Direction, ShipAssignment, ShipType},
    util,
};

use crate::config_provider::default_config_provider;
use crate::game::ship::{Ship, ShipID};
use crate::game::ship_manager::{ShipManager, ShipPlacementError};
use crate::game::states::GameState;

pub type PlayerID = u32;

#[derive(Debug, Clone)]
pub struct Game {
    pub(crate) config: Arc<Config>,

    pub(crate) players: HashMap<PlayerID, Player>,
    pub(crate) team_a: HashSet<PlayerID>,
    pub(crate) team_b: HashSet<PlayerID>,

    pub(crate) ships: ShipManager,

    pub(crate) state: GameState,
}

impl Default for Game {
    fn default() -> Self {
        Game::new(default_config_provider().game_config())
    }
}

impl Game {
    pub fn new(config: Arc<Config>) -> Self {
        Game {
            config,
            players: Default::default(),
            team_a: Default::default(),
            team_b: Default::default(),
            ships: Default::default(),
            state: GameState::Lobby,
        }
    }

    pub fn check_game_config(&self) -> Result<(), String> {
        // check that the board is big enough to host all players
        let player_count = self.config.team_size_a + self.config.team_size_b;
        if util::quadrant_size(self.config.board_size, player_count) == 0 {
            let min_board_length = util::quadrants_per_row(player_count);
            // TODO Improve: Suggest actual minimal board size including placement of ships
            return Err(format!(
                "board is too small. Requires at least {min_board_length}x{min_board_length}",
            ));
        }

        // TODO Implementation: Implement more config checks

        Ok(())
    }

    pub fn can_change_into_preparation_phase(&self) -> bool {
        matches!(self.state, GameState::Lobby)
            && self.team_a.len() == self.config.team_size_a as usize
            && self.team_b.len() == self.config.team_size_b as usize
            && self.players.iter().all(|(_, p)| p.is_ready)
    }

    pub fn can_change_into_game_phase(&self) -> bool {
        matches!(self.state, GameState::Preparation)
            && self.check_players_placed_ships(
                self.team_a.iter().cloned(),
                self.config.ship_set_team_a.clone(),
            )
            && self.check_players_placed_ships(
                self.team_b.iter().cloned(),
                self.config.ship_set_team_b.clone(),
            )
    }

    fn check_players_placed_ships(
        &self,
        mut team: impl Iterator<Item = PlayerID>,
        ships: Vec<i32>,
    ) -> bool {
        team.all(|player_id| {
            ships.iter().enumerate().all(|(ship_number, _)| {
                self.ships
                    .get_by_id(&(player_id, ship_number as u32))
                    .is_some()
            })
        })
    }

    pub fn board_bounds(&self) -> AABB<[i32; 2]> {
        AABB::from_corners([0; 2], [(self.config.board_size - 1) as i32; 2])
    }

    pub fn quadrants(&self) -> Vec<(u32, u32)> {
        let player_count = self.config.team_size_a + self.config.team_size_b;
        let quadrant_size = util::quadrant_size(self.config.board_size, player_count);
        let quadrants_per_row = util::quadrants_per_row(player_count);
        let initial_game_length = quadrants_per_row * quadrant_size;
        let tile_offset = (self.config.board_size - initial_game_length) / 2;

        (0..quadrants_per_row)
            .flat_map(|x| {
                (0..quadrants_per_row).map(move |y| {
                    (
                        tile_offset + (x * quadrant_size),
                        tile_offset + (y * quadrant_size),
                    )
                })
            })
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

    pub(crate) fn validate_placement_request(
        &self,
        player_id: PlayerID,
        assignments: &Vec<ShipAssignment>,
    ) -> Result<HashMap<ShipID, Ship>, ShipPlacementError> {
        let ship_set = match (
            self.team_a.contains(&player_id),
            self.team_b.contains(&player_id),
        ) {
            (true, false) => &self.config.ship_set_team_a,
            (false, true) => &self.config.ship_set_team_b,
            (false, false) => return Err(ShipPlacementError::PlayerNotInGame),
            _ => unreachable!(),
        };

        let player = self.players.get(&player_id).unwrap();
        let corner = player.quadrant.unwrap();
        let player_count = self.config.team_size_a + self.config.team_size_b;
        let quadrant = util::quadrant_from_corner(corner, self.config.board_size, player_count);

        if (0..ship_set.len())
            .map(|ship_number| (player_id, ship_number as u32) as ShipID)
            .any(|ship_id| self.ships.get_by_id(&ship_id).is_some())
        {
            return Err(ShipPlacementError::PlayerHasAlreadyPlacedShips);
        }

        if assignments.len() != ship_set.len() {
            return Err(ShipPlacementError::InvalidShipSet);
        }

        let mut ship_manager = ShipManager::new();
        for assignment in assignments {
            let ship_id: ShipID = (player_id, assignment.ship_number);
            if assignment.ship_number >= ship_set.len() as u32 {
                return Err(ShipPlacementError::InvalidShipNumber);
            }

            let ship_type = match ShipType::from_i32(ship_set[assignment.ship_number as usize]) {
                None => return Err(ShipPlacementError::InvalidShipType),
                Some(t) => t,
            };

            let direction = match Direction::from_i32(assignment.direction) {
                None => return Err(ShipPlacementError::InvalidShipDirection),
                Some(d) => d,
            };

            let position = match &assignment.coordinate {
                None => return Err(ShipPlacementError::InvalidShipPosition),
                Some(v) => (v.x, v.y),
            };

            let ship = Ship::new_from_type(
                ship_type,
                ship_id,
                position,
                direction.into(),
                self.config.clone(),
            );

            if !quadrant.contains_envelope(&ship.envelope()) {
                return Err(ShipPlacementError::ShipOutOfQuadrant);
            }

            ship_manager.place_ship(ship_id, ship)?;
        }

        Ok(ship_manager.into())
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
