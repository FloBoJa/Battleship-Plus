use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, trace, warn};
use rand::seq::SliceRandom;
use rand::thread_rng;
use tokio::macros::support::thread_rng_n;
use tokio::sync::{mpsc, RwLock, RwLockWriteGuard};

use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::messages::{
    GameStart, JoinResponse, LobbyChangeEvent, PlacementPhase, ProtocolMessage,
    ServerConfigResponse, ServerStateResponse, SetReadyStateRequest, SetReadyStateResponse,
    StatusCode, StatusMessage, TeamSwitchResponse,
};
use battleship_plus_common::types::{
    Config, Coordinate, Direction, PlayerLobbyState, ServerState, ShipState,
};
use bevy_quinnet_server::certificate::CertificateRetrievalMode;
use bevy_quinnet_server::{
    ClientId, Endpoint, EndpointEvent, QuinnetError, Server, ServerConfigurationData,
};

use crate::config_provider::ConfigProvider;
use crate::game::actions::{Action, ActionExecutionError, ActionValidationError};
use crate::game::data::{Game, Player, PlayerID, Turn};
use crate::game::ship::{Cooldown, Orientation, Ship};
use crate::game::states::GameState;
use crate::tasks::{upgrade_oneshot, TaskControl};

pub fn spawn_server_task(cfg: Arc<dyn ConfigProvider + Send + Sync>) -> TaskControl {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(server_task(cfg, rx));
    TaskControl::new(tx, handle)
}

type BroadcastChannel = (
    tokio::sync::broadcast::Sender<(Vec<ClientId>, ProtocolMessage)>,
    tokio::sync::broadcast::Receiver<(Vec<ClientId>, ProtocolMessage)>,
);

pub async fn server_task(
    cfg: Arc<dyn ConfigProvider + Send + Sync>,
    stop: tokio::sync::oneshot::Receiver<()>,
) {
    let mut stop = upgrade_oneshot(stop);

    let addr6 = cfg.server_config().game_address_v6;
    let addr4 = cfg.server_config().game_address_v4;
    let ascii_host = match cfg.server_config().server_domain {
        None => cfg
            .game_config()
            .server_name
            .chars()
            .filter(|c| c.is_ascii())
            .collect::<String>(),
        Some(domain) => String::from(domain),
    };

    let mut server6 = Server::new_standalone();
    let server6 = match server6.start_endpoint(
        ServerConfigurationData::new(
            ascii_host.clone(),
            addr6.port(),
            Ipv6Addr::UNSPECIFIED.to_string(),
        ),
        CertificateRetrievalMode::LoadFromFileOrGenerateSelfSigned {
            cert_file: "./certificate6.pem".to_string(),
            key_file: "./key6.pem".to_string(),
            save_on_disk: true,
        },
    ) {
        Ok(_) => Some(Arc::new(RwLock::new(server6))),
        Err(e) => {
            error!("Unable to listen on {addr6}: {e}");
            panic!("Unable to listen on {addr6}: {e}")
        }
    };

    let server4;
    {
        if addr6.ip().is_unspecified()
            && addr4.ip().is_unspecified()
            && addr6.port() == addr4.port()
        {
            // quinnet will panic on systems that support dual stack ports
            // therefore we skip the case when the server should listen on
            // the same port for IPv4 and IPv6 on 0.0.0.0 and [::].
            // https://stackoverflow.com/a/51913093

            // TODO: Find a nice way to support dual stack and non dual stack OSs
            server4 = None;
        } else {
            let mut s4 = Server::new_standalone();
            server4 = match s4.start_endpoint(
                ServerConfigurationData::new(
                    ascii_host.clone(),
                    addr4.port(),
                    Ipv4Addr::UNSPECIFIED.to_string(),
                ),
                CertificateRetrievalMode::LoadFromFileOrGenerateSelfSigned {
                    cert_file: "./certificate4.pem".to_string(),
                    key_file: "./key4.pem".to_string(),
                    save_on_disk: true,
                },
            ) {
                Ok(_) => Some(Arc::new(RwLock::new(s4))),
                Err(e) => {
                    warn!("Unable to listen on {addr4}: {e}");
                    None
                }
            };
        }
    }

    info!("Endpoints initialized");

    loop {
        let game = Game::default();

        // check game config
        if let Err(e) = game.check_game_config() {
            error!("Game config check failed: {e}");
        } else {
            let game = Arc::new(RwLock::new(game));
            let (game_end_tx, mut game_end_rx) = mpsc::unbounded_channel();

            let (broadcast_tx, broadcast_rx): BroadcastChannel =
                tokio::sync::broadcast::channel(128);

            let servers: Vec<_> = [server6.clone(), server4.clone()]
                .iter()
                .filter(|e| e.is_some())
                .map(|s| s.as_ref().unwrap().clone())
                .collect();

            let handles: Vec<_> = servers
                .iter()
                .map(|server| {
                    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();

                    (
                        tokio::spawn(endpoint_task(
                            cfg.game_config(),
                            server.clone(),
                            broadcast_tx.clone(),
                            broadcast_rx.resubscribe(),
                            game.clone(),
                            game_end_tx.clone(),
                            cancel_rx,
                        )),
                        cancel_tx,
                    )
                })
                .collect();

            info!("New game initialized");

            tokio::select! {
                _ = game_end_rx.recv() => {},
                _ = stop.recv() => return,
            }

            // TODO: find a better way to wait for queues
            // let queues run out
            tokio::time::sleep(Duration::from_secs(3)).await;
            info!("Game finished");

            for h in handles {
                h.1.send(())
                    .expect("unable to notify endpoint tasks to cancel");

                if let Err(e) = h.0.await {
                    error!("server task finished with an error {e}");
                }
            }
        }
    }
}

