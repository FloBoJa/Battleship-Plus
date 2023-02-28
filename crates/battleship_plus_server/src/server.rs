use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, trace, warn};
use rand::seq::SliceRandom;
use rand::thread_rng;
use tokio::macros::support::thread_rng_n;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{mpsc, RwLock, RwLockWriteGuard};

use battleship_plus_common::game::ship::{Cooldown, GetShipID, Orientation, Ship};
use battleship_plus_common::game::{ActionValidationError, PlayerID};
use battleship_plus_common::messages::ship_action_request::ActionProperties;
use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::messages::{
    ship_action_event, DestructionEvent, GameOverEvent, GameStart, HitEvent, JoinResponse,
    LobbyChangeEvent, NextTurn, PlacementPhase, ProtocolMessage, ServerConfigResponse,
    ServerStateResponse, SetReadyStateRequest, SetReadyStateResponse, ShipActionEvent,
    ShipActionResponse, SplashEvent, StatusCode, StatusMessage, TeamSwitchResponse, VisionEvent,
};
use battleship_plus_common::types::{
    Config, Coordinate, Direction, GameEndReason, PlayerLobbyState, ServerState, ShipState, Teams,
};
use battleship_plus_common::{protocol_name, protocol_name_with_version};
use bevy_quinnet_server::certificate::CertificateRetrievalMode;
use bevy_quinnet_server::{
    ClientId, Endpoint, EndpointEvent, QuinnetError, Server, ServerConfigurationData,
};

use crate::config_provider::ConfigProvider;
use crate::game::actions::{Action, ActionExecutionError, ActionResult};
use crate::game::data::{Game, GameResult, Player, Turn};
use crate::game::states::GameState;
use crate::tasks::{upgrade_oneshot, TaskControl};

pub fn spawn_server_task(cfg: Arc<dyn ConfigProvider + Send + Sync>) -> TaskControl {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(server_task(cfg, rx));
    TaskControl::new(tx, handle)
}

type BroadcastChannel = (
    Sender<(Vec<ClientId>, ProtocolMessage)>,
    Receiver<(Vec<ClientId>, ProtocolMessage)>,
);

