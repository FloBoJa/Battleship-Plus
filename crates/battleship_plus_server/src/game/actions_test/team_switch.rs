//noinspection DuplicatedCode
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use battleship_plus_common::game::{ActionValidationError, PlayerID};

use crate::game::actions::{Action, ActionExecutionError};
use crate::game::data::{Game, Player};

#[tokio::test]
async fn actions_team_switch() {
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

    // player is in team a
    {
        assert!(g.team_a.contains(&player_id));
        assert!(!g.team_b.contains(&player_id));
    }

    // switch team a -> b
    assert!(matches!(
        Action::TeamSwitch { player_id }.apply_on(&mut g),
        Ok(None)
    ));
    {
        assert!(!g.team_a.contains(&player_id));
        assert!(g.team_b.contains(&player_id));
    }

    // switch team b -> a
    assert!(matches!(
        Action::TeamSwitch { player_id }.apply_on(&mut g),
        Ok(None)
    ));
    {
        assert!(g.team_a.contains(&player_id));
        assert!(!g.team_b.contains(&player_id));
    }
}

#[tokio::test]
async fn actions_team_switch_detect_inconsistent_state() {
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
        team_b: HashSet::from([player_id]),
        ..Default::default()
    }));
    let mut g = g.write().await;

    let res = Action::TeamSwitch { player_id }.apply_on(&mut g);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(match err {
        ActionExecutionError::InconsistentState(e) =>
            e == "found illegal team assignment for player 42",
        _ => false,
    })
}

#[tokio::test]
async fn actions_team_switch_unknown_player() {
    let player_id: PlayerID = 42;
    let g = Arc::new(RwLock::new(Game {
        ..Default::default()
    }));
    let mut g = g.write().await;

    let res = Action::TeamSwitch { player_id }.apply_on(&mut g);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(match err {
        ActionExecutionError::Validation(ActionValidationError::NonExistentPlayer { id }) =>
            id == player_id,
        _ => false,
    })
}
