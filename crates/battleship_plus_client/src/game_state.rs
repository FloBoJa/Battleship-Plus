#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub enum GameState {
    Unconnected,
    Joining,
    Lobby,
    // TODO:
    // Placement,
    // Game,
}
