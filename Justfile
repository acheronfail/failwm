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
