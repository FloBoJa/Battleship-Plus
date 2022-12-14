use battleship_plus_common::messages::{EngineBoostRequest, MoveRequest, MultiMissileRequest, PredatorMissileRequest, RotateRequest, ScoutPlaneRequest, SetPlacementRequest, SetReadyStateRequest, ShootRequest, TorpedoRequest};

#[derive(Debug, Clone)]
pub enum Action {
    // Lobby actions
    TeamSwitch { player_id: u32 },
    SetReady { player_id: u32, request: SetReadyStateRequest },

    // Preparation actions
    PlaceShips { player_id: u32, request: SetPlacementRequest },

    // Game actions
    Move { player_id: u32, ship_id: u32, request: MoveRequest },
    Rotate { player_id: u32, ship_id: u32, request: RotateRequest },
    Shoot { player_id: u32, ship_id: u32, request: ShootRequest },
    ScoutPlane { player_id: u32, ship_id: u32, request: ScoutPlaneRequest },
    PredatorMissile { player_id: u32, ship_id: u32, request: PredatorMissileRequest },
    EngineBoost { player_id: u32, ship_id: u32, request: EngineBoostRequest },
    Torpedo { player_id: u32, ship_id: u32, request: TorpedoRequest },
    MultiMissile { player_id: u32, ship_id: u32, request: MultiMissileRequest },
}
