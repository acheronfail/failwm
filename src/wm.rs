use std::{cmp, collections::HashMap};

use xcb::{
    x::{
        self, ButtonPressEvent, ConfigureRequestEvent, KeyPressEvent, MapRequestEvent, MapState, ModMask,
        MotionNotifyEvent, StackMode, UnmapNotifyEvent, Window,
    },
    Connection, Xid,
};

xcb::atoms_struct! {
    #[derive(Debug)]
    struct Atoms {
        wm_protocols    => b"WM_PROTOCOLS",
        wm_del_window   => b"WM_DELETE_WINDOW",
        wm_state        => b"_NET_WM_STATE",
        wm_state_maxv   => b"_NET_WM_STATE_MAXIMIZED_VERT",
        wm_state_maxh   => b"_NET_WM_STATE_MAXIMIZED_HORZ",
    }
}

use crate::{point::Point, rect::Rect, ret_if_none};
// use crate::macros::try_unwrap;

#[derive(Debug, PartialEq, Eq)]
enum DragType {
    Move,
    Resize,
}

pub struct WindowManager {
    conn: Connection,
    screen_num: i32,
    atoms: Atoms,
    framed_clients: HashMap<Window, Window>,

    drag_start: Option<Point>,
    drag_start_frame_rect: Option<Rect>,
}

impl WindowManager {
    pub fn new(conn: Connection, screen_num: i32) -> xcb::Result<WindowManager> {
        let atoms = Atoms::intern_all(&conn)?;
        Ok(WindowManager {
            conn,
            screen_num,
            atoms,
            framed_clients: HashMap::new(),

            drag_start: None,
            drag_start_frame_rect: None,
        })
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

    fn on_map_request(&mut self, ev: MapRequestEvent) -> xcb::Result<()> {
        let window_id = ev.window();
        // First, we re-parent it with a frame
        self.frame_window(window_id, false)?;
        // Then, we actually map the window
        self.conn.send_and_check_request(&x::MapWindow { window: window_id })?;

        Ok(())
    }

    fn on_unmap_notify(&mut self, ev: UnmapNotifyEvent) -> xcb::Result<()> {
        // Any windows existing before we started that are framed in `App::reparent_existing_windows`
        // trigger an UnmapNotify event when they're re-parented. We just ignore these events here.
        if ev.event() == self.get_root()? {
            return Ok(());
        }

        self.unframe_window(ev.window())?;
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

        // Button (mouse) handling
        self.conn.send_and_check_request(&x::GrabButton {
            grab_window: window_id,
            owner_events: false,
            event_mask: x::EventMask::BUTTON_PRESS | x::EventMask::BUTTON_RELEASE | x::EventMask::BUTTON_MOTION,
            pointer_mode: x::GrabMode::Async,
            keyboard_mode: x::GrabMode::Async,
            confine_to: window_id,
            cursor: xcb::Xid::none(),
            button: x::ButtonIndex::Any,
            modifiers: ModMask::CONTROL,
        })?;

        self.conn.send_and_check_request(&x::GrabKey {
            grab_window: window_id,
            owner_events: false,
            key: 0x18, // Q on qwerty TODO: support keymaps
            pointer_mode: x::GrabMode::Async,
            keyboard_mode: x::GrabMode::Async,
            modifiers: ModMask::ANY,
        })?;

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

        self.conn.flush()?;

        Ok(())
    }

    fn on_button_press(&mut self, ev: ButtonPressEvent) -> xcb::Result<()> {
        let frame_id = *self.framed_clients.get(&ev.event()).unwrap();
        let geo = self.conn.wait_for_reply(self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(frame_id),
        }))?;

        self.drag_start = Some((ev.root_x(), ev.root_y()).into());
        self.drag_start_frame_rect = Some((geo.x(), geo.y(), geo.width(), geo.height()).into());

        self.conn.send_and_check_request(&x::ConfigureWindow {
            window: ev.event(),
            value_list: &[x::ConfigWindow::StackMode(StackMode::Above)],
        })?;

