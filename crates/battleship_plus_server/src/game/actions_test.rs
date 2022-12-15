use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::game::actions::Action;
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