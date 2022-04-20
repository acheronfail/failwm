use xcb::{x, Xid};

use crate::{point::Point, rect::Rect};

use super::{masks::MASKS, WindowManager};

impl WindowManager {
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
            visual: x::COPY_FROM_PARENT as u32, // TODO: ???
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
            modifiers: x::ModMask::CONTROL,
        })?;

        // After mapping and re-parenting, configure all the events (including enter window)
        self.conn.send_and_check_request(&x::ChangeWindowAttributes {
            window: frame,
            value_list: &[x::Cw::EventMask(MASKS.frame_window_events)],
        })?;

        Ok(Some(frame))
    }

    pub(super) fn unframe_window(&mut self, window_id: x::Window) -> xcb::Result<()> {
        let frame_id = match self.framed_clients.get_by_left(&window_id) {
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
            parent: self.get_root_window()?,
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
        self.framed_clients.remove_by_left(&window_id);

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

        Ok(property.value::<x::Atom>().contains(&self.atoms.wm_del_window))
    }

    pub(super) fn kill_window(&self, target: x::Window) -> xcb::Result<()> {
        // If the target is a frame, then kill its window; otherwise kill the target.
        let target = *self.framed_clients.get_by_right(&target).unwrap_or(&target);
        match self.supports_wm_delete_window(target)? {
            // If it does support it, send an event to kill it gracefully
            true => {
                let data =
                    x::ClientMessageData::Data32([self.atoms.wm_del_window.resource_id(), x::CURRENT_TIME, 0, 0, 0]);

                self.conn.send_request(&x::SendEvent {
                    propagate: false,
                    destination: x::SendEventDest::Window(target),
                    event_mask: x::EventMask::NO_EVENT,
                    event: &x::ClientMessageEvent::new(target, self.atoms.wm_protocols, data),
                });

                self.conn.flush()?;
            }
            // If it doesn't support it, just kill the client
            false => {
                self.conn.send_and_check_request(&x::KillClient {
                    resource: target.resource_id(),
                })?;
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

    pub(super) fn resize_window(&self, window: x::Window, rect: Rect) -> xcb::Result<()> {
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

    pub(super) fn get_window_rect(&self, target: x::Window) -> xcb::Result<Rect> {
        let geo = self.conn.wait_for_reply(self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(target),
        }))?;

        Ok((geo.x(), geo.y(), geo.width(), geo.height()).into())
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