async fn endpoint_task(
    cfg: Arc<Config>,
    server: Arc<RwLock<Server>>,
    broadcast_tx: tokio::sync::broadcast::Sender<(Vec<ClientId>, ProtocolMessage)>,
    mut broadcast_rx: tokio::sync::broadcast::Receiver<(Vec<ClientId>, ProtocolMessage)>,
    game: Arc<RwLock<Game>>,
    game_end_tx: mpsc::UnboundedSender<()>,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
) {
    loop {
        let mut server: RwLockWriteGuard<Server> = tokio::select! {
            _ = cancel_rx.recv() => return,
            lock = server.write() => lock,
        };

        let payload = tokio::select! {
            biased;
            _ = cancel_rx.recv() => return,
            broadcast = broadcast_rx.recv() => {
                if let Ok((ids, msg)) = broadcast {
                    debug!("broadcast to {ids:?}: {msg:?}");

                    for id in ids.clone() {
                        // At this point a data race might occur when two clients are disconnecting
                        // and the server wants to broadcast a LobbyChangeEvent triggered by the first
                        // disconnect. The following call will fail for the second client that disconnected.
                        // It should be no problem ignoring this error.
                        // TODO Refactor: find a better solution
                        if let Err(e) = server.endpoint().send_message(id, msg.clone()) {
                            trace!("failed to send broadcast to {id}: {e}.")
                        }
                    }
                }
                continue;
            },
            event = server.endpoint_mut().next_event() => {
                match event {
                    EndpointEvent::Payload(p) => {
                        debug!("Client {} sent: {p:?}", p.client_id);
                        p
                    }
                    EndpointEvent::Connect(client_id) => {
                        info!("Client {client_id} connected");
                        continue;
                    }
                    EndpointEvent::Disconnect(client_it) => {
                        info!("Client {client_it} disconnected");
                        let mut game = game.write().await;
                        if game.remove_player(client_it) {
                            info!("Ending game due lost connection to player {client_it}...");
                            debug!("Disconnecting all clients...");
                            if let Err(e) = server.endpoint_mut().disconnect_all_clients() {
                                error!("Unable to disconnect all client: {e}");
                            }
                            game_end_tx.send(()).expect("unable to notify the end of the game");
                        }
                        if matches!(game.state, GameState::Lobby) {
                            if let Err(e) = broadcast_lobby_change_event(
                                game.team_a.iter().cloned(),
                                game.team_b.iter().cloned(),
                                game.players.clone(),
                                &broadcast_tx) {
                                error!("unable to broadcast LobbyChangeEvent: {e:#?}");
                            }
                        }

                        continue;
                    }
                    EndpointEvent::UnsupportedVersionMessage{client_id, version} => {
                        debug!("Client {client_id} sent message with unsupported version {version}");
                        let description = format!(
                            "Received message with version {version}, only version {} is supported.",
                            battleship_plus_common::PROTOCOL_VERSION
                        );
                        let message = status_with_msg(StatusCode::UnsupportedVersion, &description);
                        if let Err(error) = server.endpoint_mut().send_message(client_id, message) {
                            warn!("Failed to send UnsupportedVersion response to client {client_id}: {error}");
                        };
                        continue;
                    }
                    EndpointEvent::SocketClosed => {
                        debug!("Server socket closed");
                        continue;
                    }
                    EndpointEvent::NoMorePayloads => {
                        debug!("Endpoint task finished");
                        return;
                    }
                }
            },
        };

        if payload.msg.is_none() {
            continue;
        }

        let ep = server.endpoint_mut();
        match handle_message(
            cfg.clone(),
            ep,
            payload.client_id,
            payload.msg.as_ref().unwrap(),
            &game,
            &game_end_tx,
            &broadcast_tx,
        )
        .await
        {
            Ok(_) => {
                debug!(
                    "handled message from client {}: {payload:?}",
                    payload.client_id
                );
            }
            Err(e) => {
                warn!(
                    "unable to handle message from client {}: {payload:?}: {e}",
                    payload.client_id
                );
            }
        };
    }
}

