# Build, test and lint tasks for the maze-hex workspace.

CARGO ?= cargo
# `make maze` times a run with bash's `time` keyword, which dash lacks.
SHELL := /bin/bash

# The arguments for each CLI task. Override them on the command line, e.g.
# `make cli CLI_ARGS="--seed 7 -S"`.
CLI_ARGS ?= --seed 1 -W 30 -H 20 -B -o maze
MAZE_ARGS ?= --seed 3 -W 60 -H 40 -o maze
SAMPLE_ARGS ?= --seed 1 -W 30 -H 20
RANDOM_ARGS ?= --tiling random --mix 1,1,1 --seed 1 -W 30 -H 20 -S -o random

.DEFAULT_GOAL := help
.PHONY: help build test lint fmt tilings cli random maze sample

help:
	@echo "  make build    build everything"
	@echo "  make test     run the tests"
	@echo "  make lint     cargo fmt --check and clippy with warnings as errors"
	@echo "  make fmt      format the code"
	@echo "  make tilings  draw every tiling in the catalogue into docs/tilings/"
	@echo "  make cli      run the maze-hex command in cli/ (CLI_ARGS to change its flags)"
	@echo "  make random   draw a maze on a random mix of triangles, squares and hexagons (RANDOM_ARGS)"
	@echo "  make maze     time one run of maze-hex (MAZE_ARGS to change its flags)"
	@echo "  make sample   write docs/sample.svg, docs/sample-solved.svg and docs/sample-mixed.svg and docs/sample-random.svg"

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

cli:
	cd cli && $(CARGO) run --release -- $(CLI_ARGS)

random:
	cd cli && $(CARGO) run --release -- $(RANDOM_ARGS)

maze:
	$(CARGO) build --release -p cli
	cd cli && time ../target/release/maze-hex $(MAZE_ARGS)

sample:
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) -o docs/sample
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) -S -o docs/sample-solved
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) --tiling triangle-hexagon -S -o docs/sample-mixed
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) --tiling random -S -o docs/sample-random
