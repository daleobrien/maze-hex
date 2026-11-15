<h1 align="center">maze-hex</h1>

<p align="center">Hand-drawn track mazes on hexagons, and on any tiling of equal-edged triangles, squares and hexagons, in Rust.</p>

<p align="center">
  <a href="PLAN.md"><b>Plan</b></a> &nbsp;·&nbsp;
  <a href="#quick-start">Quick start</a> &nbsp;·&nbsp;
  <a href="#how-it-works">How it works</a>
</p>

<p align="center">
  <img src="docs/tilings.png" alt="The six tilings maze-hex can build so far" width="100%">
</p>

maze-hex is a sibling of [Lubra](https://github.com/daleobrien/lubra). It also
makes winding tracks drawn in ink, but tracks only ever join a tile at the
middle of an edge. No track passes over another, so there are no fragments to
generate and no drawing order to worry about. Every edge in a tiling has the
same length, so a track can pass from a hexagon to a square to a triangle with
the same width.

**Status:** early. The exact geometry and six tilings are built and tested;
the maze itself comes next. See [PLAN.md](PLAN.md).

## Quick start

```bash
make build     # build everything
make test      # run the tests
make lint      # cargo fmt --check and clippy, warnings as errors
make tilings   # draw every tiling into docs/tilings/
make           # list every task
```

## How it works

- `lib/src/geometry.rs`: exact coordinates. Every edge points in a multiple
  of 30°, so every vertex is `(a + b√3)/2` for integers `a`, `b`, and
  neighbouring tiles agree exactly on shared corners.
- `lib/src/tiling/`: a tiling is a unit cell repeated over a rectangle and
  clipped. `catalogue.rs` holds hexagonal (6.6.6), square, triangular,
  trihexagonal (3.6.3.6), rhombitrihexagonal (3.4.6.4) and elongated
  triangular (3.3.3.4.4).

---

<p align="center"><sub>Part of <a href="https://daleobrien.github.io/#mazes">Mazes</a> &nbsp;·&nbsp; more at <a href="https://daleobrien.github.io">daleobrien.github.io</a></sub></p>
