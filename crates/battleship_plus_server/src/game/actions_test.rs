mod actions_team_switch {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::game::actions::{Action, ActionExecutionError};
    use crate::game::data::{Game, Player, PlayerID};

    #[tokio::test]
    async fn actions_team_switch() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(
            Game {
                players: HashMap::from([
                    (player_id, Player {
                        id: player_id,
                        ..Default::default()
                    })
                ]),
                team_a: HashSet::from([player_id]),
                ..Default::default()
            }
        ));

        // player is in team a
        {
            let g = game.read().await;
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }

        // switch team a -> b
        assert!(Action::TeamSwitch { player_id }.apply_on(game.clone()).await.is_ok());
        {
            let g = game.read().await;
            assert!(!g.team_a.contains(&player_id));
            assert!(g.team_b.contains(&player_id));
        }

        // switch team b -> a
        assert!(Action::TeamSwitch { player_id }.apply_on(game.clone()).await.is_ok());
        {
            let g = game.read().await;
            assert!(g.team_a.contains(&player_id));
            assert!(!g.team_b.contains(&player_id));
        }
    }

    #[tokio::test]
    async fn actions_team_switch_detect_inconsistent_state() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(
            Game {
                players: HashMap::from([
                    (player_id, Player {
                        id: player_id,
                        ..Default::default()
                    })
                ]),
                team_a: HashSet::from([player_id]),
                team_b: HashSet::from([player_id]),
                ..Default::default()
            }
        ));

        let res = Action::TeamSwitch { player_id }.apply_on(game.clone()).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::InconsistentState(e) => e == "found illegal team assignment for player 42",
            _ => false
        })
    }

    #[tokio::test]
    async fn actions_team_switch_unknown_player() {
        let player_id: PlayerID = 42;
        let game = Arc::new(RwLock::new(
            Game {
                ..Default::default()
            }
        ));

        let res = Action::TeamSwitch { player_id }.apply_on(game.clone()).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(match err {
            ActionExecutionError::Illegal(e) => e == "PlayerID 42 is unknown",
            _ => false
        })
    }
}

