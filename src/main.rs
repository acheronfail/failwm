use std::collections::HashMap;

use xcb::{
    x::{self, ConfigureRequestEvent, MapState, Window},
    Connection,
};

struct App {
    conn: Connection,
    screen_num: i32,
    framed_clients: HashMap<Window, Window>,
}

impl App {
    pub fn new(conn: Connection, screen_num: i32) -> xcb::Result<App> {
        let mut app = App {
            conn,
            screen_num,
            framed_clients: HashMap::new(),
        };

        app.become_window_manager()?;
        app.reparent_existing_windows()?;
        Ok(app)
    }

    fn reparent_existing_windows(&mut self) -> xcb::Result<()> {
        // Make sure nothing happens in the X server while we're processing existing windows
        self.conn.send_and_check_request(&x::GrabServer {})?;

        // Frame all pre-existing windows that are visible
        let query_tree = self.conn.wait_for_reply(self.conn.send_request(&x::QueryTree {
            window: self.get_root()?,
        }))?;
        assert_eq!(self.get_root()?, query_tree.root());
        for window_id in query_tree.children() {
            self.frame_window(*window_id, true)?;
        }

        // Allow things to happen again
        self.conn.send_and_check_request(&x::UngrabServer {})?;

        Ok(())
    }

    fn become_window_manager(&self) -> xcb::Result<()> {
        let c = self.conn.send_request_checked(&x::ChangeWindowAttributes {
            window: self.get_root()?,
            value_list: &[x::Cw::EventMask(
                x::EventMask::SUBSTRUCTURE_REDIRECT | x::EventMask::SUBSTRUCTURE_NOTIFY,
            )],
        });

        match self.conn.check_request(c) {
            Ok(_) => {}
            // TODO: fails in other cases? (running outside of xephyr with no X server)
            Err(xcb::ProtocolError::X(x::Error::Access(req), _)) if req.error_code() == 10 => {
                panic!("Is there an existing WM already?");
            }
            _ => unimplemented!(),
        }
        Ok(())
    }

    fn get_root(&self) -> xcb::Result<Window> {
        let setup = self.conn.get_setup();
        let screen = setup.roots().nth(self.screen_num as usize).unwrap();
        let root = screen.root();
        Ok(root)
    }

    fn on_configure_request(&self, ev: ConfigureRequestEvent) -> xcb::Result<()> {
        let window_id = ev.window();
        let value_list = [
            x::ConfigWindow::X(ev.x() as i32),
            x::ConfigWindow::Y(ev.y() as i32),
            x::ConfigWindow::Width(ev.width() as u32),
            x::ConfigWindow::Height(ev.height() as u32),
            x::ConfigWindow::BorderWidth(ev.border_width() as u32),
            // FIXME: this crashes it when ev.sibling() returns 0
            // x::ConfigWindow::Sibling(ev.sibling()),
            x::ConfigWindow::StackMode(ev.stack_mode()),
        ];

        // If we've already framed this window, also update the frame
        // TODO: do we need to ignore border width here?
        if let Some(frame_id) = self.framed_clients.get(&window_id) {
            self.conn.send_and_check_request(&x::ConfigureWindow {
                window: *frame_id,
                value_list: &value_list,
            })?;
        }

        // Pass request straight through to the X server for window
        self.conn.send_and_check_request(&x::ConfigureWindow {
            window: window_id,
            value_list: &value_list,
        })?;

        Ok(())
    }

