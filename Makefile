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
.PHONY: cover gallery help build test lint fmt tilings cli random maze sample timing

help:
	@echo "  make build    build everything"
	@echo "  make test     run the tests"
	@echo "  make lint     cargo fmt --check and clippy with warnings as errors"
	@echo "  make fmt      format the code"
	@echo "  make tilings  draw every tiling in the catalogue into docs/tilings/"
	@echo "  make cli      run the maze-hex command in cli/ (CLI_ARGS to change its flags)"
	@echo "  make random   draw a maze on a random mix of triangles, squares and hexagons (RANDOM_ARGS)"
	@echo "  make maze     time one run of maze-hex (MAZE_ARGS to change its flags)"
	@echo "  make timing   time making and drawing a maze at each of the website's hexagon levels"
	@echo "  make sample   write docs/sample.svg, docs/sample-solved.svg and docs/sample-mixed.svg and docs/sample-random.svg"
	@echo "  make cover    write docs/cover.png, the 1280x640 card the README and daleobrien.github.io show"
	@echo "  make gallery  write docs/mazes.png, one maze on every tiling, for docs/tilings.md"

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

timing:
	$(CARGO) run --release -q -p maze-hex --example timing

sample:
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) -o docs/sample
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) -S -o docs/sample-solved
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) --tiling triangle-hexagon -S -o docs/sample-mixed
	$(CARGO) run --release -p cli -- $(SAMPLE_ARGS) --tiling random -S -o docs/sample-random

# The 1280 by 640 card the README and daleobrien.github.io show, laid out like
# Lubra's: the maze, without its solution, on a white card with a thin rounded
# border. Needs rsvg-convert and ImageMagick.
COVER_ARGS ?= --seed 1 -W 28 -H 12
cover:
	$(CARGO) run --release -p cli -- $(COVER_ARGS) -o /tmp/maze-hex-cover
	rsvg-convert -b white -w 1130 /tmp/maze-hex-cover.svg -o /tmp/maze-hex-cover.png
	convert -size 1280x640 xc:white -fill white -stroke '#d8d8d8' -strokewidth 2 \
		-draw "roundrectangle 1,1 1278,638 18,18" \
		\( /tmp/maze-hex-cover.png -resize 1130x540 \) -gravity center -composite \
		-colors 256 docs/cover.png

# One maze on every tiling, in a grid of three across, for docs/tilings.md:
# the default settings and one seed, without the solution. Needs rsvg-convert
# and ImageMagick.
GALLERY_ARGS ?= --seed 3 -W 18 -H 12
GALLERY_DIR := /tmp/maze-hex-gallery
gallery:
	$(CARGO) build --release -p cli
	rm -rf $(GALLERY_DIR) && mkdir -p $(GALLERY_DIR)
	for t in $$(target/release/maze-hex --tiling list); do \
		target/release/maze-hex --tiling $$t $(GALLERY_ARGS) -o $(GALLERY_DIR)/$$t || exit 1; \
		rsvg-convert -b white -w 420 $(GALLERY_DIR)/$$t.svg -o $(GALLERY_DIR)/$$t.png; \
		convert $(GALLERY_DIR)/$$t.png -resize 420x290 -gravity center -background white \
			-extent 420x290 -gravity north -splice 0x30 -pointsize 19 -annotate +0+6 "$$t" \
			$(GALLERY_DIR)/$$t.png; \
	done
	montage $$(for t in $$(target/release/maze-hex --tiling list); do echo $(GALLERY_DIR)/$$t.png; done) \
		-tile 3x -geometry +10+10 -background white +dither -colors 64 docs/mazes.png
