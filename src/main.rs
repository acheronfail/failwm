mod macros;
mod point;
mod rect;
mod wm;

// TODO: consider abstracting away X-specific items, and allowing Wayland impls too?
//  unsure how difficult this will be (seems to be mostly X code for now)
fn main() -> xcb::Result<()> {
    // Stop and wait for debugger if R3_DEBUG present
    if matches!(std::env::var("R3_DEBUG"), Ok(_)) {
        nix::sys::signal::raise(nix::sys::signal::SIGSTOP).unwrap();
    }

    let (conn, screen_num) = xcb::Connection::connect_with_extensions(None, &[xcb::Extension::Xkb], &[])?;

    // Event loop
    let mut app = wm::WindowManager::new(conn, screen_num)?;
    app.run()
}
