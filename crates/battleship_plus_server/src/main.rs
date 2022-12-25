use log::info;

use crate::server::server_task;
use crate::server_advertisement::start_announcement_timer;

mod config_provider;
mod game;
mod server_advertisement;
mod server;

#[tokio::main]
async fn main() {
    env_logger::init();

    info!("Battleship Plus server startup");

    let cfg = config_provider::default_config_provider();

    start_announcement_timer(cfg.as_ref()).await;

    server_task(cfg.clone()).await;
}
