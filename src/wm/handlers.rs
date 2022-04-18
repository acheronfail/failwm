use std::cmp;

use xcb::x::{
    self, ButtonPressEvent, ConfigureRequestEvent, KeyPressEvent, MapRequestEvent, MotionNotifyEvent, UnmapNotifyEvent,
};

use super::{DragType, QuitReason, WindowManager};
use crate::{point::Point, ret_if_none};

impl WindowManager {
    /**
     * X Events
     */

    pub(super) fn on_configure_request(&self, ev: ConfigureRequestEvent) -> xcb::Result<()> {
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

    pub(super) fn on_map_request(&mut self, ev: MapRequestEvent) -> xcb::Result<()> {
        let window_id = ev.window();
        // First, we re-parent it with a frame
        self.frame_window(window_id, false)?;
        // Then, we actually map the window
        self.conn.send_and_check_request(&x::MapWindow { window: window_id })?;

        Ok(())
    }

    pub(super) fn on_unmap_notify(&mut self, ev: UnmapNotifyEvent) -> xcb::Result<()> {
        // Any windows existing before we started that are framed in `App::reparent_existing_windows`
        // trigger an UnmapNotify event when they're re-parented. We just ignore these events here.
        if ev.event() == self.get_root()? {
            return Ok(());
        }

        self.unframe_window(ev.window())?;
        Ok(())
    }

    /**
     * Key Events
     */

    pub(super) fn on_key_press(&mut self, ev: KeyPressEvent) -> xcb::Result<()> {
        let window_id = ev.event();
        // CTRL + Q (on qwerty) - kill window
        // TODO: support keymaps
        if ev.state().contains(x::KeyButMask::CONTROL) && ev.detail() == 0x18 {
            self.kill_window(window_id)?;
        }

        // CTRL + SHIFT + Q - kill window manager
        if ev.state().contains(x::KeyButMask::CONTROL | x::KeyButMask::SHIFT) && ev.detail() == 0x18 {
            self.quit_reason = Some(QuitReason::UserQuit);
        }

        Ok(())
    }

    pub(super) fn on_key_release(&self, _ev: KeyPressEvent) -> xcb::Result<()> {
        Ok(())
    }

    /**
     * Mouse Events
     */

    pub(super) fn on_button_press(&mut self, ev: ButtonPressEvent) -> xcb::Result<()> {
        let frame_id = *self.framed_clients.get(&ev.event()).unwrap();
        let geo = self.conn.wait_for_reply(self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(frame_id),
        }))?;

        self.drag_start = Some((ev.root_x(), ev.root_y()).into());
        self.drag_start_frame_rect = Some((geo.x(), geo.y(), geo.width(), geo.height()).into());

        self.conn.send_and_check_request(&x::ConfigureWindow {
            window: ev.event(),
            value_list: &[x::ConfigWindow::StackMode(x::StackMode::Above)],
        })?;

        Ok(())
    }

    pub(super) fn on_motion_notify(&mut self, ev: MotionNotifyEvent) -> xcb::Result<()> {
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

    pub(super) fn on_button_release(&mut self, _ev: ButtonPressEvent) -> xcb::Result<()> {
        self.drag_start = None;
        self.drag_start_frame_rect = None;
        Ok(())
    }
}
