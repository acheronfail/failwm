# Check that commands exist
@check +CMDS:
    echo {{CMDS}} | xargs -n1 sh -c 'if ! command -v $1 &> /dev/null; then echo "$1 is required!"; exit 1; fi' bash

# Build for release
build-release:
	cargo build --features=default --release

# Debug build - also enables xcb dev features
build-debug:
	cargo build --features=debug

# Run in an X server and STOP for debugger to attach
debug: build-debug
	xinit ./.vscode/xinitrc.debug -- "$(which Xephyr)" :101 -ac -screen 800x600 -host-cursor

# Run in an X server
run: build-debug
	xinit ./.vscode/xinitrc.run -- "$(which Xephyr)" :100 -ac -screen 800x600 -host-cursor
run-fs: build-debug
	xinit ./.vscode/xinitrc.run -- "$(which Xephyr)" :100 -ac -fullscreen host-cursor

# Run the tests: arguments are passed to `cargo test`
test *ARGS: build-debug (check "Xephyr" "Xvfb")
	cargo test -- {{ARGS}}

# Run the tests with test debug environment variables set.
# See `./r3/test_integration/README.md`.
# Arguments are passed to `cargo test`
test-debug *ARGS: build-debug (check "Xephyr" "Xvfb")
	TEST_ENABLE_XEPHYR=1 \
		TEST_ENABLE_R3_STDIO=1 \
		TEST_ENABLE_X_STDIO=1 \
		cargo test -- {{ARGS}}
