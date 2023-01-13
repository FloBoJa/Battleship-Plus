use std::fmt::{Display, Formatter};

use log::debug;
use tokio::sync::RwLockWriteGuard;

use battleship_plus_common::messages::{EventMessage, ProtocolMessage};

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
    pub(crate) fn validate_inbound_message_allowed(
        &self,
        msg: &ProtocolMessage,
    ) -> Result<(), String> {
        if matches!(
            msg,
            ProtocolMessage::StatusMessage(_) | ProtocolMessage::ServerAdvertisement(_)
        ) || EventMessage::try_from(msg.clone()).is_ok()
        {
            return Err(format!("{msg:?} is not allowed as server-bound message"));
        }

        if !match self {
            GameState::Lobby => matches!(
                msg,
                ProtocolMessage::ServerConfigRequest(_)
                    | ProtocolMessage::JoinRequest(_)
                    | ProtocolMessage::TeamSwitchRequest(_)
                    | ProtocolMessage::SetReadyStateRequest(_)
            ),
            GameState::Preparation => matches!(
                msg,
                ProtocolMessage::ServerConfigRequest(_) | ProtocolMessage::SetPlacementRequest(_)
            ),
            GameState::InGame => matches!(
                msg,
                ProtocolMessage::ServerConfigRequest(_)
                    | ProtocolMessage::ServerStateRequest(_)
                    | ProtocolMessage::ActionRequest(_)
            ),
            GameState::End => matches!(
                msg,
                ProtocolMessage::ServerConfigRequest(_) | ProtocolMessage::ServerStateRequest(_)
            ),
        } {
            Err(format!("{msg:?} is not allowed in {self}"))
        } else {
            Ok(())
        }
    }

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
