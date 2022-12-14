use std::sync::Arc;

use tokio::sync::RwLock;

use crate::game::actions::Action;
use crate::game::data::Game;

#[derive(Debug, Copy, Clone)]
pub enum GameState {
    Lobby,
    Preparation,
    InGame,
    End,
}

#[derive(Debug, Clone)]
pub enum ActionExecutionError {
    OutOfState(GameState),
    Illegal(String),
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

        match action {
            // TODO: Action::TeamSwitch { .. } => {}
            // TODO: Action::SetReady { .. } => {}
            // TODO: Action::PlaceShips { .. } => {}
            // TODO: Action::Move { .. } => {}
            // TODO: Action::Rotate { .. } => {}
            // TODO: Action::Shoot { .. } => {}
            // TODO: Action::ScoutPlane { .. } => {}
            // TODO: Action::PredatorMissile { .. } => {}
            // TODO: Action::EngineBoost { .. } => {}
            // TODO: Action::Torpedo { .. } => {}
            // TODO: Action::MultiMissile { .. } => {}
            _ => todo!()
        }
    }
}