mod config;
mod macros;
mod point;
mod window_geometry;
mod wm;

use wm::QuitReason;

// TODO: consider abstracting away X-specific items, and allowing Wayland impls too?
//  unsure how difficult this will be (seems to be mostly X code for now)
fn main() -> xcb::Result<()> {
    // Stop and wait for debugger if R3_DEBUG present
    #[cfg(feature = "debug")]
    if matches!(std::env::var("R3_DEBUG"), Ok(_)) {
        nix::sys::signal::raise(nix::sys::signal::SIGSTOP).unwrap();
    }

    // Event loop
    let mut app = wm::WindowManager::new(config::Config::new())?;
    match app.run()? {
        QuitReason::UserQuit => {
            println!("Quitting due to user action.");
        }
    }

    Ok(())
}
