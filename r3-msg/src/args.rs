use clap_derive::Parser;
use r3lib::R3Command;

#[derive(Debug, Parser)]
pub struct Args {
    /// Path to the r3 socket
    /// If not provided, r3-msg will try to read it from the R3_SOCKET_PATH on
    /// the root window of the running X server
    #[clap(long = "socket", short = 's')]
    pub socket: Option<String>,

    /// The command to send
    #[clap(subcommand)]
    pub command: R3Command,
}
