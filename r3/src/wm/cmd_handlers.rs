use super::WindowManager;
use r3lib::R3Command;

impl<'a> WindowManager<'a> {
    pub fn handle_command(&mut self, cmd: &R3Command) -> xcb::Result<()> {
        match cmd {
            R3Command::CloseWindow => {
                if let Some(window) = self.focused_window.take() {
                    println!("focused: {:?}", window);
                    self.kill_window(window)?;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
