use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use log::{debug, error, info, warn};
use tokio::macros::support::thread_rng_n;
use tokio::sync::{mpsc, RwLock, RwLockWriteGuard};

use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::messages::{
    JoinResponse, LobbyChangeEvent, ProtocolMessage, ServerConfigResponse, SetReadyStateRequest,
    SetReadyStateResponse, StatusMessage, TeamSwitchResponse,
};
use battleship_plus_common::types::{Config, PlayerLobbyState};
use bevy_quinnet::server::certificate::CertificateRetrievalMode;
use bevy_quinnet::server::{Endpoint, EndpointEvent, Server, ServerConfigurationData};
use bevy_quinnet::shared::{ClientId, QuinnetError};

use crate::config_provider::ConfigProvider;
use crate::game::actions::{Action, ActionExecutionError, ActionValidationError};
use crate::game::data::{Game, Player, PlayerID};
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
    let ascii_host: String = cfg
        .game_config()
        .server_name
        .chars()
        .filter(|c| c.is_ascii())
        .collect();

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
        let game = Arc::new(RwLock::new(Game::new(
            cfg.game_config().board_size,
            cfg.game_config().team_size_a,
            cfg.game_config().team_size_b,
        )));
        let (game_end_tx, mut game_end_rx) = mpsc::unbounded_channel();

        let (broadcast_tx, broadcast_rx): BroadcastChannel = tokio::sync::broadcast::channel(128);

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

        tokio::select! {
            _ = game_end_rx.recv() => {},
            _ = stop.recv() => return,
        }

        for h in handles {
            h.1.send(())
                .expect("unable to notify endpoint tasks to cancel");

            if let Err(e) = h.0.await {
                error!("server task finished with an error {e}");
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
            _ = cancel_rx.recv() => return,
            broadcast = broadcast_rx.recv() => {
                if let Ok((ids, msg)) = broadcast {
                    if let Err(e) = server.endpoint().send_group_message(ids.iter(), msg.clone()) {
                        warn!("unable to broadcast {msg:?}: {e}");
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
                            let _ = game_end_tx.send(());
                        }
                        if matches!(game.state, GameState::Lobby) {
                            if let Err(e) = broadcast_lobby_change_event(
                                game.team_a.iter().cloned(),
                                game.team_b.iter().cloned(),
                                game.players.clone(),
                                &broadcast_tx).await {
                                error!("unable to broadcast LobbyChangeEvent: {e}");
                            }
                        }

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
    match msg {
        // common
        ProtocolMessage::ServerConfigRequest(_) => ep
            .send_message(
                client_id,
                status_with_data(
                    200,
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
                    ep.send_message(client_id, status_with_msg(400, "you joined already"))
                        .map_err(MessageHandlerError::Network)?;
                }
                if g.players.values().any(|p| p.name == props.username) {
                    ep.send_message(client_id, status_with_msg(441, "username is already taken"))
                        .map_err(MessageHandlerError::Network)?;
                }
            }

            let mut g = game.write().await;
            g.players.insert(
                client_id,
                Player {
                    id: client_id,
                    name: props.username.clone(),
                    action_points: 0,
                    is_ready: false,
                },
            );

            place_into_team(client_id, &mut g);
            g.unready_players();

            ep.send_message(
                client_id,
                status_with_data(
                    200,
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
            .await
        }
        ProtocolMessage::TeamSwitchRequest(_) => {
            let action = Action::TeamSwitch {
                player_id: client_id,
            };

            let mut g = game.write().await;
            let state = game.write().await.get_state();
            if let Err(e) = state.execute_action(action, &mut g) {
                action_validation_error_reply(ep, client_id, e, game_end_tx)
            } else {
                ep.send_message(
                    client_id,
                    status_with_data(200, TeamSwitchResponse {}.into()),
                )
                .map_err(MessageHandlerError::Network)?;

                broadcast_lobby_change_event(
                    g.team_a.iter().cloned(),
                    g.team_b.iter().cloned(),
                    g.players.clone(),
                    broadcast_tx,
                )
                .await
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
            let state = game.write().await.get_state();
            if let Err(e) = state.execute_action(action, &mut g) {
                action_validation_error_reply(ep, client_id, e, game_end_tx)
            } else {
                ep.send_message(
                    client_id,
                    status_with_data(200, SetReadyStateResponse {}.into()),
                )
                .map_err(MessageHandlerError::Network)?;

                broadcast_lobby_change_event(
                    g.team_a.iter().cloned(),
                    g.team_b.iter().cloned(),
                    g.players.clone(),
                    broadcast_tx,
                )
                .await
            }
        }

        // preparation phase
        ProtocolMessage::SetPlacementRequest(_) => {
            // TODO: SetPlacementRequest
            todo!()
        }

        // game
        ProtocolMessage::ServerStateRequest(_) => {
            // TODO: ServerStateRequest
            todo!()
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
                status_with_msg(400, "unable to process client-bound messages"),
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

    let mut teams = [(&mut a, game.team_a_size), (&mut b, game.team_b_size)];
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

async fn broadcast_lobby_change_event(
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
                                status_with_msg(400, format!("player id {id} does not exist").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::NonExistentShip { id } =>
                ep.send_message(client_id,
                                status_with_msg(400, format!("ship ({}, {}) does not exist", id.0, id.1).as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::Cooldown { remaining_rounds } =>
                ep.send_message(client_id,
                                status_with_msg(471, format!("requested action is on cooldown for the next {remaining_rounds} rounds").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::InsufficientPoints { required } =>
                ep.send_message(client_id,
                                status_with_msg(471, format!("insufficient action points for requested action ({required})").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::Unreachable =>
                ep.send_message(client_id,
                                status_with_msg(472, "request target is unreachable".to_string().as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::OutOfMap =>
                ep.send_message(client_id,
                                status_with_msg(472, "request target is out of map".to_string().as_str()))
                    .map_err(MessageHandlerError::Network),
        },
        ActionExecutionError::OutOfState(state) => {
            ep.send_message(client_id,
                            status_with_msg(400, format!("request not allowed in {state}").as_str()))
                .map_err(MessageHandlerError::Network)?;
            ep.disconnect_client(client_id)
                .map_err(MessageHandlerError::Network)
        }
        ActionExecutionError::InconsistentState(s) => {
            if let Err(e) = ep.broadcast_message(status_with_msg(500, format!("server detected an inconsistent state: {s}").as_str())) {
                error!("detected inconsistent state: {e}");
            }

            game_end_tx.send(()).expect("failed to end game");
            Err(MessageHandlerError::Protocol(error))
        }
    }
}

fn status_with_msg(code: u32, msg: &str) -> ProtocolMessage {
    status_response(code, msg, None)
}

fn status_with_data(code: u32, data: Data) -> ProtocolMessage {
    status_response(code, "", Some(data))
}

fn status_response(code: u32, message: &str, data: Option<Data>) -> ProtocolMessage {
    ProtocolMessage::StatusMessage(StatusMessage {
        code, // bad request
        message: message.to_string(),
        data,
    })
}

#[derive(Debug)]
pub enum MessageHandlerError {
    Network(QuinnetError),
    Protocol(ActionExecutionError),
    Broadcast(Box<tokio::sync::broadcast::error::SendError<(Vec<ClientId>, ProtocolMessage)>>),
}

impl Display for MessageHandlerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("MessageHandlerError: ")?;
        match self {
            MessageHandlerError::Network(e) => f.write_str(format!("{e}").as_str()),
            MessageHandlerError::Protocol(e) => f.write_str(format!("{e:?}").as_str()),
            MessageHandlerError::Broadcast(e) => f.write_str(format!("{e:?}").as_str()),
        }
    }
}
