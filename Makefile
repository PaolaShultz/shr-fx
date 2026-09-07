.PHONY: check release smoke soak
check:
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
release:
	cargo build --release --locked
smoke: release
	python3 scripts/smoke_terminal.py target/release/fx
soak:
	cargo test --release --locked --test soak -- --ignored --nocapture
