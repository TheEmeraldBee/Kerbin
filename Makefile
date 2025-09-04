build:
	cargo build

run: build build-config
	cargo run

build-config:
	cd config && make build
