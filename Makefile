.PHONY: build test check install notices notices-check package
build:
	cargo build --release --locked
	mkdir -p bin
	install -m 755 target/release/jirust bin/jirust
test:
	cargo test --locked
check:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
install: build
	mkdir -p "$(HOME)/.local/bin"
	install -m 755 bin/jirust "$(HOME)/.local/bin/jirust"

notices:
	python3 scripts/notices.py
notices-check:
	python3 scripts/notices.py --check
package: build notices-check
	python3 scripts/package.py
