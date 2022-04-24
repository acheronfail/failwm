use xcb::{x, Xid};

use super::masks::MASKS;
use super::WindowManager;
use crate::point::Point;
use crate::ret_ok_if_none;
use crate::window_geometry::WindowGeometry;

impl<'a> WindowManager<'a> {
    pub(super) fn get_frame_and_window(&self, target: x::Window) -> Option<(x::Window, x::Window)> {
        if let Some(frame) = self.framed_clients.get_by_left(&target) {
            Some((target, *frame))
        } else if let Some(window) = self.framed_clients.get_by_right(&target) {
            Some((*window, target))
        } else {
            None
        }
    }

    pub(super) fn frame_window(
        &mut self,
        window: x::Window,
        existed_before_wm: bool,
    ) -> xcb::Result<Option<x::Window>> {
        // Get window attributes
        let geo = self.conn.wait_for_reply(self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(window),
        }))?;

        // If window was created before window manager started, we should frame
        // it only if it is visible and doesn't set override_redirect.
        if existed_before_wm {
            let attrs = self
                .conn
                .wait_for_reply(self.conn.send_request(&x::GetWindowAttributes { window }))?;
            if attrs.override_redirect() || attrs.map_state() != x::MapState::Viewable {
                return Ok(None);
            }
        }

        // Create frame
        let frame = self.conn.generate_id();
        let root_window = self.get_root_window()?;
        self.conn.send_and_check_request(&x::CreateWindow {
            depth: x::COPY_FROM_PARENT as u8,   // TODO: ???
            visual: x::COPY_FROM_PARENT as u32, // TODO: get from screen.root_visual()
            wid: frame,
            parent: root_window,
            x: geo.x(),
            y: geo.y(),
            width: geo.width(),
            height: geo.height(),
            border_width: 10,
            class: x::WindowClass::CopyFromParent,
            value_list: &[
                // Frame background color
                // TODO: can I make this transparent in any way?
                x::Cw::BackPixel(0x0000ff),
                // Border pixel colour
                x::Cw::BorderPixel(0xff0000),
                // Which events to capture and send to the event loop
                // NOTE: we ignore enter events during re-parenting
                x::Cw::EventMask(MASKS.frame_window_events & !x::EventMask::ENTER_WINDOW),
            ],
        })?;

        // Set an atom on our frame to indicate that it is indeed a frame
        self.conn.send_and_check_request(&x::ChangeProperty {
            mode: x::PropMode::Replace,
            window: frame,
            property: self.atoms.r3_frame,
            r#type: x::ATOM_STRING,
            data: b"1",
        })?;

        // Start listening to window events
        self.conn.send_and_check_request(&x::ChangeWindowAttributes {
            window,
            // Which events to capture and send to the event loop
            value_list: &[x::Cw::EventMask(MASKS.child_window_events)],
        })?;

        // Add window to save set
        // TODO: doc why
        self.conn.send_and_check_request(&x::ChangeSaveSet {
            window,
            mode: x::SetMode::Insert,
        })?;

        // Re-parent window into frame
        self.conn.send_and_check_request(&x::ReparentWindow {
            window,
            parent: frame,
            // Offset of client window within frame
            x: 0,
            y: 0,
        })?;

        // Map frame
        self.conn.send_and_check_request(&x::MapWindow { window: frame })?;

        // Save association b/w window and frame
        self.framed_clients.insert(window, frame);

        // Button (mouse) handling
        self.conn.send_and_check_request(&x::GrabButton {
            grab_window: window,
            owner_events: false,
            event_mask: x::EventMask::BUTTON_PRESS | x::EventMask::BUTTON_RELEASE | x::EventMask::BUTTON_MOTION,
            pointer_mode: x::GrabMode::Async,
            keyboard_mode: x::GrabMode::Async,
            confine_to: root_window,
            cursor: xcb::Xid::none(),
            button: x::ButtonIndex::Any,
            modifiers: x::ModMask::ANY,
        })?;

        // After mapping and re-parenting, configure all the events (including enter window)
        self.conn.send_and_check_request(&x::ChangeWindowAttributes {
            window: frame,
            value_list: &[x::Cw::EventMask(MASKS.frame_window_events)],
        })?;

        Ok(Some(frame))
    }

    pub(super) fn unframe_window(&mut self, target: x::Window) -> xcb::Result<()> {
        let (window, frame) = ret_ok_if_none!(self.get_frame_and_window(target));

        // If it was the frame that was unmapped, then we don't need to do anything.
        if target == frame {
            self.framed_clients.remove_by_right(&frame);
            if self.focused_window == Some(frame) {
                self.focused_window = None;
            }

            return Ok(());
        }

        // Unmap frame
        self.conn.send_and_check_request(&x::UnmapWindow { window: frame })?;

        // Re-parent client window back to root
        // FIXME: when checked this and others below error with BadWindow(3)
        self.conn.send_request_checked(&x::ReparentWindow {
            window,
            parent: self.get_root_window()?,
            // Offset of client within root
            x: 0,
            y: 0,
        });

        // Remove client window from save set, since we're not managing it anymore
        self.conn.send_request_checked(&x::ChangeSaveSet {
            window,
            mode: x::SetMode::Delete,
        });

        // Destroy the frame
        self.conn.send_request_checked(&x::DestroyWindow { window: frame });

        // Drop window->frame association
        self.framed_clients.remove_by_left(&window);

        // If it was the focused window, remove it
        if self.focused_window == Some(window) || self.focused_window == Some(frame) {
            self.focused_window = None;
        }

        self.conn.flush()?;

        Ok(())
    }

    fn supports_wm_delete_window(&self, window: x::Window) -> xcb::Result<bool> {
        // Check if the window has declared support for WM_DELETE_WINDOW
        let property = self.conn.wait_for_reply(self.conn.send_request(&x::GetProperty {
            delete: false,
            window,
            property: self.atoms.wm_protocols,
            r#type: x::ATOM_ATOM,
            long_offset: 0,
            long_length: u32::MAX,
        }))?;

        let protocols = property.value::<x::Atom>();
        Ok(protocols.contains(&self.atoms.wm_del_window))
    }

    pub(super) fn kill_window(&self, target: x::Window) -> xcb::Result<()> {
        // If the window supports WM_DELETE_WINDOW, then we tell it to exit - when we receive the
        // UnmapNotify event for that window we'll clean up the frame. If the target doesn't support
        // WM_DELETE_WINDOW, then we just destroy the frame itself which will destroy the child window.
        let (window, frame) = self.get_frame_and_window(target).unwrap_or((target, target));

        // Don't kill the root window! xD
        if target == self.get_root_window()? {
            return Ok(());
        }

        match self.supports_wm_delete_window(window)? {
            // If it does support it, send an event to kill it gracefully
            true => {
                let data =
                    x::ClientMessageData::Data32([self.atoms.wm_del_window.resource_id(), x::CURRENT_TIME, 0, 0, 0]);

                self.conn.send_request(&x::SendEvent {
                    propagate: false,
                    destination: x::SendEventDest::Window(window),
                    event_mask: x::EventMask::NO_EVENT,
                    event: &x::ClientMessageEvent::new(window, self.atoms.wm_protocols, data),
                });

                self.conn.flush()?;
            }
            // If it doesn't support it, just destroy the window
            false => {
                self.conn.send_and_check_request(&x::DestroyWindow { window: frame })?;
            }
        }

        Ok(())
    }

    pub(super) fn move_window(&self, window: x::Window, pos: Point) -> xcb::Result<()> {
        let value_list = &[x::ConfigWindow::X(pos.x.into()), x::ConfigWindow::Y(pos.y.into())];

        let id = match self.framed_clients.get_by_left(&window) {
            // If it has a frame, move the frame
            Some(frame) => *frame,
            // If it doesn't, just move the window
            None => window,
        };

        // Move window
        self.conn
            .send_and_check_request(&x::ConfigureWindow { window: id, value_list })?;

        Ok(())
    }

    pub(super) fn resize_window(&self, window: x::Window, rect: WindowGeometry) -> xcb::Result<()> {
        let mut value_list = vec![
            x::ConfigWindow::X(rect.x.into()),
            x::ConfigWindow::Y(rect.y.into()),
            x::ConfigWindow::Width(rect.w.into()),
            x::ConfigWindow::Height(rect.h.into()),
        ];

        // Move frame if it has one
        if let Some(frame_id) = self.framed_clients.get_by_left(&window) {
            self.conn.send_and_check_request(&x::ConfigureWindow {
                window: *frame_id,
                value_list: &value_list,
            })?;

            // NOTE: x and y coords are relative to parent window (in this case the frame)
            value_list[0] = x::ConfigWindow::X(0);
            value_list[1] = x::ConfigWindow::Y(0);
        }

        // Move window
        self.conn.send_and_check_request(&x::ConfigureWindow {
            window,
            value_list: &value_list,
        })?;

        Ok(())
    }

    pub(super) fn get_window_rect(&self, target: x::Window) -> xcb::Result<WindowGeometry> {
        let geo = self.conn.wait_for_reply(self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(target),
        }))?;

        let x = geo.x();
        let y = geo.y();
        let w = geo.width();
        let h = geo.height();
        let bw = geo.border_width();
        Ok((x, y, w, h, bw).into())
    }

    pub(super) fn window_at_pos(&self, root: x::Window, pos: Point) -> xcb::Result<Option<x::Window>> {
        let query_tree = self
            .conn
            .wait_for_reply(self.conn.send_request(&x::QueryTree { window: root }))?;

        assert_eq!(root, query_tree.root());

        for window in query_tree.children() {
            // FIXME: check visibility and stacking order, etc - this kills the wrong window if another is placed above it
            //  this entire function can be removed once we sort out "focus"
            let win_rect = self.get_window_rect(*window)?;
            if win_rect.contains(&pos) {
                return Ok(Some(*window));
            }
        }

        Ok(None)
    }
}
