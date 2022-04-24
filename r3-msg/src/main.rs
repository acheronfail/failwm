mod args;

use std::error::Error;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

use clap::Parser;
use xcb::x::{GetProperty, ATOM_ANY};
use xcb::{x, Connection, ProtocolError};

use crate::args::Args;

xcb::atoms_struct! {
    #[derive(Debug)]
    struct Atoms {
        pub r3_socket_path => b"R3_SOCKET_PATH",
    }
}

pub fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // If no socket path was provided, connect to X and look for the atom on the root window
    let socket_path = args.socket.map_or_else(get_socket_path, |s| Ok(s))?;

    // TODO: right now r3 only supports a single command per read (reads until EOF)
    //  we need to consider the case of long-lived IPC connections, and potentially buffer per line?
    let mut conn = UnixStream::connect(socket_path)?;
    conn.write_all(&serde_json::to_vec(&args.command)?)?;
    conn.shutdown(Shutdown::Write)?;

    // Read response
    let mut buffer = String::new();
    conn.read_to_string(&mut buffer)?;
    println!("response: {}", buffer);

    Ok(())
}

/// Look for and read the R3_SOCKET_PATH atom on the root X window
fn get_socket_path() -> Result<String, Box<dyn Error>> {
    // Connect to X
    let (conn, default_screen) = match Connection::connect(None) {
        Ok(inner) => inner,
        Err(_) => {
            return Err(format!(
                "Failed to connect to X. Ensure the DISPLAY environment variable is set."
            ))?
        }
    };
    let root_window = conn.get_setup().roots().nth(default_screen as usize).unwrap().root();

    // Fetch the Atom from the X server
    let atoms = Atoms::intern_all(&conn)?;
    if atoms.r3_socket_path == x::ATOM_NONE {
        return Err("The X server isn't aware of R3_SOCKET_PATH. Is r3 running?")?;
    }

    // Read the atom on the root X window
    let cookie = conn.send_request(&GetProperty {
        delete: false,
        window: root_window,
        property: atoms.r3_socket_path,
        r#type: ATOM_ANY,
        long_offset: 0,
        long_length: u32::MAX,
    });

    let reply = match conn.wait_for_reply(cookie) {
        Err(xcb::Error::Protocol(ProtocolError::X(x::Error::Atom(_), _))) => {
            return Err("Failed to find R3_SOCKET_PATH atom on the root window. Is r3 running?")?;
        }
        Err(e) => panic!("{}", e),
        Ok(reply) => reply,
    };

    // Read the reply to get the socket path
    let value = match String::from_utf8(reply.value::<u8>().into()) {
        Ok(s) => s,
        Err(e) => Err(format!("Failed to decode string from X atom: {}", e))?,
    };

    if value.is_empty() {
        return Err("Found R3_SOCKET_PATH, but it was empty.")?;
    }

    Ok(value)
}
