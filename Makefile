run:
	cargo run -p kerbin -- -c ./config

release:
	cargo run -p kerbin --release -- -c ./config

mini:
	cargo clean && cargo build --profile mini

jj:
	jj bookmark move master class --to @ &&\
	jj git push &&\
	jj git push --remote class
