# How it works

[Back to the README](../README.md)

This is a short tour. [The design](design.md) has the full story, with the
numbers behind each rule.

## Exact geometry

Every edge in every tiling points in a multiple of 30°, so every corner is
`(a + b√3)/2` for integers `a` and `b` (`lib/src/geometry.rs`). Neighbouring
tiles agree exactly on the corners they share, and tiles are joined by exact
points rather than floating-point ones. Coordinates only become floats when
they are drawn.

## Tilings

A tiling is just faces, edges and points (`lib/src/tiling/`); nothing else in
the program knows about a grid or a unit cell. The periodic tilings repeat a
unit cell from `catalogue.rs` over the region; the random ones merge
triangles into hexagons (`merge.rs`), lay random rows (`rows.rs`) or grow tile
by tile (`grow.rs`). The region is then cut to its shape, and only the largest
piece joined edge to edge is kept. See [Tilings](tilings.md).

## Tracks inside a tile

A track leaves a tile at the middle of an edge, at right angles to it
(`lib/src/track/`). Between two sides of a regular polygon there are only
three cases, each exact:

| Sides | Curve |
| ----- | ----- |
| Next to each other | an arc of radius ½ round their shared corner |
| Opposite | a straight line |
| Two apart (hexagons) | an arc of radius 3⁄2 round the point where the two edge lines meet |

A track is a band round that centreline, so its walls are offsets of the
same curves. One side open is a dead end with a rounded cap, two is a bend or
a straight, and three or more is a junction.

## Clearance

Every wall a tile draws stays inside that tile, clear of its closed edges.
So tiles never get in each other's way, and nothing depends on the order
they are drawn in. A table (`clearance.rs`), built at start-up, lists which
groupings of a tile's sides can be drawn at the chosen track width without
the walls crowding.

## Building the maze

In pass-through mode (the default) the maze is a randomised Kruskal over the
ports, the middles of edges (`lib/src/maze/generate.rs`). A link joins two
sides of one tile, and is accepted when it closes no loop, crosses nothing
else in the tile and passes the clearance table. So one tile can carry
several separate tracks, or a junction. Repair passes then join any trees
left over, cap dangling ports as dead ends, and grow short branches into
empty tiles.

**Bridges.** Before that, each tile of four or more sides gets a bridge with
probability `--crossings`: two through-tracks crossing at 40° or more. A coin
decides which goes over. The route, drawn in red with `-S`, shows as a faint
trace where it passes under a bridge, as in Lubra.

**Hub mode** (`--mode hub`) allows only one group per tile, which makes the
maze a spanning tree over the tiles. Any classic algorithm then works on any
tiling: Wilson, recursive backtracker, Kruskal or Prim (`hub.rs`).

The entrance and exit are at the bottom left and top right, with Lubra's red
dots, and the solution is a search from one to the other (`solution.rs`).

## Drawing

Each point where a wall meets an open port has exactly two wall pieces, one
from each tile, so the pieces chain across tiles into one continuous line
(`lib/src/render/walls.rs`). The ink pen (`ink.rs`, copied from Lubra) then
draws long strokes that wobble and vary in width, and the SVG holds a few
paths rather than one per tile. `-P` draws even black walls instead.

`lib` does no file I/O, so it also builds for WebAssembly; `play.rs` is the
game interface the maze website uses, ported from Lubra.
