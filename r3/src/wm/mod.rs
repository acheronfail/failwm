mod cmd_handlers;
mod ignored_sequences;
mod masks;
mod windows;
mod x_handlers;

use std::{
    os::unix::prelude::OsStrExt,
    path::Path,
    sync::{Arc, Mutex},
};

use self::{ignored_sequences::IgnoredSequences, masks::MASKS};
use crate::{config::Config, point::Point, window_geometry::WindowGeometry};
use bimap::BiHashMap;
use mio::Waker;
use r3lib::R3Command;
use xcb::{x, Connection};

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

        // Custom atoms
        r3_pid           => b"R3_PID",
        r3_socket_path   => b"R3_SOCKET_PATH",
        r3_sync          => b"R3_SYNC",
        r3_frame         => b"R3_FRAME",
    }
}

/// When clicking and dragging a floating window, what kind of drag operation are we performing?
#[derive(Debug, PartialEq, Eq)]
enum DragType {
    Move,
    Resize,
}

pub struct WindowManager<'a> {
    /// Our way of communicating back to the main loop
    ev_waker: Arc<Waker>,
    ev_queue: Arc<Mutex<Vec<R3Command>>>,

    /// WM Configuration
    config: Config,

    /// XCB connection
    conn: &'a Connection,
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
}

impl<'a> WindowManager<'a> {
    /// Connect to the X Server and create a `WindowManager`.
    /// It will not attempt to become the X Server's window manager until `.run()` is called.
    pub fn new(
        (conn, default_screen): (&'a Connection, i32),
        (ev_waker, ev_queue): (Arc<Waker>, Arc<Mutex<Vec<R3Command>>>),
    ) -> xcb::Result<WindowManager<'a>> {
        let atoms = Atoms::intern_all_with_exists(&conn, false)?;
        Ok(WindowManager {
            ev_waker,
            ev_queue,

            config: Config::new(),

            conn,
            atoms,
            default_screen,

            framed_clients: BiHashMap::new(),
            ignored_sequences: IgnoredSequences::new(),

            drag_start: None,
            drag_start_frame_rect: None,

            focused_window: None,
        })
    }

    /// Become the window manager and setup root event masks
    pub fn become_window_manager(&mut self, socket_path: &Path) -> xcb::Result<()> {
        // Request to become the X window manager
        self.acquire_wm_event_mask()?;

        // Start managing any existing windows
        self.reparent_existing_windows()?;

        // Bind key events on root window so they're always reported
        let root = self.get_root_window()?;
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

        // Set an atom on the root window with the path to our IPC socket
        let set_atom = |atom, data| {
            self.conn.send_and_check_request(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window: root,
                property: atom,
                r#type: x::ATOM_STRING,
                data,
            })
        };

        let pid = std::process::id().to_string();
        set_atom(self.atoms.r3_pid, pid.as_bytes())?;
        set_atom(self.atoms.r3_socket_path, socket_path.as_os_str().as_bytes())?;

        Ok(())
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
    fn acquire_wm_event_mask(&self) -> xcb::Result<()> {
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
                time: x::CURRENT_TIME,
            })?;
        }

        Ok(())
    }
}
