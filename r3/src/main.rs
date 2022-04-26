mod config;
mod macros;
mod point;
mod tiler;
mod window_geometry;
mod wm;

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixListener;
use std::os::unix::prelude::AsRawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, fs, process, thread};

use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token, Waker};
use r3lib::R3Command;
use signal_hook::consts::SIGTERM;
use signal_hook_mio::v0_8::Signals;
use wm::WindowManager;
use xcb::Connection;

const T_XCB: Token = Token(0);
const T_IPC: Token = Token(1);
const T_CMD: Token = Token(2);
const T_SIG: Token = Token(3);

fn get_socket_path() -> Result<PathBuf, Box<dyn Error>> {
    let dir = match env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(e) => {
            eprintln!("Failed to interpret XDG_RUNTIME_DIR: {}", e);
            eprintln!("Falling back to /tmp");
            PathBuf::from("/tmp")
        }
    };

    // Nest in a directory
    let dir = dir.join("r3");

    // Create the directory
    fs::create_dir_all(&dir)?;

    // Return the socket path
    let socket_path = dir.join(format!("ipc-socket.{}", process::id()));
    eprintln!("socket_path: {}", socket_path.display());
    Ok(socket_path)
}

// TODO: consider abstracting away X-specific items, and allowing Wayland impls too?
//  unsure how difficult this will be (seems to be mostly X code for now)
fn main() -> Result<(), Box<dyn Error>> {
    // Stop and wait for debugger if R3_DEBUG present
    #[cfg(feature = "debug")]
    if matches!(std::env::var("R3_DEBUG"), Ok(_)) {
        nix::sys::signal::raise(nix::sys::signal::SIGSTOP).unwrap();
    }

    // Event Loop setup:
    //  Register XCB events by listening to its file descriptor
    //  Register IPC events by listening to its file descriptor
    //  Create a waker which is used by client-ipc threads to send commands back
    //  Register signal events to respond to them
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);
    let registry = poll.registry();
    let loop_waker = Arc::new(Waker::new(registry, T_CMD)?);
    let command_queue: Arc<Mutex<Vec<R3Command>>> = Arc::new(Mutex::new(vec![]));

    // IPC setup:
    //  TODO: doc
    let socket_path = get_socket_path()?;
    let ipc_socket = UnixListener::bind(&socket_path)?;
    ipc_socket.set_nonblocking(true)?;
    registry.register(&mut SourceFd(&ipc_socket.as_raw_fd()), T_IPC, Interest::READABLE)?;

    // XCB setup:
    //  If display name is none, rust-xcb will use the DISPLAY environment variable
    //  TODO: doc
    let (xcb_conn, xcb_default_screen) = Connection::connect_with_extensions(None, &[], &[])?;
    let mut wm = WindowManager::new(
        (&xcb_conn, xcb_default_screen),
        (loop_waker.clone(), command_queue.clone()),
    )?;
    wm.become_window_manager(&socket_path)?;
    registry.register(&mut SourceFd(&xcb_conn.as_raw_fd()), T_XCB, Interest::READABLE)?;

    // Signal setup:
    //  TODO doc
    let mut signals = Signals::new(&[SIGTERM])?;
    registry.register(&mut signals, T_SIG, Interest::READABLE)?;

    // The event loop!
    let loop_timeout = Some(Duration::from_millis(20));
    'event_loop: loop {
        // This is analogous to ev's `ev_prepare`: before we start blocking on our event loop, we want
        // to make sure that XCB's incoming and outgoing queues are completely empty, so there are
        // no race conditions between `poll`ing the file descriptor and data being ready there
        {
            // Loop until we consume all available XCB events
            loop {
                match xcb_conn.poll_for_event() {
                    // No events left to read
                    Ok(None) => break,
                    // We read an xcb event
                    Ok(Some(ev)) => wm.handle_event(Ok(ev))?,
                    // Some error occurred when polling/reading event
                    Err(e) => wm.handle_event(Err(e))?,
                }
            }
            xcb_conn.flush()?;
        }

        // Event loop block
        poll.poll(&mut events, loop_timeout)?;
        for event in &events {
            match event.token() {
                T_XCB => {
                    // We do nothing here, since we process all XCB events before blocking the event loop
                }
                T_IPC => {
                    // Loop until we've accepted all waiting IPC connections
                    loop {
                        match ipc_socket.accept() {
                            // We got an IPC connection, read it and send a message back
                            Ok((mut socket, addr)) => {
                                println!("Client connection: {:?} - {:?}", socket, addr);
                                let thread_waker = loop_waker.clone();
                                let thread_commands = command_queue.clone();
                                thread::Builder::new().name(format!("ipc-client")).spawn(move || {
                                    // Timeout connection after periods of inactivity
                                    socket.set_read_timeout(Some(Duration::from_secs(180))).unwrap();

                                    let mut message = String::new();

                                    // NOTE: the fastest way to deserialise right now is to read the entire body at once
                                    // into a string and then deserialise that. See: https://github.com/serde-rs/json/issues/160
                                    match socket.read_to_string(&mut message) {
                                        Ok(_) => {
                                            println!("Client message: {}", message);
                                            let command: R3Command = serde_json::from_str(&message).unwrap();
                                            println!("Client command: {:?}", command);
                                            thread_commands.lock().unwrap().push(command);
                                            thread_waker.wake().unwrap();

                                            // TODO: construct JSON reply
                                            socket.write_all(b"Hello from the server!").unwrap();
                                            socket.shutdown(Shutdown::Both).unwrap();
                                        }
                                        // The read took to long, so drop it
                                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                            eprintln!("IPC message took too long to read, dropping");
                                            return;
                                        }
                                        // Some other error occurred when reading
                                        Err(e) => panic!("ipc read error: {}", e),
                                    }

                                    println!("ipc client thread exit");
                                })?;
                            }
                            // We tried to accept, but there are no more connections (we'd start blocking)
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            // Some other error occurred when accepting/reading connections
                            Err(e) => panic!("ipc error: {}", e),
                        }
                    }
                }
                T_CMD => {
                    let cmds = command_queue.lock().unwrap().drain(..).collect::<Vec<_>>();
                    for cmd in cmds {
                        // TODO: extend R3Command so there are WM-specific commands and app-specific commands
                        match cmd {
                            R3Command::WM(wm_cmd) => wm.handle_command(&wm_cmd)?,
                            // TODO: how to send reply? wrap items in command_queue with a context/reply/etc field/closure?
                            R3Command::GetConfig => todo!(),
                            R3Command::GetVersion => todo!(),
                            R3Command::Exit => break 'event_loop,
                        }
                    }
                }
                T_SIG => {
                    for sig in signals.pending() {
                        match sig {
                            SIGTERM => {
                                eprintln!("Received: SIGTERM");
                                break 'event_loop;
                            }
                            _ => unimplemented!("{:?}", sig),
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    // Clean up before exit
    eprintln!("r3 exiting...");
    drop(ipc_socket);
    fs::remove_file(&socket_path)?;

    Ok(())
}
