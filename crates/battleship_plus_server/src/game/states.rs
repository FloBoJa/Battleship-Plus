use std::fmt::{Display, Formatter, Write};

use log::debug;
use tokio::sync::RwLockWriteGuard;

use crate::game::actions::{Action, ActionExecutionError};
use crate::game::data::Game;

#[derive(Debug, Copy, Clone)]
pub enum GameState {
    Lobby,
    Preparation,
    InGame,
    End,
}

impl Display for GameState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GameState::Lobby => f.write_str("Lobby"),
            GameState::Preparation => f.write_str("Preparation"),
            GameState::InGame => f.write_str("InGame"),
            GameState::End => f.write_str("End"),
        }
    }
}

impl GameState {
    pub fn is_action_valid(&self, action: &Action) -> bool {
        match self {
            GameState::Lobby => {
                matches!(action, Action::TeamSwitch { .. } | Action::SetReady { .. })
            }
            GameState::Preparation => matches!(action, Action::PlaceShips { .. }),
            GameState::InGame => matches!(
                action,
                Action::Move { .. }
                    | Action::Rotate { .. }
                    | Action::Shoot { .. }
                    | Action::ScoutPlane { .. }
                    | Action::PredatorMissile { .. }
                    | Action::EngineBoost { .. }
                    | Action::Torpedo { .. }
                    | Action::MultiMissile { .. }
            ),
            GameState::End => false,
        }
    }

    pub fn execute_action(
        &self,
        action: Action,
        game: &mut RwLockWriteGuard<Game>,
    ) -> Result<(), ActionExecutionError> {
        if !self.is_action_valid(&action) {
            return Err(ActionExecutionError::OutOfState(*self));
        }

        debug!("execute {:?} action on game", action);
        action.apply_on(game)
    }
}
