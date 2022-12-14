use battleship_plus_common::messages::{EngineBoostRequest, MoveRequest, MultiMissileRequest, PredatorMissileRequest, RotateRequest, ScoutPlaneRequest, SetPlacementRequest, SetReadyStateRequest, ShootRequest, TorpedoRequest};

use crate::game::data::{PlayerID, ShipID};

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
