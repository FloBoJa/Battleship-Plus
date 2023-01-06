use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "Battleship Plus Examiner")]
#[command(author = "Ludger H. <uxmlz@student.kit.edu>")]
#[command(version = "0.1")]
#[command(about = "Examines Battleship Plus server interactively or automatically.", long_about = None)]
#[command(propagate_version = true)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) commands: Commands,

    /// Specify the server hostname or IP. Requires port.
    #[arg(short, long)]
    pub(crate) server: Option<String>,

    /// Specify the server port. Requires server.
    #[arg(short, long)]
    pub(crate) port: Option<u16>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Starts interactive mode
    Interactive,
    /// Runs automatic tests against a given server
    Test,
}
