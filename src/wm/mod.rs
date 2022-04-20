mod handlers;
mod ignored_sequences;
mod masks;
mod windows;

use bimap::BiHashMap;
use xcb::{x, Connection};

use self::{ignored_sequences::IgnoredSequences, masks::MASKS};
use crate::{config::Config, point::Point, window_geometry::WindowGeometry};

crate::atoms_struct! {
    #[derive(Debug)]
    struct Atoms {
        wm_protocols     => b"WM_PROTOCOLS",
        wm_del_window    => b"WM_DELETE_WINDOW",
        #[allow(dead_code)]
        wm_active_window => b"_NET_ACTIVE_WINDOW",
        #[allow(dead_code)]
        wm_state         => b"_NET_WM_STATE",
        #[allow(dead_code)]
        wm_state_maxv    => b"_NET_WM_STATE_MAXIMIZED_VERT",
        #[allow(dead_code)]
        wm_state_maxh    => b"_NET_WM_STATE_MAXIMIZED_HORZ",
    }
}

/// When clicking and dragging a floating window, what kind of drag operation are we performing?
#[derive(Debug, PartialEq, Eq)]
enum DragType {
    Move,
    Resize,
}

/// The reason why the WindowManager is quitting.
#[derive(Debug, Clone, Copy)]
pub enum QuitReason {
    UserQuit,
}

pub struct WindowManager {
    /// WM Configuration
    config: Config,

    /// XCB connection
    conn: Connection,
    /// The atoms we need
    atoms: Atoms,
    /// X's default screen
    default_screen: i32,

    /// A mapping of Window -> Frame to help keep track of framed windows
    framed_clients: BiHashMap<x::Window, x::Window>,
    /// List of event sequences to ignore. Sometimes, X will trigger EnterNotify events for
    /// mapped (and unmapped!) windows; these events are indistinguishable from user-generated
    /// events, and don't provide any value for us. In these cases, we maintain a list of event
    /// sequences to ignore so we can skip them. This data structure will clean itself up and
    /// won't infinitely grow in size.
    ignored_sequences: IgnoredSequences,

    /// If a drag is in progress, this will contain the coordinates of its starting position
    drag_start: Option<Point>,
    /// If a drag is in progress, this will contain the starting rect of the frame dragged
    drag_start_frame_rect: Option<WindowGeometry>,

    /// The currently focused window
    focused_window: Option<x::Window>,

    /// If this is set, on the next turn of the event loop it will exit the loop
    quit_reason: Option<QuitReason>,
}

impl WindowManager {
    /// Connect to the X Server and create a `WindowManager`.
    /// It will not attempt to become the X Server's window manager until `.run()` is called.
    pub fn new(config: Config) -> xcb::Result<WindowManager> {
        let (conn, default_screen) = xcb::Connection::connect_with_extensions(None, &[xcb::Extension::Xkb], &[])?;
        let atoms = Atoms::intern_all_with_exists(&conn, false)?;
        Ok(WindowManager {
            config,

            conn,
            atoms,
            default_screen,

            framed_clients: BiHashMap::new(),
            ignored_sequences: IgnoredSequences::new(),

            drag_start: None,
            drag_start_frame_rect: None,

            focused_window: None,

            quit_reason: None,
        })
    }

    /// To be called just after becoming the X Server's window manager.
    /// This will iterate all existing X windows and frame them as needed.
    fn reparent_existing_windows(&mut self) -> xcb::Result<()> {
        // Make sure nothing happens in the X server while we're processing existing windows
        self.conn.send_and_check_request(&x::GrabServer {})?;

        // Frame all pre-existing windows that are visible
        let root = self.get_root_window()?;
        let query_tree = self
            .conn
            .wait_for_reply(self.conn.send_request(&x::QueryTree { window: root }))?;

        for window in query_tree.children() {
            self.frame_window(*window, true)?;
        }

        // Allow things to happen again
        self.conn.send_and_check_request(&x::UngrabServer {})?;

        Ok(())
    }

    /// Try to become the X Server's window manager.
    /// TODO: link to documentation, or explain it here
    fn become_window_manager(&self) -> xcb::Result<()> {
        let c = self.conn.send_request_checked(&x::ChangeWindowAttributes {
            window: self.get_root_window()?,
            value_list: &[x::Cw::EventMask(
                x::EventMask::SUBSTRUCTURE_REDIRECT | x::EventMask::SUBSTRUCTURE_NOTIFY,
            )],
        });

        match self.conn.check_request(c) {
            Ok(_) => {}
            Err(xcb::ProtocolError::X(x::Error::Access(req), _)) if req.error_code() == 10 => {
                panic!("Is there an existing WM already?");
            }
            _ => unimplemented!(),
        }

        Ok(())
    }

    /// Get the X Server's root window from the default screen.
    fn get_root_window(&self) -> xcb::Result<x::Window> {
        let setup = self.conn.get_setup();
        let screen = setup.roots().nth(self.default_screen as usize).unwrap();
        let root = screen.root();
        Ok(root)
    }

