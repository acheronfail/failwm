#[macro_export]
macro_rules! ret_ok_if_none {
    ($option:expr) => {{
        match $option {
            Some(x) => x,
            None => return Ok(()),
        }
    }};
}
