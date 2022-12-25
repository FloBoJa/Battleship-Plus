use std::net::Ipv6Addr;
use std::sync::Arc;

use log::{error, info, trace, warn};
use tokio::sync::{mpsc, Mutex, MutexGuard, RwLock};

use battleship_plus_common::messages::{JoinResponse, ProtocolMessage, ServerConfigResponse, StatusMessage};
use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::types::Config;
use bevy_quinnet::server::{ClientPayload, Endpoint, Server, ServerConfigurationData};
use bevy_quinnet::server::certificate::CertificateRetrievalMode;
use bevy_quinnet::shared::{ClientId, QuinnetError};

use crate::config_provider::ConfigProvider;
use crate::game::data::{Game, Player};

pub(crate) async fn server_task(cfg: Arc<dyn ConfigProvider>) {
    let addr6 = cfg.server_config().game_address_v6;
    let addr4 = cfg.server_config().game_address_v4;

    let mut server6 = Server::new_standalone();
    let server6 = match server6.start_endpoint(
        ServerConfigurationData::new(addr6.ip().to_string(), addr6.port(), Ipv6Addr::UNSPECIFIED.to_string()),
        CertificateRetrievalMode::LoadFromFileOrGenerateSelfSigned {
            cert_file: "./certificate6.pem".to_string(),
            key_file: "./key6.pem".to_string(),
            save_on_disk: true,
        },
    ) {
        Ok(_) => Some(Arc::new(Mutex::new(server6))),
        Err(e) => {
            error!("Unable to listen on {addr6}: {e}");
            panic!("Unable to listen on {addr6}: {e}")
        }
    };

    info!("Endpoint initialized");

    loop {
        let game = Arc::new(RwLock::new(
            Game::new(
                cfg.game_config().board_size,
                cfg.game_config().team_size_a,
                cfg.game_config().team_size_b))
        );
        let (game_end_tx, mut game_end_rx) = mpsc::unbounded_channel();

        let handles: Vec<_> =
            [&server6] // add other endpoints here (e.g. in case the system does not support dual stack ports)
                .iter()
                .filter(|e| e.is_some())
                .map(|server| {
                    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();

                    (tokio::spawn(endpoint_task(
                        cfg.game_config(),
                        server.as_ref().unwrap().clone(),
                        game.clone(),
                        game_end_tx.clone(),
                        cancel_rx)
                    ), cancel_tx)
                })
                .collect();

        let _ = game_end_rx.recv().await;

        for h in handles {
            let _ = h.1.send(());

            if let Err(e) = h.0.await {
                error!("server task finished with an error {e}");
            }
        }
    }
}

async fn endpoint_task(cfg: Arc<Config>, server: Arc<Mutex<Server>>, game: Arc<RwLock<Game>>, game_end_tx: mpsc::UnboundedSender<()>, mut cancel_rx: mpsc::UnboundedReceiver<()>) {
    loop {
        let mut server: MutexGuard<Server> = tokio::select! {
                _ = cancel_rx.recv() => return,
                lock = server.lock() => lock,
            };

        let ep = server.endpoint_mut();
        let payload: ClientPayload = tokio::select! {
                _ = cancel_rx.recv() => return,
                p = ep.receive_payload_waiting() => {
                    match p {
                        Some(p) => p,
                        None => return,
                    }
                },
            };

        if payload.msg.is_none() {
            continue;
        }
        match handle_message(cfg.clone(), ep, payload.client_id, payload.msg.as_ref().unwrap(), &game, &game_end_tx) {
            Ok(_) => {
                trace!("handled message from {:?}", payload);
            }
            Err(e) => {
                warn!("unable to handle message {:?}: {e}", payload);
            }
        };
    }
}

async fn handle_message(cfg: Arc<Config>,
                        ep: &mut Endpoint,
                        client_id: ClientId,
                        msg: &ProtocolMessage,
                        game: &Arc<RwLock<Game>>,
                        game_end_tx: &mpsc::UnboundedSender<()>) -> Result<(), QuinnetError> {
    match msg {
        // common
        ProtocolMessage::ServerConfigRequest(_) =>
            ep.send_message(client_id, ProtocolMessage::ServerConfigResponse(ServerConfigResponse {
                config: Some(cfg.as_ref().clone()),
            })),

        // lobby
        ProtocolMessage::JoinRequest(props) => {
            {
                let g = game.read().await;
                if g.players.contains_key(&client_id) {
                    ep.send_message(client_id,
                                    status_msg(400, "you joined already"))?;
                }
                if g.players.values().any(|p| p.name == props.username) {
                    ep.send_message(client_id,
                                    status_msg(441, "username is already taken"))?;
                }
            }

            let g = game.write().await;
            g.players.insert(client_id, Player {
                id: client_id,
                name: props.username.clone(),
                action_points: 0,
                is_ready: false,
            });

            ep.send_message(client_id, ProtocolMessage::JoinResponse(
                JoinResponse {
                    player_id: client_id,
                }
            ))
        }
        ProtocolMessage::TeamSwitchRequest(_) => {
            // TODO: TeamSwitchRequest
            todo!()
        }
        ProtocolMessage::SetReadyStateRequest(_) => {
            // TODO: SetReadyStateRequest
            todo!()
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
        ProtocolMessage::ActionRequest(_) => {
            // TODO: ActionRequest
            todo!()
        }

        // received a client-bound message
        _ => {
            warn!("Client {} sent a client-bound message {:?}", client_id, msg);
            ep.send_message(client_id,
                            status_msg(400, "unable to process client-bound messages"))?;
            ep.disconnect_client(client_id)
        }
    }
}

fn status_msg(code: u32, msg: &str) -> ProtocolMessage {
    ProtocolMessage::StatusMessage(StatusMessage {
        code, // bad request
        data: Some(Data::Message(msg.to_string())),
    })
}