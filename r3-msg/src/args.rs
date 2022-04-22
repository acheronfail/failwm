use clap_derive::Parser;
use r3lib::R3Command;

#[derive(Debug, Parser)]
pub struct Args {
    /// Path to the r3 socket
    // TODO: default to getting the R3_SOCKET_PATH atom from the root window if it exists!
    #[clap(long = "socket", short = 's', default_value = "/tmp/r3.sock")]
    pub socket: String,

    /// The command to send
    #[clap(subcommand)]
    pub command: R3Command,
}
