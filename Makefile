# Build, test and lint tasks for the maze-hex workspace.

CARGO ?= cargo

.DEFAULT_GOAL := help
.PHONY: help build test lint fmt tilings

help:
	@echo "  make build    build everything"
	@echo "  make test     run the tests"
	@echo "  make lint     cargo fmt --check and clippy with warnings as errors"
	@echo "  make fmt      format the code"
	@echo "  make tilings  draw every tiling in the catalogue into docs/tilings/"

build:
	$(CARGO) build --workspace

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	$(CARGO) fmt --all

tilings:
	mkdir -p docs/tilings
	$(CARGO) run --release --example tilings -- docs/tilings
