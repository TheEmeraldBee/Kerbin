run:
	cargo run -p kerbin -- -c ./config

release:
	cargo run -p kerbin --release -- -c ./config

rebuild-config:
	cargo run --bin booster -- generate -c ./config -b ./

mini:
	cargo clean && cargo build --profile mini