        Ok(())
    }

    fn on_motion_notify(&mut self, ev: MotionNotifyEvent) -> xcb::Result<()> {
        let window_id = ev.event();
        let frame_id = *self.framed_clients.get(&window_id).unwrap();

        let drag_start = ret_if_none!(self.drag_start);
        let drag_start_frame_rect = ret_if_none!(self.drag_start_frame_rect);

        let delta = Point::new(ev.root_x(), ev.root_y()) - drag_start;
        let drag_type = ret_if_none!(if ev.state().contains(x::KeyButMask::BUTTON1) {
            Some(DragType::Move)
        } else if ev.state().contains(x::KeyButMask::BUTTON3) {
            Some(DragType::Resize)
        } else {
            None
        });

        // FIXME: these events don't fire if the mouse moves out of the window itself
        //  in order to draw like i3, we may need to register events on the root window
        // TODO: also like i3, if in top-left cut, resize and move rather than just resize
        match drag_type {
            DragType::Move => self.conn.send_and_check_request(&x::ConfigureWindow {
                window: frame_id,
                value_list: &[
                    x::ConfigWindow::X((drag_start_frame_rect.x + delta.x) as i32),
                    x::ConfigWindow::Y((drag_start_frame_rect.y + delta.y) as i32),
                ],
            })?,
            DragType::Resize => {
                let (x, y, w, h) = match ret_if_none!(drag_start_frame_rect.corner(&drag_start)) {
                    // TODO: change anchor point while resizing depending on corner
                    _ => (
                        None,
                        None,
                        cmp::max(0, drag_start_frame_rect.width as i32 + delta.x as i32) as u32,
                        cmp::max(0, drag_start_frame_rect.height as i32 + delta.y as i32) as u32,
                    ),
                };

                // NOTE: items in value_list must be sorted
                let mut value_list = vec![];
                if let Some(x) = x {
                    value_list.push(x::ConfigWindow::X(x));
                }
                if let Some(y) = y {
                    value_list.push(x::ConfigWindow::Y(y));
                }
                value_list.push(x::ConfigWindow::Width(w));
                value_list.push(x::ConfigWindow::Height(h));

                let value_list = &value_list;
                self.conn.send_and_check_request(&x::ConfigureWindow {
                    window: frame_id,
                    value_list,
                })?;
                self.conn.send_and_check_request(&x::ConfigureWindow {
                    window: window_id,
                    value_list,
                })?;
            }
        }

        Ok(())
    }

    fn on_button_release(&mut self, _ev: ButtonPressEvent) -> xcb::Result<()> {
        self.drag_start = None;
        self.drag_start_frame_rect = None;
        Ok(())
    }

    fn on_key_press(&self, ev: KeyPressEvent) -> xcb::Result<()> {
        let window_id = ev.event();
        // CTRL + Q (on qwerty)
        // TODO: support keymaps
        if ev.state().contains(x::KeyButMask::CONTROL) && ev.detail() == 0x18 {
            // Check if the window has declared support for WM_DELETE_WINDOW
            let property = self.conn.wait_for_reply(self.conn.send_request(&x::GetProperty {
                delete: false,
                window: window_id,
                property: self.atoms.wm_protocols,
                r#type: x::ATOM_ATOM,
                long_offset: 0,
                long_length: u32::MAX,
            }))?;

            let supports_wm_delete_window = property.value::<x::Atom>().contains(&self.atoms.wm_del_window);
            match supports_wm_delete_window {
                // If it does support it, send an event to kill it gracefully
                true => {
                    let data = x::ClientMessageData::Data32([
                        self.atoms.wm_del_window.resource_id(),
                        x::CURRENT_TIME,
                        0,
                        0,
                        0,
                    ]);

                    let event = x::ClientMessageEvent::new(window_id, self.atoms.wm_protocols, data);

                    self.conn.send_request(&x::SendEvent {
                        propagate: false,
                        destination: x::SendEventDest::Window(window_id),
                        event_mask: x::EventMask::NO_EVENT,
                        event: &event,
                    });

                    self.conn.flush()?;
                }
                // If it doesn't support it, just kill the client
                false => {
                    self.conn.send_and_check_request(&x::KillClient {
                        resource: window_id.resource_id(),
                    })?;
                }
            }
        }

        Ok(())
    }

    fn on_key_release(&self, _ev: KeyPressEvent) -> xcb::Result<()> {
        Ok(())
    }

    pub fn run(&mut self) -> xcb::Result<()> {
        self.become_window_manager()?;
        self.reparent_existing_windows()?;

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

                // Ignored events
                xcb::Event::X(x::Event::ReparentNotify(_)) => {}
                xcb::Event::X(x::Event::CreateNotify(_)) => {}
                xcb::Event::X(x::Event::DestroyNotify(_)) => {}
                xcb::Event::X(x::Event::ConfigureNotify(_)) => {}
                xcb::Event::X(x::Event::MappingNotify(_)) => {}
                xcb::Event::X(x::Event::MapNotify(_)) => {}
                _ => {
                    println!("{:#?}", event);
                }
            }
        }
    }
}
