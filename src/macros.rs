#[macro_export]
macro_rules! ret_if_none {
    ($option:expr) => {{
        match $option {
            Some(x) => x,
            None => return Ok(()),
        }
    }};
}
