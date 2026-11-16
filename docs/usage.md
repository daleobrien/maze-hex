# Drawing mazes

[Back to the README](../README.md)

## Make tasks

| Command | What it does |
| ------- | ------------ |
| `make build` | Build everything |
| `make test` | Run the tests |
| `make lint` | `cargo fmt --check`, then clippy with warnings as errors |
| `make fmt` | Format the code |
| `make cli` | Run the `maze-hex` command in `cli/`, with `CLI_ARGS` as its options |
| `make random` | A maze on a random mix of triangles, squares and hexagons (`RANDOM_ARGS`) |
| `make maze` | Time one run (`MAZE_ARGS`) |
| `make tilings` | Draw every tiling into `docs/tilings/` |
| `make sample` | Redraw the sample mazes in `docs/` |
| `make cover` | Redraw `docs/cover.png` (`COVER_ARGS`; needs rsvg-convert and ImageMagick) |

## Examples

```bash
make cli CLI_ARGS="--seed 1 -W 30 -H 20 -S -o maze"   # cli/maze.svg, with the solution
make cli CLI_ARGS="--seed 1 -P -o plain"              # plain walls instead of ink
make cli CLI_ARGS="--tiling list"                     # every tiling's name
make cli CLI_ARGS="--tiling snub-hexagonal -S"        # another tiling
make cli CLI_ARGS="--tiling random --mix 1,0,2 -S"    # a mix with no squares, twice the hexagons
make cli CLI_ARGS="--seed 1 -C -S -o rings"           # every dead end ends in a ring
make cli CLI_ARGS="--mode hub --algorithm prim"       # one track block per tile
make cli CLI_ARGS="--shape circle -W 30 -H 30"        # a round maze
```

The same options and seed always give the same maze. Without `--seed` one is
taken from the clock and printed, so a maze you like can be drawn again.

## Every option

```text
maze-hex: mazes on tilings of equal-edged polygons

usage: maze-hex [options]

  --tiling NAME       tiling to draw on (default hexagonal); `list` prints every name
  --seed N            random seed, so the same flags give the same maze
                      (default: from the clock, and printed to stderr)
  -W, --width N       width in edge lengths (default 30)
  -H, --height N      height in edge lengths (default 20)
  --shape SHAPE       rectangle, hexagon or circle (default rectangle)
  --mix T,S,H         with --tiling random: how much of each shape, as three weights for
                      triangles, squares and hexagons (default 1,1,1). A pull, not an exact
                      ratio; squares and hexagons cannot meet without triangles
  -o, --output NAME   file to write, NAME.svg (default maze)
  -S, --solution      draw the route from entrance to exit in red
  -B, --boundaries    draw faint tile outlines
  -N, --numbers       draw each tile's number
  -T, --ports         draw a dot on every open port
  -P, --plain         even black walls instead of hand-drawn ink
  -C, --circles       end every dead end in a ring (the entrance and exit
                      always have one where it fits)
  --track-width F     width of the track (default 0.4)
  --cap-depth F       how far a dead end reaches into its tile (default 0.3)
  --mode MODE         pass-through or hub (default pass-through); hub puts one
                      track block in each tile, so the maze spans every tile
  --algorithm NAME    hub mode only: wilson, backtracker, kruskal or prim (default wilson)
  --pass-through F    how often a tile may take a second track, 0 to 1 (default 0.7)
  --crossings F       how often a track may cross another as a bridge, 0 to 1
                      (default 0.5); tiles of four or more sides only
  --junctions F       how readily tracks join into junctions, 0 to 1 (default
                      0.3); lower favours bridges and through-tracks
  --straightness F    preference for going straight through a tile, 0 to 1 (default 0.3)
  -h, --help          print this help and exit
```
