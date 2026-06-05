.PHONY: all dev release debug wleave-release wleave-debug ./target/release/wleave ./target/debug/wleave completions prepare fmt fmt-rust fmt-other lint clean

all: wleave-release

dev: prepare wleave-debug

release: wleave-release

debug: wleave-debug

wleave-release: ./target/release/wleave

wleave-debug: ./target/debug/wleave

./target/release/wleave: $(wildcard src/**.rs)
	cargo build --frozen --release --all-features

./target/debug/wleave: $(wildcard src/**.rs)
	cargo build --all-features

completions: wleave-release
	mkdir -p completions
	OUT_DIR=completions cargo run --package wleave_completions --bin wleave_completions

prepare:
	$(MAKE) clean
	$(MAKE) lint
	$(MAKE) fmt

fmt: fmt-rust fmt-other

fmt-rust:
	cargo fmt

fmt-other:
	prettier --write "*.md" "data/**"

lint:
	cargo clippy --all-features

clean:
	rm -rf ./target ./completions_generated
