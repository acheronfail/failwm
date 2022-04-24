use core::panic;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{env, thread};

use r3lib::R3Command;
use rand::Rng;
use xcb::Xid;

pub struct XTestRunner {
    display_num: AtomicUsize,
}

impl XTestRunner {
    pub fn new() -> XTestRunner {
        XTestRunner {
            display_num: AtomicUsize::new(100),
        }
    }

    pub fn test(&self) -> XTestCase {
        let n = self.display_num.fetch_add(1, Ordering::SeqCst);
        XTestCase::start(n)
    }
}

xcb::atoms_struct! {
    #[derive(Debug)]
    pub struct Atoms {
        pub wm_protocols     => b"WM_PROTOCOLS",
        pub wm_del_window    => b"WM_DELETE_WINDOW",

        pub r3_sync          => b"R3_SYNC",
        pub r3_socket_path   => b"R3_SOCKET_PATH",
        pub r3_frame         => b"R3_FRAME",
    }
}

// TODO: create a way to interact with wm
pub struct XTestCase {
    /// Start time of this test
    pub start: Instant,
    /// Connection to the Xephyr X instance
    pub conn: Arc<xcb::Connection>,
    /// Some X Atoms we need
    pub atoms: Arc<Atoms>,
    /// Handle to the root window
    root: xcb::x::Window,
    /// Handle to a special window we use for syncing with r3
    sync_window: Option<xcb::x::Window>,
    /// The handle to the child process for r3
    r3_child: Child,
    /// The handle to the child process for the X server
    x_child: Child,
}

impl XTestCase {
    fn start(display_num: usize) -> XTestCase {
        // Spawn X server
        let display = format!(":{}", display_num);
        let (program, extra_args) = match env::var("TEST_ENABLE_XEPHYR") {
            Ok(_) => (
                which::which("Xephyr").unwrap(),
                vec!["-screen", "800x600", "-no-host-grab"],
            ),
            Err(_) => (which::which("Xvfb").unwrap(), vec!["-screen", "0", "800x600x24"]),
        };
        let x_stdio = match env::var("TEST_ENABLE_X_STDIO") {
            Ok(_) => Stdio::inherit,
            Err(_) => Stdio::null,
        };
        let mut x_child = Command::new(program)
            .arg(&display)
            .arg("-ac")
            .args(extra_args)
            .stdout(x_stdio())
            .stderr(x_stdio())
            .spawn()
            .unwrap();

        // Connect to the X server
        let (conn, n) = {
            let mut attempt = 0;
            let max_attempts = 5;
            loop {
                match xcb::Connection::connect_with_extensions(Some(&display), &[], &[]) {
                    Ok(inner) => break inner,
                    Err(_) => {
                        if attempt == max_attempts {
                            x_child.kill().unwrap();
                            panic!("Failed to connect to X server, attempts: {}", attempt);
                        }

                        thread::sleep(Duration::from_millis(10));
                        attempt += 1;
                        continue;
                    }
                }
            }
        };

        // Spawn r3
        let r3_stdio = match env::var("TEST_ENABLE_R3_STDIO") {
            Ok(_) => Stdio::inherit,
            Err(_) => Stdio::null,
        };
        let r3_bin_path = env::current_dir().unwrap().join("../target/debug/r3");
        let r3_child = Command::new(r3_bin_path)
            .env("DISPLAY", display)
            .stdout(r3_stdio())
            .stderr(r3_stdio())
            .spawn()
            .unwrap();

        // Intern the X atoms we need
        let atoms = {
            // Wait for r3 to intern atoms in the X server
            let mut atoms;
            while {
                atoms = Atoms::intern_all(&conn).unwrap();
                atoms.r3_socket_path == xcb::x::ATOM_NONE
            } {
                thread::sleep(Duration::from_millis(10));
            }

            atoms
        };

        let mut t = XTestCase {
            root: conn.get_setup().roots().nth(n as usize).unwrap().root(),
            sync_window: None,
            conn: Arc::new(conn),
            atoms: Arc::new(atoms),
            r3_child,
            x_child,
            start: Instant::now(),
        };
        XTestCase::create_sync_window(&mut t);
        t
    }

    // TODO: more ergonomic configuration here - use a vec of enums for Cw attrs?
    pub fn open_window(&self, rect: (i16, i16, u16, u16)) -> XWindow {
        self._open_window(rect, false)
    }

    fn _open_window(&self, (x, y, w, h): (i16, i16, u16, u16), override_redirect: bool) -> XWindow {
        let wid = self.conn.generate_id();
        self.conn
            .send_and_check_request(&xcb::x::CreateWindow {
                depth: xcb::x::COPY_FROM_PARENT as u8,
                visual: xcb::x::COPY_FROM_PARENT as u32,
                wid,
                parent: self.root,
                x,
                y,
                width: w,
                height: h,
                border_width: 0,
                class: xcb::x::WindowClass::CopyFromParent,
                value_list: &[
                    xcb::x::Cw::BackPixel(0xc0c0c0),
                    xcb::x::Cw::OverrideRedirect(override_redirect),
                    xcb::x::Cw::EventMask(xcb::x::EventMask::STRUCTURE_NOTIFY),
                ],
            })
            .unwrap();

        XWindow {
            id: wid,
            conn: self.conn.clone(),
            atoms: self.atoms.clone(),
        }
    }

