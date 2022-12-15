use std::sync::Arc;

use log::debug;
use tokio::sync::RwLock;

use crate::game::actions::{Action, ActionExecutionError};
use crate::game::data::Game;

#[derive(Debug, Copy, Clone)]
pub enum GameState {
    Lobby,
    Preparation,
    InGame,
    End,
}

impl GameState {
    pub fn is_action_valid(&self, action: &Action) -> bool {
        match self {
            GameState::Lobby => match action {
                Action::TeamSwitch { .. } |
                Action::SetReady { .. } => true,
                _ => false,
            }
            GameState::Preparation => match action {
                Action::PlaceShips { .. } => true,
                _ => false,
            }
            GameState::InGame => match action {
                Action::Move { .. } |
                Action::Rotate { .. } |
                Action::Shoot { .. } |
                Action::ScoutPlane { .. } |
                Action::PredatorMissile { .. } |
                Action::EngineBoost { .. } |
                Action::Torpedo { .. } |
                Action::MultiMissile { .. } => true,
                _ => false,
            }
            GameState::End => match action {
                _ => false,
            }
        }
    }

    pub async fn execute_action(&self, action: Action, game: Arc<RwLock<Game>>) -> Result<(), ActionExecutionError> {
        if !self.is_action_valid(&action) {
            return Err(ActionExecutionError::OutOfState(self.clone()));
        }

        debug!("execute {:?} action on game", action);
        action.apply_on(game).await
    }
}