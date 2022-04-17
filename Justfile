build:
	cargo build

debug: build
	xinit ./.vscode/xinitrc.debug -- "$(which Xephyr)" :100 -ac -screen 800x600 -host-cursor

run: build
	xinit ./.vscode/xinitrc.run -- "$(which Xephyr)" :100 -ac -screen 800x600 -host-cursor
