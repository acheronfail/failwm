use std::cmp;

use xcb::x::{
    self, ButtonPressEvent, ConfigureRequestEvent, KeyPressEvent, MapRequestEvent, MotionNotifyEvent, UnmapNotifyEvent,
};

use super::{DragType, QuitReason, WindowManager};
use crate::{point::Point, rect::Quadrant, ret_ok_if_none};

impl WindowManager {
    /**
     * X Events
     */

    pub(super) fn on_configure_request(&self, ev: ConfigureRequestEvent) -> xcb::Result<()> {
        let window = ev.window();
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
        if let Some(frame_id) = self.framed_clients.get_by_left(&window) {
            self.conn.send_and_check_request(&x::ConfigureWindow {
                window: *frame_id,
                value_list: &value_list,
            })?;
        }

        // Pass request straight through to the X server for window
        self.conn.send_and_check_request(&x::ConfigureWindow {
            window,
            value_list: &value_list,
        })?;

        Ok(())
    }

    pub(super) fn on_map_request(&mut self, ev: MapRequestEvent) -> xcb::Result<()> {
        let window = ev.window();
        // First, we re-parent it with a frame
        self.frame_window(window, false)?;
        // Then, we actually map the window
        self.conn.send_and_check_request(&x::MapWindow { window })?;

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

    // TODO: remove hardcoded values when configuration is available
    pub(super) fn on_key_press(&mut self, ev: KeyPressEvent) -> xcb::Result<()> {
        let window_id = ev.event();
        // CTRL + Q (on qwerty) - kill window
        if ev.state().contains(x::KeyButMask::CONTROL) && ev.detail() == 0x18 {
            self.kill_window(window_id)?;
        }

        // CTRL + SHIFT + Q - kill window manager
        // TODO: this has to be fired on a window
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
        let target = ev.event();
        let (_, frame) = ret_ok_if_none!(self.get_frame_and_window(target));
        let geo = self.conn.wait_for_reply(self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(frame),
        }))?;

        self.drag_start = Some((ev.root_x(), ev.root_y()).into());
        self.drag_start_frame_rect = Some((geo.x(), geo.y(), geo.width(), geo.height()).into());

        self.conn.send_and_check_request(&x::ConfigureWindow {
            window: frame,
            value_list: &[x::ConfigWindow::StackMode(x::StackMode::Above)],
        })?;

        Ok(())
    }

    // TODO: remove hardcoded values when configuration is available
    pub(super) fn on_motion_notify(&mut self, ev: MotionNotifyEvent) -> xcb::Result<()> {
        let target = ev.event();
        let (window, _) = ret_ok_if_none!(self.get_frame_and_window(target));

        let drag_start = ret_ok_if_none!(self.drag_start);
        let drag_start_frame_rect = ret_ok_if_none!(self.drag_start_frame_rect);

        let delta = Point::new(ev.root_x(), ev.root_y()) - drag_start;
        let drag_type = ret_ok_if_none!(if ev.state().contains(x::KeyButMask::BUTTON1) {
            Some(DragType::Move)
        } else if ev.state().contains(x::KeyButMask::BUTTON3) {
            Some(DragType::Resize)
        } else {
            None
        });

        match drag_type {
            DragType::Move => self.move_window(
                window,
                (drag_start_frame_rect.x + delta.x, drag_start_frame_rect.y + delta.y).into(),
            )?,
            DragType::Resize => self.resize_window(
                window,
                match ret_ok_if_none!(drag_start_frame_rect.quadrant(&drag_start)) {
                    Quadrant::TopLeft => (
                        drag_start_frame_rect.x + delta.x,
                        drag_start_frame_rect.y + delta.y,
                        cmp::max(1, drag_start_frame_rect.w as i32 - delta.x as i32) as u16,
                        cmp::max(1, drag_start_frame_rect.h as i32 - delta.y as i32) as u16,
                    ),
                    Quadrant::TopRight => (
                        drag_start_frame_rect.x,
                        drag_start_frame_rect.y + delta.y,
                        cmp::max(1, drag_start_frame_rect.w as i32 + delta.x as i32) as u16,
                        cmp::max(1, drag_start_frame_rect.h as i32 - delta.y as i32) as u16,
                    ),
                    Quadrant::BottomLeft => (
                        drag_start_frame_rect.x + delta.x,
                        drag_start_frame_rect.y,
                        cmp::max(1, drag_start_frame_rect.w as i32 - delta.x as i32) as u16,
                        cmp::max(1, drag_start_frame_rect.h as i32 + delta.y as i32) as u16,
                    ),
                    Quadrant::BottomRight => (
                        drag_start_frame_rect.x,
                        drag_start_frame_rect.y,
                        cmp::max(1, drag_start_frame_rect.w as i32 + delta.x as i32) as u16,
                        cmp::max(1, drag_start_frame_rect.h as i32 + delta.y as i32) as u16,
                    ),
                }
                .into(),
            )?,
        }

        Ok(())
    }

    pub(super) fn on_button_release(&mut self, _ev: ButtonPressEvent) -> xcb::Result<()> {
        self.drag_start = None;
        self.drag_start_frame_rect = None;
        Ok(())
    }
}
