pub struct Config {
    pub focus_follows_mouse: bool,
}

impl Config {
    pub fn new() -> Config {
        Config {
            focus_follows_mouse: true,
        }
    }
}
