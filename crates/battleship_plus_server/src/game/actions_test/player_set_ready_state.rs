//noinspection DuplicatedCode
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use battleship_plus_common::game::{ActionValidationError, PlayerID};
use battleship_plus_common::messages::SetReadyStateRequest;

use crate::game::actions::{Action, ActionExecutionError, ActionResult};
use crate::game::data::{Game, Player};

#[tokio::test]
async fn actions_player_set_ready() {
    let player_id: PlayerID = 42;
    let g = Arc::new(RwLock::new(Game {
        players: HashMap::from([(
            player_id,
            Player {
                id: player_id,
                ..Default::default()
            },
        )]),
        team_a: HashSet::from([player_id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    // set player ready
    assert!(matches!(
        Action::SetReady {
            player_id,
            request: SetReadyStateRequest { ready_state: true },
        }
        .apply_on(&mut g),
        Ok(ActionResult::None)
    ));
    {
        assert!(g.players.get(&player_id).unwrap().is_ready);
    }

    // set player not ready
    assert!(matches!(
        Action::SetReady {
            player_id,
            request: SetReadyStateRequest { ready_state: false },
        }
        .apply_on(&mut g),
        Ok(ActionResult::None)
    ));
    {
        assert!(!g.players.get(&player_id).unwrap().is_ready);
    }
}

#[tokio::test]
async fn actions_set_ready_unknown_player() {
    let player_id: PlayerID = 42;
    let g = Arc::new(RwLock::new(Game {
        ..Default::default()
    }));
    let mut g = g.write().await;

    let res = Action::SetReady {
        player_id,
        request: SetReadyStateRequest { ready_state: true },
    }
    .apply_on(&mut g);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(match err {
        ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }) =>
            id == player_id,
        _ => false,
    })
}