async fn handle_message(
    cfg: Arc<Config>,
    ep: &mut Endpoint,
    client_id: ClientId,
    msg: &ProtocolMessage,
    game: &Arc<RwLock<Game>>,
    game_end_tx: &mpsc::UnboundedSender<()>,
    broadcast_tx: &tokio::sync::broadcast::Sender<(Vec<ClientId>, ProtocolMessage)>,
) -> Result<(), MessageHandlerError> {
    {
        let g = game.read().await;
        if let Err(reason) = g.state.validate_inbound_message_allowed(msg) {
            ep.send_message(
                client_id,
                status_with_msg(StatusCode::BadRequest, "message not allowed now"),
            )
            .map_err(MessageHandlerError::Network)?;

            return Err(MessageHandlerError::InvalidInboundMessage(reason));
        }
    }

    match msg {
        // common
        ProtocolMessage::ServerConfigRequest(_) => ep
            .send_message(
                client_id,
                status_with_data(
                    StatusCode::Ok,
                    ServerConfigResponse {
                        config: Some(cfg.as_ref().clone()),
                    }
                    .into(),
                ),
            )
            .map_err(MessageHandlerError::Network),

        // lobby
        ProtocolMessage::JoinRequest(props) => {
            {
                let g = game.read().await;
                if g.players.contains_key(&client_id) {
                    ep.send_message(
                        client_id,
                        status_with_msg(StatusCode::BadRequest, "you joined already"),
                    )
                    .map_err(MessageHandlerError::Network)?;
                }
                if g.players.values().any(|p| p.name == props.username) {
                    ep.send_message(
                        client_id,
                        status_with_msg(StatusCode::UsernameIsTaken, "username is already taken"),
                    )
                    .map_err(MessageHandlerError::Network)?;
                }
            }

            let mut g = game.write().await;
            g.players.insert(
                client_id,
                Player {
                    id: client_id,
                    name: props.username.clone(),
                    is_ready: false,
                    quadrant: None,
                },
            );

            place_into_team(client_id, &mut g);
            g.unready_players();

            ep.send_message(
                client_id,
                status_with_data(
                    StatusCode::Ok,
                    JoinResponse {
                        player_id: client_id,
                    }
                    .into(),
                ),
            )
            .map_err(MessageHandlerError::Network)?;

            broadcast_lobby_change_event(
                g.team_a.iter().cloned(),
                g.team_b.iter().cloned(),
                g.players.clone(),
                broadcast_tx,
            )
        }
        ProtocolMessage::TeamSwitchRequest(_) => {
            let action = Action::TeamSwitch {
                player_id: client_id,
            };

            let mut g = game.write().await;
            let state = g.get_state();
            if let Err(e) = state.execute_action(action, &mut g) {
                action_validation_error_reply(ep, client_id, e, game_end_tx)
            } else {
                ep.send_message(
                    client_id,
                    status_with_data(StatusCode::Ok, TeamSwitchResponse {}.into()),
                )
                .map_err(MessageHandlerError::Network)?;

                broadcast_lobby_change_event(
                    g.team_a.iter().cloned(),
                    g.team_b.iter().cloned(),
                    g.players.clone(),
                    broadcast_tx,
                )
            }
        }
        ProtocolMessage::SetReadyStateRequest(props) => {
            let action = Action::SetReady {
                player_id: client_id,
                request: SetReadyStateRequest {
                    ready_state: props.ready_state,
                },
            };

            let mut g = game.write().await;
            let state = g.get_state();
            if let Err(e) = state.execute_action(action, &mut g) {
                action_validation_error_reply(ep, client_id, e, game_end_tx)
            } else {
                ep.send_message(
                    client_id,
                    status_with_data(StatusCode::Ok, SetReadyStateResponse {}.into()),
                )
                .map_err(MessageHandlerError::Network)?;

                broadcast_lobby_change_event(
                    g.team_a.iter().cloned(),
                    g.team_b.iter().cloned(),
                    g.players.clone(),
                    broadcast_tx,
                )?;

                if g.can_change_into_preparation_phase() {
                    g.state = GameState::Preparation;
                    info!("GamePhase: Preparation");
                    let quadrants = g.quadrants();

                    broadcast_game_preparation_start(
                        g.players.values_mut().collect(),
                        quadrants,
                        broadcast_tx,
                    )?;
                }

                Ok(())
            }
        }

        // preparation phase
        ProtocolMessage::SetPlacementRequest(request) => {
            let action = Action::from((client_id, request));

            let mut g = game.write().await;
            g.get_state()
                .execute_action(action, &mut g)
                .map_err(MessageHandlerError::Protocol)?;

            if g.can_change_into_game_phase() {
                info!("GamePhase: InGame");
                g.state = GameState::InGame;
                broadcast_game_start(&g, broadcast_tx)?;
            }

            Ok(())
        }

        // game
        ProtocolMessage::ServerStateRequest(_) => {
            let g = game.read().await;
            let players = match g.players.get(&client_id) {
                Some(p) => p,
                None => {
                    return ep
                        .send_message(
                            client_id,
                            status_with_msg(StatusCode::BadRequest, "not joined"),
                        )
                        .map_err(MessageHandlerError::Network);
                }
            };

            let ship_sets = get_ships_by_team(&g);
            let visible_hostile_ships = (
                g.ships.get_ship_parts_seen_by(&ship_sets.0),
                g.ships.get_ship_parts_seen_by(&ship_sets.1),
            );
            let ship_sets: (Vec<_>, Vec<_>) = (
                ship_sets
                    .0
                    .iter()
                    .map(|ship| create_ship_state(ship))
                    .collect(),
                ship_sets
                    .1
                    .iter()
                    .map(|ship| create_ship_state(ship))
                    .collect(),
            );

            ep.send_message(
                client_id,
                status_with_data(
                    StatusCode::Ok,
                    ServerStateResponse {
                        state: Some(get_server_state_for_player(
                            players,
                            &g,
                            ship_sets,
                            visible_hostile_ships,
                        )),
                    }
                    .into(),
                ),
            )
            .map_err(MessageHandlerError::Network)
        }
        ProtocolMessage::ActionRequest(request) => {
            let action = Action::from((client_id, request));
            if let Action::None = action {
                return Ok(());
            }

            let mut g = game.write().await;
            let /*action_result*/ _ = g
                .get_state()
                .execute_action(action, &mut g)
                .map_err(MessageHandlerError::Protocol);

            // TODO: respond according to the action and the action result
            todo!()
        }

        // received a client-bound message
        _ => {
            warn!("Client {} sent a client-bound message {:?}", client_id, msg);
            ep.send_message(
                client_id,
                status_with_msg(
                    StatusCode::BadRequest,
                    "unable to process client-bound messages",
                ),
            )
            .map_err(MessageHandlerError::Network)?;
            ep.disconnect_client(client_id)
                .map_err(MessageHandlerError::Network)
        }
    }
}