    fn frame_window(&mut self, window_id: Window, existed_before_wm: bool) -> xcb::Result<()> {
        // Get window attributes
        let geo = self.conn.wait_for_reply(self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(window_id),
        }))?;

        // If window was created before window manager started, we should frame
        // it only if it is visible and doesn't set override_redirect.
        if existed_before_wm {
            let attrs = self
                .conn
                .wait_for_reply(self.conn.send_request(&x::GetWindowAttributes { window: window_id }))?;
            if attrs.override_redirect() || attrs.map_state() != MapState::Viewable {
                return Ok(());
            }
        }

        // Create frame
        let frame_id = self.conn.generate_id();
        self.conn.send_and_check_request(&x::CreateWindow {
            depth: x::COPY_FROM_PARENT as u8,   // TODO: ???
            visual: x::COPY_FROM_PARENT as u32, // TODO: ???
            wid: frame_id,
            parent: self.get_root()?,
            x: geo.x(),
            y: geo.y(),
            width: geo.width(),
            height: geo.height(),
            border_width: 3,
            class: x::WindowClass::CopyFromParent,
            value_list: &[
                // Border pixel colour
                x::Cw::BorderPixel(0xff0000),
                x::Cw::EventMask(x::EventMask::SUBSTRUCTURE_REDIRECT | x::EventMask::SUBSTRUCTURE_NOTIFY),
            ],
        })?;

        // Add window to save set
        // TODO: doc why
        self.conn.send_and_check_request(&x::ChangeSaveSet {
            window: window_id,
            mode: x::SetMode::Insert,
        })?;

        // Re-parent window into frame
        self.conn.send_and_check_request(&x::ReparentWindow {
            window: window_id,
            parent: frame_id,
            // Offset of client window within frame
            x: 0,
            y: 0,
        })?;

        // Map frame
        self.conn.send_and_check_request(&x::MapWindow { window: frame_id })?;

        // Save association b/w window and frame
        self.framed_clients.insert(window_id, frame_id);

        Ok(())
    }

    fn unframe_window(&mut self, window_id: Window) -> xcb::Result<()> {
        let frame_id = match self.framed_clients.get(&window_id) {
            Some(id) => id,
            None => return Ok(()),
        };

        // Unmap frame
        self.conn
            .send_and_check_request(&x::UnmapWindow { window: *frame_id })?;

        // Re-parent client window back to root
        // FIXME: when checked this and others below error with BadWindow(3)
        self.conn.send_request_checked(&x::ReparentWindow {
            window: window_id,
            parent: self.get_root()?,
            // Offset of client within root
            x: 0,
            y: 0,
        });

        // Remove client window from save set, since we're not managing it anymore
        self.conn.send_request_checked(&x::ChangeSaveSet {
            window: window_id,
            mode: x::SetMode::Delete,
        });

        // Destroy the frame
        self.conn.send_request_checked(&x::DestroyWindow { window: *frame_id });

        // Drop window->frame association
        self.framed_clients.remove(&window_id);

        Ok(())
    }

    pub fn run(&mut self) -> xcb::Result<()> {
        loop {
            let event = match self.conn.wait_for_event() {
                Ok(event) => event,
                Err(xcb::Error::Connection(err)) => {
                    panic!("unexpected I/O error: {}", err);
                }
                Err(xcb::Error::Protocol(err)) => {
                    panic!("unexpected protocol error: {:#?}", err);
                }
            };

            match event {
                // We received a request to map (render) a window
                xcb::Event::X(x::Event::MapRequest(ev)) => {
                    let window_id = ev.window();
                    // First, we re-parent it with a frame
                    self.frame_window(window_id, false)?;
                    // Then, we actually map the window
                    self.conn.send_request_checked(&x::MapWindow { window: window_id });
                    self.conn.flush()?;
                }
                // We received a request to configure a window
                xcb::Event::X(x::Event::ConfigureRequest(ev)) => {
                    self.on_configure_request(ev)?;
                    self.conn.flush()?;
                }

                // When a window is unmapped, then we "un-frame" it if we've framed it
                xcb::Event::X(x::Event::UnmapNotify(ev)) => {
                    if ev.event() == self.get_root()? {
                        continue;
                    }
                    self.unframe_window(ev.window())?;
                    self.conn.flush()?;
                }

                xcb::Event::X(x::Event::KeyPress(ev)) => {
                    println!("Key '{}' pressed", ev.detail());
                    if ev.detail() == 0x18 {
                        // Q (on qwerty)
                        break;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

// TODO: consider abstracting away X-specific items, and allowing Wayland impls too?
//  unsure how difficult this will be (seems to be mostly X code for now)
fn main() -> xcb::Result<()> {
    // Stop and wait for debugger if R3_DEBUG present
    if matches!(std::env::var("R3_DEBUG"), Ok(_)) {
        nix::sys::signal::raise(nix::sys::signal::SIGSTOP).unwrap();
    }

    let (conn, screen_num) = xcb::Connection::connect_with_extensions(None, &[xcb::Extension::Xkb], &[])?;

    // Event loop
    let mut app = App::new(conn, screen_num)?;
    app.run()
}
