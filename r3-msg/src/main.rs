mod args;

use crate::args::Args;
use clap::Parser;
use std::error::Error;
use std::io::Read;
use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

pub fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // TODO: right now r3 only supports a single command per read (reads until EOF)
    //  we need to consider the case of long-lived IPC connections, and potentially buffer per line?
    let mut conn = UnixStream::connect(args.socket)?;
    conn.write_all(&serde_json::to_vec(&args.command)?)?;
    conn.shutdown(Shutdown::Write)?;

    // Read response
    let mut buffer = String::new();
    conn.read_to_string(&mut buffer)?;
    println!("response: {}", buffer);

    Ok(())
}