/// Tries to fill the player into the team with capacity left and less players first.
/// Otherwise random.
fn place_into_team(player_id: ClientId, game: &mut RwLockWriteGuard<Game>) {
    let mut a = game.team_a.clone();
    let mut b = game.team_b.clone();

    let mut teams = [
        (&mut a, game.config.team_size_a),
        (&mut b, game.config.team_size_b),
    ];
    teams.sort_by_key(|(team, _)| team.len());
    if let Some((team, _)) = teams
        .iter_mut()
        .find(|(team, size)| team.len() < *size as usize)
    {
        team.insert(player_id);

        game.team_a = a;
        game.team_b = b;
    } else {
        match thread_rng_n(2) {
            0 => game.team_a.insert(player_id),
            1 => game.team_b.insert(player_id),
            _ => panic!("the universe just broke"),
        };
    }
}

fn broadcast_lobby_change_event(
    team_a: impl Iterator<Item = PlayerID>,
    team_b: impl Iterator<Item = PlayerID>,
    players: HashMap<PlayerID, Player>,
    broadcast_tx: &tokio::sync::broadcast::Sender<(Vec<ClientId>, ProtocolMessage)>,
) -> Result<(), MessageHandlerError> {
    let msg = ProtocolMessage::LobbyChangeEvent(LobbyChangeEvent {
        team_state_a: build_team_states(team_a, &players),
        team_state_b: build_team_states(team_b, &players),
    });
    match broadcast_tx.send((players.keys().cloned().collect(), msg)) {
        Ok(_) => Ok(()),
        Err(e) => Err(MessageHandlerError::Broadcast(Box::new(e))),
    }
}

