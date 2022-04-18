use xcb::{x, Xid};

use super::WindowManager;

impl WindowManager {
    pub(super) fn frame_window(&mut self, window_id: x::Window, existed_before_wm: bool) -> xcb::Result<()> {
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
            if attrs.override_redirect() || attrs.map_state() != x::MapState::Viewable {
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
            modifiers: x::ModMask::CONTROL,
        })?;

        self.conn.send_and_check_request(&x::GrabKey {
            grab_window: window_id,
            owner_events: false,
            key: 0x18, // Q on qwerty TODO: support keymaps
            pointer_mode: x::GrabMode::Async,
            keyboard_mode: x::GrabMode::Async,
            modifiers: x::ModMask::ANY,
        })?;

        Ok(())
    }

    pub(super) fn unframe_window(&mut self, window_id: x::Window) -> xcb::Result<()> {
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

    pub(super) fn supports_wm_delete_window(&self, window: x::Window) -> xcb::Result<bool> {
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

    pub(super) fn kill_window(&self, window: x::Window) -> xcb::Result<()> {
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
            // If it doesn't support it, just kill the client
            false => {
                self.conn.send_and_check_request(&x::KillClient {
                    resource: window.resource_id(),
                })?;
            }
        }

        Ok(())
    }
}
