use std::net::ToSocketAddrs;

use clap::Parser;
use log::debug;

use crate::cli::{Cli, Commands};
use crate::interactive::interactive_main;

mod cli;
mod interactive;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.commands {
        Commands::Interactive => {
            tui_logger::init_logger(log::LevelFilter::Debug).unwrap();
            tui_logger::set_default_level(log::LevelFilter::Debug);
            debug!("Entering interactive mode...");

            interactive_main(match (cli.server, cli.port) {
                (Some(server), Some(port)) => Some(
                    match format!("{server}:{port}").to_socket_addrs() {
                        Ok(mut addresses) => {
                            let ipv6_address = addresses.clone().find(|address| address.is_ipv6());
                            if let Some(address) = ipv6_address {
                                Ok(address)
                            } else if let Some(address) = addresses.next() {
                                Ok(address)
                            } else {
                                Err("unable to parse a SocketAddr".to_string())
                            }
                        }
                        Err(error) => Err(format!("Could not resolve host name: {error}")),
                    }
                    .unwrap(),
                ),
                (None, None) => None,
                (Some(server), None) => {
                    panic!("Specifying server ({server}) requires specifying port.")
                }
                (None, Some(port)) => {
                    panic!("Specifying port ({port}) requires specifying server.")
                }
            })
            .await
            .expect("interactive mode stopped with an error");
        }
        _ => {
            pretty_env_logger::init_timed();
            debug!("Command line arguments: {cli:#?}");
        }
    }
}