fn broadcast_game_preparation_start(
    players: Vec<&mut Player>,
    mut quadrants: Vec<(u32, u32)>,
    broadcast_tx: &tokio::sync::broadcast::Sender<(Vec<ClientId>, ProtocolMessage)>,
) -> Result<(), MessageHandlerError> {
    if players.len() > quadrants.len() {
        panic!("board has less quadrants than players in the game");
    }

    quadrants.shuffle(&mut thread_rng());

    for p in players {
        p.quadrant = Some(quadrants.pop().unwrap());

        // This function does not send the messages directly through the endpoint struct.
        // Instead it queues them in the broadcast channel.
        // Doing so will ensure that this broadcast will be sent in order with other broadcasts.
        broadcast_tx
            .send((
                vec![p.id],
                PlacementPhase {
                    corner: Some(Coordinate {
                        x: p.quadrant.unwrap().0,
                        y: p.quadrant.unwrap().1,
                    }),
                }
                .into(),
            ))
            .map_err(|e| MessageHandlerError::Broadcast(Box::new(e)))?;
    }

    Ok(())
}

fn broadcast_game_start(
    game: &Game,
    broadcast_tx: &tokio::sync::broadcast::Sender<(Vec<ClientId>, ProtocolMessage)>,
) -> Result<(), MessageHandlerError> {
    let (team_ships_a, team_ships_b) = get_ships_by_team(game);

    let visible_hostile_ships_a = game.ships.get_ship_parts_seen_by(&team_ships_a);
    let visible_hostile_ships_b = game.ships.get_ship_parts_seen_by(&team_ships_b);

    let team_ships_a: Vec<_> = team_ships_a
        .iter()
        .map(|ship| create_ship_state(ship))
        .collect();
    let team_ships_b: Vec<_> = team_ships_b
        .iter()
        .map(|ship| create_ship_state(ship))
        .collect();

    for (&id, player) in game.players.iter() {
        // This function does not send the messages directly through the endpoint struct.
        // Instead it queues them in the broadcast channel.
        // Doing so will ensure that this broadcast will be sent in order with other broadcasts.
        broadcast_tx
            .send((
                vec![id],
                game_state_for_player(
                    player,
                    game,
                    (team_ships_a.clone(), team_ships_b.clone()),
                    (
                        visible_hostile_ships_a.clone(),
                        visible_hostile_ships_b.clone(),
                    ),
                )
                .into(),
            ))
            .map_err(|e| MessageHandlerError::Broadcast(Box::new(e)))?;
    }

    Ok(())
}