    // TODO: doc
    // TODO: in the future, will probably have to maintain some internal state and only render diff
    //  rather than touching every window every single time an event is received
    fn render(&self) -> xcb::Result<()> {
        let mut requests = vec![];

        // Iterate through all frames and color the focused one if it exists (the focused window may
        // be a window that's not framed)
        for (window, frame) in &self.framed_clients {
            let is_focused = Some(*window) == self.focused_window || Some(*frame) == self.focused_window;
            requests.push(self.conn.send_request_checked(&x::ChangeWindowAttributes {
                window: *frame,
                value_list: &[x::Cw::BorderPixel(if is_focused { 0xff0000 } else { 0xaaaaaa })],
            }));
            if is_focused {
                requests.push(self.conn.send_request_checked(&x::ConfigureWindow {
                    window: *frame,
                    value_list: &[x::ConfigWindow::StackMode(x::StackMode::Above)],
                }))
            }
        }

        for cookie in requests {
            self.conn.check_request(cookie)?;
        }

        // If we have a focused window, then tell X to focus it specifically.
        if let Some(target) = self.focused_window {
            // If the focused window is a frame, then focus its window
            let focus = *self.framed_clients.get_by_right(&target).unwrap_or(&target);
            self.conn.send_and_check_request(&x::SetInputFocus {
                revert_to: x::InputFocus::PointerRoot,
                focus,
                // /usr/include/xcb/xcb.h: #define XCB_CURRENT_TIME 0L
                time: 0,
            })?;
        }

        Ok(())
    }

    /// Become the window manager and start managing windows!
    pub fn run(&mut self) -> xcb::Result<QuitReason> {
        //
        // WM setup
        //

        self.become_window_manager()?;
        self.reparent_existing_windows()?;

        //
        // Root window events
        //

        let root = self.get_root_window()?;

        // Bind key events on root window so they're always reported
        self.conn.send_and_check_request(&x::GrabKey {
            grab_window: root,
            owner_events: false,
            key: 0x18, // Q on qwerty TODO: support keymaps
            pointer_mode: x::GrabMode::Async,
            keyboard_mode: x::GrabMode::Async,
            modifiers: x::ModMask::ANY,
        })?;

        // Start listening to events on the root window
        self.conn.send_and_check_request(&x::ChangeWindowAttributes {
            window: root,
            value_list: &[x::Cw::EventMask(MASKS.root_window_events)],
        })?;

        //
        // Event loop
        //

        loop {
            if let Some(quit_reason) = self.quit_reason {
                return Ok(quit_reason);
            }

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
                // We received a request to configure a window
                xcb::Event::X(x::Event::ConfigureRequest(ev)) => self.on_configure_request(ev)?,
                // We received a request to map (render) a window
                xcb::Event::X(x::Event::MapRequest(ev)) => self.on_map_request(ev)?,
                // When a window is unmapped, then we "un-frame" it if we've framed it
                xcb::Event::X(x::Event::UnmapNotify(ev)) => self.on_unmap_notify(ev)?,

                // Handle key events
                xcb::Event::X(x::Event::KeyPress(ev)) => self.on_key_press(ev)?,
                xcb::Event::X(x::Event::KeyRelease(ev)) => self.on_key_release(ev)?,

                // Handle mouse events
                xcb::Event::X(x::Event::ButtonPress(ev)) => self.on_button_press(ev)?,
                xcb::Event::X(x::Event::ButtonRelease(ev)) => self.on_button_release(ev)?,
                xcb::Event::X(x::Event::MotionNotify(ev)) => self.on_motion_notify(ev)?,

                // Handle window events
                xcb::Event::X(x::Event::EnterNotify(ev)) => self.on_enter_notify(ev)?,
                xcb::Event::X(x::Event::LeaveNotify(ev)) => self.on_leave_notify(ev)?,
                xcb::Event::X(x::Event::Expose(ev)) => self.on_expose(ev)?,
                xcb::Event::X(x::Event::FocusIn(ev)) => self.on_focus_in(ev)?,
                xcb::Event::X(x::Event::FocusOut(ev)) => self.on_focus_out(ev)?,
                xcb::Event::X(x::Event::PropertyNotify(ev)) => self.on_property_notify(ev)?,

                // Ignored events
                xcb::Event::X(x::Event::ReparentNotify(_)) => {}
                xcb::Event::X(x::Event::CreateNotify(_)) => {}
                xcb::Event::X(x::Event::DestroyNotify(_)) => {}
                xcb::Event::X(x::Event::ConfigureNotify(_)) => {}
                xcb::Event::X(x::Event::MappingNotify(_)) => {}
                xcb::Event::X(x::Event::MapNotify(_)) => {}

                // TODO: handle all events!
                _ => {
                    eprintln!("{:#?}", event);
                }
            }

            self.render()?;
        }
    }
}
