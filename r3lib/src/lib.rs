use clap_derive::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Subcommand)]
pub enum WMCommand {
    /// Close the currently focused window
    CloseWindow,
    // TODO: get window state, etc
}

#[derive(Debug, Serialize, Deserialize, Subcommand)]
pub enum R3Command {
    /// Commands specific to Window Management
    #[clap(subcommand)]
    WM(WMCommand),
    /// Returns the currently running version
    GetVersion,
    /// Returns the current configuration
    GetConfig,
    /// Exit the app
    Exit,
}