pub async fn server_task(
    cfg: Arc<dyn ConfigProvider + Send + Sync>,
    stop: tokio::sync::oneshot::Receiver<()>,
) {
    let mut stop = upgrade_oneshot(stop);

    let alpns = vec![
        protocol_name_with_version(),
        protocol_name(),
        "http/0.9".to_string(), // keep to maximize compatibility
        "http/1.0".to_string(), // keep to maximize compatibility
        "http/1.1".to_string(), // keep to maximize compatibility
        "http/3".to_string(),   // keep to maximize compatibility
    ];

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
    let server6 = match server6.start_endpoint_with_alpn(
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
        alpns.clone(),
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
            server4 = match s4.start_endpoint_with_alpn(
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
                alpns.clone(),
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

                            server.endpoint().try_broadcast_message(GameOverEvent {
                                reason: GameEndReason::Disconnect.into(),
                                winner: Teams::None.into(),
                            }.into());

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

                if let Err(e) = ep.send_message(
                    payload.client_id,
                    status_with_msg(StatusCode::BadRequest, e.to_string().as_str()),
                ) {
                    error!(
                        "unable to send error message to {}: {}",
                        payload.client_id, e
                    )
                }
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
    game_end_tx: &UnboundedSender<()>,
    broadcast_tx: &Sender<(Vec<ClientId>, ProtocolMessage)>,
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
                g.advance_turn();
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
                g.ships.get_ship_parts_seen_by(
                    &ship_sets.0.iter().map(|ship| ship.id()).collect::<Vec<_>>(),
                ),
                g.ships.get_ship_parts_seen_by(
                    &ship_sets.1.iter().map(|ship| ship.id()).collect::<Vec<_>>(),
                ),
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

            let mut g = game.write().await;
            let turn = match g.turn.as_ref() {
                None => {
                    return Err(MessageHandlerError::Protocol(
                        ActionExecutionError::OutOfState(g.state),
                    ))
                }
                Some(t) => t,
            };

            if turn.player_id != client_id {
                return Err(MessageHandlerError::Protocol(
                    ActionExecutionError::Validation(ActionValidationError::NotPlayersTurn),
                ));
            }

            let team = match (
                g.team_a.contains(&turn.player_id),
                g.team_b.contains(&turn.player_id),
            ) {
                (true, false) => &g.team_a,
                (false, true) => &g.team_b,
                _ => unreachable!(),
            }
            .iter()
            .cloned()
            .collect::<Vec<_>>();
            if let Action::None = action {
                let turn = g.clear_temp_vision_and_advance_turn(team.as_slice(), broadcast_tx)?;
                return ep
                    .broadcast_message(
                        NextTurn {
                            next_player_id: turn.player_id,
                            position_in_queue: 0, //TODO
                        }
                        .into(),
                    )
                    .map_err(MessageHandlerError::Network);
            }

            let action_result = g
                .get_state()
                .execute_action(action, &mut g)
                .map_err(MessageHandlerError::Protocol)?;

            broadcast_tx
                .send((
                    team,
                    ShipActionEvent {
                        ship_number: request.ship_number,
                        action_properties: request.action_properties.clone().map(|p| match p {
                            ActionProperties::MoveProperties(p) => {
                                ship_action_event::ActionProperties::MoveProperties(p)
                            }
                            ActionProperties::ShootProperties(p) => {
                                ship_action_event::ActionProperties::ShootProperties(p)
                            }
                            ActionProperties::RotateProperties(p) => {
                                ship_action_event::ActionProperties::RotateProperties(p)
                            }
                            ActionProperties::TorpedoProperties(p) => {
                                ship_action_event::ActionProperties::TorpedoProperties(p)
                            }
                            ActionProperties::ScoutPlaneProperties(p) => {
                                ship_action_event::ActionProperties::ScoutPlaneProperties(p)
                            }
                            ActionProperties::MultiMissileProperties(p) => {
                                ship_action_event::ActionProperties::MultiMissileProperties(p)
                            }
                            ActionProperties::PredatorMissileProperties(p) => {
                                ship_action_event::ActionProperties::PredatorMissileProperties(p)
                            }
                            ActionProperties::EngineBoostProperties(p) => {
                                ship_action_event::ActionProperties::EngineBoostProperties(p)
                            }
                        }),
                    }
                    .into(),
                ))
                .map_err(|e| MessageHandlerError::Broadcast(e.into()))?;

            match action_result {
                ActionResult::None => Ok(()),
                ActionResult::Single {
                    lost_vision_at,
                    temp_vision_at,
                    gain_vision_at,
                    ships_destroyed,
                    inflicted_damage_at,
                    gain_enemy_vision,
                    lost_enemy_vision,
                    splash_tiles,
                    ..
                } => {
                    match handle_action_result(
                        &mut g,
                        client_id,
                        broadcast_tx,
                        gain_vision_at,
                        temp_vision_at,
                        lost_vision_at,
                        gain_enemy_vision,
                        lost_enemy_vision,
                        inflicted_damage_at,
                        ships_destroyed,
                        splash_tiles,
                    ) {
                        Ok(GameResult::Pending) => Ok(()),
                        Ok(result) => broadcast_game_result(
                            result,
                            g.players.keys().cloned().collect(),
                            broadcast_tx,
                            game_end_tx,
                        ),
                        Err(e) => Err(e),
                    }
                }
                ActionResult::Multiple(results) => {
                    for res in results {
                        match res {
                            Ok(ActionResult::Single {
                                lost_vision_at,
                                temp_vision_at,
                                gain_vision_at,
                                ships_destroyed,
                                inflicted_damage_at,
                                gain_enemy_vision,
                                lost_enemy_vision,
                                splash_tiles,
                                ..
                            }) => match handle_action_result(
                                &mut g,
                                client_id,
                                broadcast_tx,
                                gain_vision_at,
                                temp_vision_at,
                                lost_vision_at,
                                gain_enemy_vision,
                                lost_enemy_vision,
                                inflicted_damage_at,
                                ships_destroyed,
                                splash_tiles,
                            ) {
                                Ok(GameResult::Pending) => Ok(()),
                                Ok(result) => broadcast_game_result(
                                    result,
                                    g.players.keys().cloned().collect(),
                                    broadcast_tx,
                                    game_end_tx,
                                ),
                                Err(e) => Err(e),
                            }?,
                            Err(_) => ep
                                .send_message(
                                    client_id,
                                    status_with_msg(StatusCode::OkWithWarning, "OK, with warning"),
                                )
                                .map_err(MessageHandlerError::Network)?,
                            _ => unreachable!(),
                        }
                    }

                    Ok(())
                }
            }?;

            ep.send_message(
                client_id,
                status_with_data(StatusCode::Ok, ShipActionResponse::default().into()),
            )
            .map_err(MessageHandlerError::Network)?;

            Ok(())
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

#[allow(clippy::too_many_arguments)]
fn handle_action_result(
    g: &mut Game,
    client_id: ClientId,
    broadcast_tx: &tokio::sync::broadcast::Sender<(Vec<ClientId>, ProtocolMessage)>,
    gain_vision_at: HashSet<Coordinate>,
    temp_vision_at: HashSet<Coordinate>,
    lost_vision_at: HashSet<Coordinate>,
    gain_enemy_vision: HashSet<Coordinate>,
    lost_enemy_vision: HashSet<Coordinate>,
    inflicted_damage_at: HashMap<Coordinate, u32>,
    ships_destroyed: Vec<Ship>,
    splash_tiles: HashSet<Coordinate>,
) -> Result<GameResult, MessageHandlerError> {
    // vision events
    let ally_vision_gain = gain_vision_at
        .iter()
        .chain(
            temp_vision_at
                .iter()
                .filter(|&c| g.turn.as_mut().unwrap().temp_vision.insert(c.clone())),
        )
        .cloned()
        .collect::<Vec<_>>();

    let (allies, enemies) = match (g.team_a.contains(&client_id), g.team_b.contains(&client_id)) {
        (true, false) => (&g.team_a, &g.team_b),
        (false, true) => (&g.team_b, &g.team_a),
        _ => unreachable!(),
    };

    if !ally_vision_gain.is_empty() || !lost_vision_at.is_empty() {
        if let Err(e) = broadcast_tx.send((
            allies.iter().cloned().collect(),
            VisionEvent {
                discovered_ship_fields: ally_vision_gain,
                vanished_ship_fields: lost_vision_at.iter().cloned().collect(),
            }
            .into(),
        )) {
            return Err(MessageHandlerError::Broadcast(e.into()));
        }
    }

    if !gain_enemy_vision.is_empty() || !lost_enemy_vision.is_empty() {
        if let Err(e) = broadcast_tx.send((
            enemies.iter().cloned().collect(),
            VisionEvent {
                discovered_ship_fields: gain_enemy_vision.iter().cloned().collect(),
                vanished_ship_fields: lost_enemy_vision.iter().cloned().collect(),
            }
            .into(),
        )) {
            return Err(MessageHandlerError::Broadcast(e.into()));
        }
    }

    // hit events
    for (c, &damage) in inflicted_damage_at.iter() {
        if let Err(e) = broadcast_tx.send((
            g.players.keys().into_iter().cloned().collect(),
            HitEvent {
                coordinate: c.clone().into(),
                damage,
            }
            .into(),
        )) {
            return Err(MessageHandlerError::Broadcast(e.into()));
        }
    }

    // destruction events
    for ship in ships_destroyed.iter() {
        if let Err(e) = broadcast_tx.send((
            g.players.keys().into_iter().cloned().collect(),
            DestructionEvent {
                coordinate: Some(Coordinate {
                    x: ship.data().pos_x as u32,
                    y: ship.data().pos_y as u32,
                }),
                direction: Direction::from(ship.data().orientation).into(),
                ship_number: ship.data().id.1,
                owner: ship.data().id.0,
            }
            .into(),
        )) {
            return Err(MessageHandlerError::Broadcast(e.into()));
        }
    }

    // splash events
    if !splash_tiles.is_empty() {
        if let Err(e) = broadcast_tx.send((
            g.players.keys().into_iter().cloned().collect(),
            SplashEvent {
                coordinate: splash_tiles.iter().cloned().collect(),
            }
            .into(),
        )) {
            return Err(MessageHandlerError::Broadcast(e.into()));
        }
    }

    Ok(g.game_result())
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
    mut quadrants: Vec<(u32, u32, u32)>,
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
                    quadrant_size: p.quadrant.unwrap().2,
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

    let visible_hostile_ships_a = game.ships.get_ship_parts_seen_by(
        &team_ships_a
            .iter()
            .map(|ship| ship.id())
            .collect::<Vec<_>>(),
    );
    let visible_hostile_ships_b = game.ships.get_ship_parts_seen_by(
        &team_ships_b
            .iter()
            .map(|ship| ship.id())
            .collect::<Vec<_>>(),
    );

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
                game_start_for_player(
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

    let turn = game.turn.as_ref().unwrap();
    broadcast_tx
        .send((
            vec![turn.player_id],
            ProtocolMessage::NextTurn(NextTurn {
                next_player_id: turn.player_id,
                position_in_queue: 0, // TODO
            }),
        ))
        .map_err(|e| MessageHandlerError::Broadcast(Box::new(e)))?;

    Ok(())
}

fn broadcast_game_result(
    result: GameResult,
    players: Vec<ClientId>,
    broadcast_tx: &Sender<(Vec<ClientId>, ProtocolMessage)>,
    game_end_tx: &UnboundedSender<()>,
) -> Result<(), MessageHandlerError> {
    match result {
        GameResult::Pending => return Ok(()),
        GameResult::Draw => broadcast_tx
            .send((
                players,
                GameOverEvent {
                    reason: GameEndReason::Regular.into(),
                    winner: Teams::None.into(),
                }
                .into(),
            ))
            .map(|_| ())
            .map_err(|e| MessageHandlerError::Broadcast(e.into()))?,
        GameResult::Win(team) => broadcast_tx
            .send((
                players,
                GameOverEvent {
                    reason: GameEndReason::Regular.into(),
                    winner: team.into(),
                }
                .into(),
            ))
            .map(|_| ())
            .map_err(|e| MessageHandlerError::Broadcast(e.into()))?,
    }

    game_end_tx.send(()).expect("unable to end game");

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
            ..
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

fn game_start_for_player(
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
                ep.send_message(client_id, status_with_msg(StatusCode::BadRequest, format!("player id {id} does not exist").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::NonExistentShip { id } =>
                ep.send_message(client_id, status_with_msg(StatusCode::BadRequest, format!("ship ({}, {}) does not exist", id.0, id.1).as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::Cooldown { remaining_rounds } =>
                ep.send_message(client_id, status_with_msg(StatusCode::InsufficientResources, format!("requested action is on cooldown for the next {remaining_rounds} rounds").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::InsufficientPoints { required } =>
                ep.send_message(client_id, status_with_msg(StatusCode::InsufficientResources, format!("insufficient action points for requested action ({required})").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::Unreachable =>
                ep.send_message(client_id, status_with_msg(StatusCode::InvalidMove, "request target is unreachable"))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::OutOfMap =>
                ep.send_message(client_id, status_with_msg(StatusCode::InvalidMove, "request target is out of map"))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::InvalidShipPlacement(e) =>
                ep.send_message(client_id, status_with_msg(StatusCode::BadRequest, format!("ship placement is invalid: {e}").as_str()))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::NotPlayersTurn =>
                ep.send_message(client_id, status_with_msg(StatusCode::InvalidMove, "not your turn"))
                    .map_err(MessageHandlerError::Network),
            ActionValidationError::InvalidShipType => ep.send_message(client_id, status_with_msg(StatusCode::InvalidMove, "the selected ship is not able to perform the requested action"))
                .map_err(MessageHandlerError::Network),
            ActionValidationError::Ignored => ep.send_message(client_id, status_with_msg(StatusCode::InvalidMove, "the requested action was ignored"))
                .map_err(MessageHandlerError::Network),
        },
        ActionExecutionError::OutOfState(state) => {
            debug!("client: {client_id} sent an request not allowed in {state}. Aborting connection..");
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
        ActionExecutionError::BadRequest(explanation) => {
            debug!("client: {client_id} sent an invalid request ({explanation}). Aborting connection..");
            ep.send_message(client_id, status_with_msg(StatusCode::BadRequest, "bad request"))
                .map_err(MessageHandlerError::Network)?;
            ep.disconnect_client(client_id)
                .map_err(MessageHandlerError::Network)
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
