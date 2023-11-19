l:
	@ just -l

rr *args:
	cargo run --release -- {{args}}

cr:
	cargo check --release

br:
	cargo build --release

fmt:
	cargo fmt

