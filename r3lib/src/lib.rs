use clap_derive::Subcommand;
use serde::{Deserialize, Serialize};

// TODO: should this live in a shared r3lib? so server and client can use it?
// TODO: consider how commands/config should be formatted (i3-like attributes)
#[derive(Debug, Serialize, Deserialize, Subcommand)]
pub enum R3Command {
    /// Exits r3
    Exit,
    /// Closes a window
    CloseWindow,
}
