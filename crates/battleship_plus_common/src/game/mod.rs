pub mod ship;
pub mod ship_manager;

pub type PlayerID = u32;

#[derive(Debug, Clone)]
pub enum ActionValidationError {
    NonExistentPlayer { id: PlayerID },
    NonExistentShip { id: ship::ShipID },
    Cooldown { remaining_rounds: u32 },
    InsufficientPoints { required: u32 },
    Unreachable,
    OutOfMap,
    InvalidShipPlacement(ship_manager::ShipPlacementError),
    NotPlayersTurn,
}
