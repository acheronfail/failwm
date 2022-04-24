use r3lib::WMCommand;

use super::WindowManager;

impl<'a> WindowManager<'a> {
    pub fn handle_command(&mut self, cmd: &WMCommand) -> xcb::Result<()> {
        match cmd {
            WMCommand::CloseWindow => {
                if let Some(window) = self.focused_window.take() {
                    println!("focused: {:?}", window);
                    self.kill_window(window)?;
                }
            }
        }

        Ok(())
    }
}