fn get_ships_by_team(game: &Game) -> (Vec<&Ship>, Vec<&Ship>) {
    game.ships.iter_ships().fold(
        (Vec::new(), Vec::new()),
        |(mut team_ships_a, mut team_ships_b), (&ship_id, ship)| {
            let player_id = ship_id.0;
            match (
                game.team_a.contains(&player_id),
                game.team_b.contains(&player_id),
            ) {
                (true, false) => team_ships_a.push(ship),
                (false, true) => team_ships_b.push(ship),
                _ => unreachable!(),
            }

            (team_ships_a, team_ships_b)
        },
    )
}

fn get_server_state_for_player(
    player: &Player,
    game: &Game,
    (team_ships_a, team_ships_b): (Vec<ShipState>, Vec<ShipState>),
    (visible_hostile_ships_a, visible_hostile_ships_b): (Vec<Coordinate>, Vec<Coordinate>),
) -> ServerState {
    let action_points_left = match game.turn {
        Some(Turn {
            player_id,
            action_points_left,
        }) if player_id == player.id => action_points_left,
        _ => 0,
    };

    match (
        game.team_a.contains(&player.id),
        game.team_b.contains(&player.id),
    ) {
        (true, false) => ServerState {
            team_ships: team_ships_a,
            action_points: action_points_left,
            visible_hostile_ships: visible_hostile_ships_a,
        },
        (false, true) => ServerState {
            team_ships: team_ships_b,
            action_points: action_points_left,
            visible_hostile_ships: visible_hostile_ships_b,
        },
        _ => unreachable!(),
    }
}

fn game_state_for_player(
    player: &Player,
    game: &Game,
    (team_ships_a, team_ships_b): (Vec<ShipState>, Vec<ShipState>),
    (visible_hostile_ships_a, visible_hostile_ships_b): (Vec<Coordinate>, Vec<Coordinate>),
) -> GameStart {
    GameStart {
        state: Some(get_server_state_for_player(
            player,
            game,
            (team_ships_a, team_ships_b),
            (visible_hostile_ships_a, visible_hostile_ships_b),
        )),
    }
}

fn build_team_states(
    team_player_ids: impl Iterator<Item = PlayerID>,
    players: &HashMap<PlayerID, Player>,
) -> Vec<PlayerLobbyState> {
    team_player_ids
        .map(|id| players.get(&id))
        .map(|player| {
            let player = player.expect("player id in a team is not found in the game");
            PlayerLobbyState {
                ready: player.is_ready,
                player_id: player.id,
                name: player.name.clone(),
            }
        })
        .collect()
}

