use x_test_runner::XTestRunner;

mod test_window;
mod x_test_runner;

lazy_static::lazy_static! {
    pub static ref X_TEST_RUNNER: XTestRunner = XTestRunner::new();
}

#[macro_export]
macro_rules! wm_test {
    ($name:ident, $func:expr) => {
        #[test]
        fn $name() {
            $func(crate::X_TEST_RUNNER.test());
        }
    };
}
