use std::sync::Arc;

use log::{debug, error};
use tokio::sync::RwLock;

use battleship_plus_common::messages::{EngineBoostRequest, MoveRequest, MultiMissileRequest, PredatorMissileRequest, RotateRequest, ScoutPlaneRequest, SetPlacementRequest, SetReadyStateRequest, ShootRequest, TorpedoRequest};

use crate::game::data::{Game, PlayerID, ShipID};
use crate::game::states::GameState;

#[derive(Debug, Clone)]
pub enum Action {
    // Lobby actions
    TeamSwitch { player_id: PlayerID },
    SetReady { player_id: PlayerID, request: SetReadyStateRequest },

    // Preparation actions
    PlaceShips { player_id: PlayerID, request: SetPlacementRequest },

    // Game actions
    Move { player_id: PlayerID, ship_id: ShipID, request: MoveRequest },
    Rotate { player_id: PlayerID, ship_id: ShipID, request: RotateRequest },
    Shoot { player_id: PlayerID, ship_id: ShipID, request: ShootRequest },
    ScoutPlane { player_id: PlayerID, ship_id: ShipID, request: ScoutPlaneRequest },
    PredatorMissile { player_id: PlayerID, ship_id: ShipID, request: PredatorMissileRequest },
    EngineBoost { player_id: PlayerID, ship_id: ShipID, request: EngineBoostRequest },
    Torpedo { player_id: PlayerID, ship_id: ShipID, request: TorpedoRequest },
    MultiMissile { player_id: PlayerID, ship_id: ShipID, request: MultiMissileRequest },
}

#[derive(Debug, Clone)]
pub enum ActionExecutionError {
    OutOfState(GameState),
    Illegal(String),
    InconsistentState(String),
}

impl Action {
    pub(crate) async fn apply_on(&self, game: Arc<RwLock<Game>>) -> Result<(), ActionExecutionError> {
        // TODO: implement actions below
        // TODO: add tests for all actions

        match self {
            Action::TeamSwitch { player_id } => {
                {
                    let g = game.read().await;
                    if !g.players.contains_key(player_id) {
                        let msg = format!("PlayerID {} is unknown", player_id);
                        debug!("{}", msg.as_str());
                        return Err(ActionExecutionError::Illegal(msg));
                    }
                }

                {
                    let mut g = game.write().await;

                    match (g.team_a.remove(&player_id), g.team_b.remove(player_id)) {
                        (true, false) => g.team_b.insert(*player_id),
                        (false, true) => g.team_a.insert(*player_id),
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