    fn create_sync_window(&mut self) {
        // Create a hidden un-managed window which will be used for syncing
        if let None = self.sync_window {
            // Create window outside of viewport, and set override redirect (so WM doesn't manage it)
            let w = self._open_window((-15, -15, 10, 10), true);
            w.map();
            self.sync_window = Some(w.id);
        }
    }

    pub fn sync(&self) {
        // This is created in the `::new()` call
        let window_id = self.sync_window.unwrap();

        // Send a randomly generated number to r3
        let mut rng = rand::thread_rng();
        let magic: u32 = rng.gen();
        let data = xcb::x::ClientMessageData::Data32([
            window_id.resource_id(),
            magic, // random data
            0,
            0,
            0, // padding
        ]);

        eprintln!("[sync] send: {}", magic);
        self.conn.send_request(&xcb::x::SendEvent {
            propagate: false,
            destination: xcb::x::SendEventDest::Window(self.root),
            event_mask: xcb::x::EventMask::SUBSTRUCTURE_REDIRECT,
            event: &xcb::x::ClientMessageEvent::new(window_id, self.atoms.r3_sync, data),
        });
        self.conn.flush().unwrap();

        // Wait for r3 to process the sync message, and send the reply back to our sync window
        eprintln!("[sync] wait: {}", magic);
        loop {
            let event = self.conn.wait_for_event().unwrap();
            match event {
                xcb::Event::X(xcb::x::Event::ClientMessage(ev)) => match ev.data() {
                    xcb::x::ClientMessageData::Data32([_wid, n, _, _, _]) if n == magic => break,
                    _ => {}
                },
                _ => {}
            }
        }
        eprintln!("[sync] recv: {}", magic);
    }

    pub fn get_all_windows(&self) -> Vec<XWindow> {
        let query_tree = self
            .conn
            .wait_for_reply(self.conn.send_request(&xcb::x::QueryTree { window: self.root }))
            .unwrap();

        query_tree
            .children()
            .into_iter()
            .filter(|id| **id != self.sync_window.unwrap())
            .map(|id| XWindow {
                id: *id,
                conn: self.conn.clone(),
                atoms: self.atoms.clone(),
            })
            .collect()
    }

    pub fn get_socket_path(&self) -> String {
        let reply = self
            .conn
            .wait_for_reply(self.conn.send_request(&xcb::x::GetProperty {
                delete: false,
                window: self.root,
                property: self.atoms.r3_socket_path,
                r#type: xcb::x::ATOM_STRING,
                long_offset: 0,
                long_length: 1024,
            }))
            .unwrap();

        String::from_utf8(reply.value::<u8>().into()).unwrap()
    }

    pub fn command(&self, command: R3Command) {
        eprintln!("[command] send: {:?}", command);
        let mut c = UnixStream::connect(self.get_socket_path()).unwrap();
        c.write_all(&serde_json::to_vec(&command).unwrap()).unwrap();
        c.shutdown(Shutdown::Write).unwrap();

        // Read response
        let mut buffer = String::new();
        c.read_to_string(&mut buffer).unwrap();
        eprintln!("[command] recv: {:?}", buffer);
        // TODO: return response once a format for it exists
    }
}

pub struct XWindow {
    pub id: xcb::x::Window,
    conn: Arc<xcb::Connection>,
    atoms: Arc<Atoms>,
}

impl Debug for XWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XWindow")
            .field("id", &self.id)
            .field("conn", &self.conn.get_raw_conn())
            .finish()
    }
}

// TODO: easier helpers to create from XWindow or XTestCase
impl XWindow {
    pub fn map(&self) {
        self.conn
            .send_and_check_request(&xcb::x::MapWindow { window: self.id })
            .unwrap();
    }

    pub fn close(self) {
        self.conn
            .send_and_check_request(&xcb::x::DestroyWindow { window: self.id })
            .unwrap();
    }

    pub fn is_frame(&self) -> bool {
        let reply = self
            .conn
            .wait_for_reply(self.conn.send_request(&xcb::x::GetProperty {
                delete: false,
                window: self.id,
                property: self.atoms.r3_frame,
                r#type: xcb::x::ATOM_STRING,
                long_offset: 0,
                long_length: u32::MAX,
            }))
            .unwrap();

        String::from_utf8(reply.value::<u8>().into()).unwrap() == "1"
    }

    pub fn get_frame(&self) -> XWindow {
        let query_tree = self
            .conn
            .wait_for_reply(self.conn.send_request(&xcb::x::QueryTree { window: self.id }))
            .unwrap();

        XWindow {
            id: query_tree.parent(),
            conn: self.conn.clone(),
            atoms: self.atoms.clone(),
        }
    }

    pub fn rect(&self) -> (i16, i16, u16, u16) {
        let geo = self
            .conn
            .wait_for_reply(self.conn.send_request(&xcb::x::GetGeometry {
                drawable: xcb::x::Drawable::Window(self.id),
            }))
            .unwrap();

        (geo.x(), geo.y(), geo.width(), geo.height())
    }

    pub fn border_width(&self) -> u16 {
        let geo = self
            .conn
            .wait_for_reply(self.conn.send_request(&xcb::x::GetGeometry {
                drawable: xcb::x::Drawable::Window(self.id),
            }))
            .unwrap();

        geo.border_width()
    }
}

impl Drop for XTestCase {
    fn drop(&mut self) {
        self.r3_child.kill().unwrap();
        self.x_child.kill().unwrap();
    }
}
