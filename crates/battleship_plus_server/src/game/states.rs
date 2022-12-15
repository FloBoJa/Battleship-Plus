use std::sync::Arc;

use log::{debug, error};
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
    InconsistentState(String),
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

        // TODO: implement actions below
        // TODO: add tests for all actions

        match action {
            Action::TeamSwitch { player_id } => {
                {
                    let g = game.read().await;
                    if !g.players.contains_key(&player_id) {
                        let msg = format!("PlayerID {} is unknown", player_id);
                        debug!("{}", msg.as_str());
                        return Err(ActionExecutionError::Illegal(msg));
                    }
                }

                {
                    let mut g = game.write().await;

                    match (g.team_a.remove(&player_id), g.team_b.remove(&player_id)) {
                        (true, false) => g.team_b.insert(player_id),
                        (false, true) => g.team_a.insert(player_id),
                        _ => {
                            let msg = format!("found illegal team assignment for player {}", player_id);
                            error!("{}", msg.as_str());
                            return Err(ActionExecutionError::InconsistentState(msg));
                        }
                    };
                }

                Ok(())
            }
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

        // TODO: find a good way to return Action Results
    }
}