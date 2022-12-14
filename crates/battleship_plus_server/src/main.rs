use std::borrow::Borrow;

use log::info;
use prost::Message;
use tokio::signal;

use crate::config_provider::ConfigProvider;
use crate::server_advertisement::start_announcement_timer;

mod config_provider;
mod server_advertisement;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Battleship Plus server startup");

    let cfg = config_provider::default_config_provider();

    start_announcement_timer(cfg.as_ref()).await;

    signal::ctrl_c().await.unwrap();
}
