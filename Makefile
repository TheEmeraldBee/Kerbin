run: build-test-plugin
	cargo run

build-test-plugin:
	cargo build -p test_plugin --release
