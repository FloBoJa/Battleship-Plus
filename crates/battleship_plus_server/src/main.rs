use log::info;

use crate::server::spawn_server_task;
use crate::server_advertisement::spawn_timer_task;

mod config_provider;
mod game;
mod server;
mod server_advertisement;
mod tasks;

#[cfg(test)]
mod server_test;

#[tokio::main]
async fn main() {
    env_logger::init();

    info!("Battleship Plus server startup");

    let cfg = config_provider::default_config_provider();

    let announcement_ctrl = spawn_timer_task(cfg.as_ref()).await;

    let server_ctrl = spawn_server_task(cfg.clone());
    server_ctrl.wait().await;

    if let Some(ctrl) = announcement_ctrl {
        ctrl.stop().await;
    }
}