fn action_validation_error_reply(
    ep: &mut Endpoint,
    client_id: ClientId,
    error: ActionExecutionError,
    game_end_tx: &mpsc::UnboundedSender<()>,
) -> Result<(), MessageHandlerError> {
    match error.clone() {
        ActionExecutionError::Validation(e) => match e {
            ActionValidationError::NonExistentPlayer { id } =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::BadRequest, format!("player id {id} does not exist").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::NonExistentShip { id } =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::BadRequest, format!("ship ({}, {}) does not exist", id.0, id.1).as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::Cooldown { remaining_rounds } =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::InsufficientResources, format!("requested action is on cooldown for the next {remaining_rounds} rounds").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::InsufficientPoints { required } =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::InsufficientResources, format!("insufficient action points for requested action ({required})").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::Unreachable =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::InvalidMove, "request target is unreachable"))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::OutOfMap =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::InvalidMove, "request target is out of map"))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::InvalidShipPlacement(e) =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::BadRequest, format!("ship placement is invalid: {e}").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::NotPlayersTurn =>
                ep.send_message(client_id,
                                status_with_msg(StatusCode::InvalidMove, "not your turn"))
                    .map_err(MessageHandlerError::Network),
        },
        ActionExecutionError::OutOfState(state) => {
            ep.send_message(client_id,
                            status_with_msg(StatusCode::BadRequest, format!("request not allowed in {state}").as_str()))
                .map_err(MessageHandlerError::Network)?;
            ep.disconnect_client(client_id)
                .map_err(MessageHandlerError::Network)
        }
        ActionExecutionError::InconsistentState(s) => {
            if let Err(e) = ep.broadcast_message(status_with_msg(StatusCode::ServerError, format!("server detected an inconsistent state: {s}").as_str())) {
                error!("detected inconsistent state: {e}");
            }

            game_end_tx.send(()).expect("failed to end game");
            Err(MessageHandlerError::Protocol(error))
        }
    }
}

fn status_with_msg(code: StatusCode, msg: &str) -> ProtocolMessage {
    status_response(code, msg, None)
}

fn status_with_data(code: StatusCode, data: Data) -> ProtocolMessage {
    status_response(code, "", Some(data))
}

fn status_response(code: StatusCode, message: &str, data: Option<Data>) -> ProtocolMessage {
    StatusMessage {
        code: code.into(),
        message: message.to_string(),
        data,
    }
    .into()
}

fn create_ship_state(ship: &Ship) -> ShipState {
    let player_id = ship.get_player_id();
    ShipState {
        ship_type: ship.ship_type() as i32,
        position: Some(Coordinate {
            x: ship.position().0 as u32,
            y: ship.position().1 as u32,
        }),
        direction: <Orientation as Into<Direction>>::into(ship.orientation()) as i32,
        health: ship.data().health,
        owner_id: player_id,
        remaining_cooldown_move: ship
            .cool_downs()
            .iter()
            .find(|cd| matches!(cd, Cooldown::Movement { .. }))
            .map_or(0, |cd| cd.remaining_rounds()),
        remaining_cooldown_rotate: ship
            .cool_downs()
            .iter()
            .find(|cd| matches!(cd, Cooldown::Rotate { .. }))
            .map_or(0, |cd| cd.remaining_rounds()),
        remaining_cooldown_shoot: ship
            .cool_downs()
            .iter()
            .find(|cd| matches!(cd, Cooldown::Cannon { .. }))
            .map_or(0, |cd| cd.remaining_rounds()),
        remaining_cooldown_ability: ship
            .cool_downs()
            .iter()
            .find(|cd| matches!(cd, Cooldown::Ability { .. }))
            .map_or(0, |cd| cd.remaining_rounds()),
    }
}

#[derive(Debug)]
pub enum MessageHandlerError {
    Network(QuinnetError),
    Protocol(ActionExecutionError),
    Broadcast(Box<tokio::sync::broadcast::error::SendError<(Vec<ClientId>, ProtocolMessage)>>),
    InvalidInboundMessage(String),
}

impl Display for MessageHandlerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("MessageHandlerError: ")?;
        match self {
            MessageHandlerError::Network(e) => f.write_str(format!("{e}").as_str()),
            MessageHandlerError::Protocol(e) => f.write_str(format!("{e:?}").as_str()),
            MessageHandlerError::Broadcast(e) => f.write_str(format!("{e:?}").as_str()),
            MessageHandlerError::InvalidInboundMessage(msg) => f.write_str(msg),
        }
    }
}
