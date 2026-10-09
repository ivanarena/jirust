.PHONY: build test check install
build:
	cargo build --release --locked
	install -Dm755 target/release/jiraui bin/jiraui
test:
	cargo test --locked
check:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
install: build
	install -Dm755 bin/jiraui "$(HOME)/.local/bin/jiraui"
