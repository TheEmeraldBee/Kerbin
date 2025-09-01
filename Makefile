run: build-plugin
	cargo run

build-plugin:
	cargo build -p test_plugin --release
