use xcb::x::EventMask;

lazy_static::lazy_static! {
    pub static ref MASKS: Masks = Masks::new();
}

pub struct Masks {
    /// Events for the child windows (the actual windows)
    pub child_window_events: EventMask,
    /// Events for the frame windows
    pub frame_window_events: EventMask,
    /// Events for the root window
    pub root_window_events: EventMask,
}

impl Masks {
    fn new() -> Masks {
        Masks {
            child_window_events: EventMask::PROPERTY_CHANGE | EventMask::SUBSTRUCTURE_NOTIFY | EventMask::FOCUS_CHANGE,
            frame_window_events: EventMask::BUTTON_PRESS // Mouse pressed
                | EventMask::BUTTON_RELEASE              // Mouse released
                | EventMask::BUTTON_MOTION               // Mouse moved while pressed
                | EventMask::EXPOSURE                    // Frame needs to be redrawn (TODO doc)
                | EventMask::STRUCTURE_NOTIFY            // Frame gets destroyed
                | EventMask::SUBSTRUCTURE_NOTIFY         // Subwindows get notifies
                | EventMask::SUBSTRUCTURE_REDIRECT       // Inner application tries to configure itself (resize, etc)
                | EventMask::ENTER_WINDOW, // Pointer is moved into the frame
            root_window_events: EventMask::BUTTON_PRESS  // Mouse pressed on root window
                | EventMask::STRUCTURE_NOTIFY            // When a screen is added (another output) root window gets configure notify
                | EventMask::SUBSTRUCTURE_REDIRECT
                | EventMask::POINTER_MOTION              // Pointer motion on root window
                | EventMask::PROPERTY_CHANGE
                | EventMask::FOCUS_CHANGE
                | EventMask::ENTER_WINDOW, // Pointer moved onto root window
        }
    }
}
