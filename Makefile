build:
	cargo build --release

run: build build-config
	cargo run -- -c ./config

build-config:
	cd config && make build
