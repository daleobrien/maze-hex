# CLAUDE.md

Guidance for AI assistants working on maze-hex. Read PLAN.md first; it is the
design and the roadmap.

## Rules

- Keep lint happy: no build warnings, `cargo fmt --all` clean, and
  `cargo clippy --workspace --all-targets -- -D warnings` clean.
  `make lint` checks all three; run it before every commit.
- Commits go straight to `main`.
- Geometry stays exact (`geometry::Q3`, `geometry::Point`) until drawing. Never
  compare or hash float coordinates; join tiles by exact points or ids.
- `lib` does no file I/O so it can build for wasm; the CLI does the writing.
- A tile only ever draws inside itself (PLAN.md §5.4). If a change would let a
  wall leave its tile, fix the clearance table instead of adding drawing order.
- Follow Lubra's conventions (Makefile tasks, CLI flags, ink style, red
  entrance/exit markers) unless PLAN.md says otherwise.
- Comments explain why, in plain sentences. No `unwrap` in library code without
  a comment saying why it cannot fail.
- New tilings go in `lib/src/tiling/catalogue.rs`; the tests there check every
  entry automatically (area, exact unit edges, 360° at every interior vertex).
