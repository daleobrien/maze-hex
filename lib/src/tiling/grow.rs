//! Grown free mix (docs/design.md §2.1, item 3): triangles, squares and hexagons in an
//! irregular arrangement, with a chosen ratio. A patch grows outward from one
//! tile, one tile at a time, and backs up when a corner can no longer be
//! completed.
//!
//! Two things keep the mix irregular. Squares and hexagons that would share an
//! edge with their own kind are a last resort, because rows of squares and
//! lattices of hexagons are what an unguided growth settles into; and the
//! shape weights are steered toward the requested shares as the patch grows.
//! The shares are a pull, not a promise: hexagons need a wide gap, and many
//! gaps allow one shape only.
//!
//! Everything is exact. A vertex is an exact [`Point`], and the angle already
//! filled round it is kept as a mask of twelve 30° sectors, so a gap is a run
//! of empty sectors and there is no float angle to compare. Floats appear only
//! in the overlap test, which is a separating-axis test with a small tolerance.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use super::{centre_is_inside, source::TilingSource, PolygonKind, Region, Tiling};
use crate::geometry::{Direction, Point, Q3};
use crate::rng::Rng;

/// The three shapes in the order the weights are given: triangle, square, hexagon.
const KINDS: [PolygonKind; 3] = [
    PolygonKind::Triangle,
    PolygonKind::Square,
    PolygonKind::Hexagon,
];

/// All twelve sectors of a vertex filled.
const FULL: u16 = 0xFFF;

/// How far past the region the patch grows, in edge lengths. A tile whose centre
/// is inside has every corner within one edge of it, so two edges of margin
/// leave every such corner closed, with room to spare for the hexagon outline.
const MARGIN: f64 = 2.0;

/// Two tiles closer than this (centre to centre) can overlap: a hexagon's
/// corners are one edge from its centre.
const REACH: f64 = 2.0;

/// How much two tiles may overlap and still count as touching. Tiles that share
/// an edge or a corner are separated by exactly zero, and float rounding must
/// not read that as an overlap.
const TOLERANCE: f64 = 1e-9;

/// How strongly the shape weights are steered toward the requested shares.
const STEERING: f64 = 12.0;

/// The most tiles one backtrack may remove.
const MAX_UNDO: usize = 40;

/// The work allowed per expected tile before growing gives up and keeps what it has.
const WORK_PER_TILE: usize = 50;

/// A free mix of triangles, squares and hexagons grown outward from the middle.
#[derive(Clone, Copy, Debug)]
pub struct RandomMix {
    /// How likely each shape is to be tried first at a gap: triangles,
    /// squares, hexagons. A weight of 0 leaves that shape out.
    pub weights: [f64; 3],
}

impl Default for RandomMix {
    fn default() -> Self {
        RandomMix {
            weights: [1.0, 1.0, 1.0],
        }
    }
}

/// How a growth went, for tests and for tuning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrowStats {
    /// Tiles in the finished patch, including the margin outside the region.
    pub tiles: usize,
    /// Loop iterations: one per gap visited, whether it was filled or not.
    pub steps: usize,
    /// How many times growth backed up.
    pub backtracks: usize,
    /// Tiles removed by backing up.
    pub undone: usize,
    /// Whether the work cap was hit before the patch closed.
    pub capped: bool,
}

impl RandomMix {
    /// Grows a patch over `region` and returns every tile of it, with how the
    /// growth went. The patch reaches past the region's outline; the caller clips.
    pub fn grow(
        &self,
        region: Region,
        rng: &mut Rng,
    ) -> (Vec<(PolygonKind, Vec<Point>)>, GrowStats) {
        let mut grower = Grower::new(region, self.weights);
        let stats = grower.run(rng);
        let faces = grower
            .tiles
            .iter()
            .map(|tile| {
                let points = tile.vertices.iter().map(|&v| grower.points[v]).collect();
                (tile.kind, points)
            })
            .collect();
        (faces, stats)
    }
}

impl TilingSource for RandomMix {
    fn name(&self) -> &str {
        "random"
    }

    fn build(&self, region: Region, rng: &mut Rng) -> Tiling {
        let (mut faces, _) = self.grow(region, rng);
        faces.retain(|(_, vertices)| centre_is_inside(vertices, region));
        Tiling::from_faces(&faces, region.smooth)
    }
}

/// A tile that has been placed.
struct Placed {
    kind: PolygonKind,
    /// Vertex ids, anticlockwise.
    vertices: Vec<usize>,
    /// The sectors this tile fills at each vertex.
    sectors: Vec<u16>,
    /// The corners in floats, for the overlap test.
    corners: Vec<(f64, f64)>,
    centre: (f64, f64),
}

/// The growing patch.
struct Grower {
    /// The region grown by `MARGIN`, as a box from `-MARGIN`.
    outline: Region,
    centre: (f64, f64),
    /// The kinds with a positive weight, as indices into `KINDS`.
    weights: [f64; 3],
    /// Whether a gap of this many sectors can be filled by the allowed kinds.
    fillable: [bool; 13],
    /// Expected tiles, which sets the work cap.
    expected: usize,

    ids: HashMap<Point, usize>,
    points: Vec<Point>,
    xy: Vec<(f64, f64)>,
    /// The filled sectors at each vertex: bit `k` is the sector between
    /// direction `k` and `k + 1`, counted in 30° steps anticlockwise from east.
    mask: Vec<u16>,
    in_range: Vec<bool>,
    /// Open vertices inside the outline, most filled first, then nearest the
    /// centre, then oldest. The key holds everything, so the order never
    /// depends on a hash.
    frontier: BTreeSet<(u8, u64, usize)>,

    /// Placed tiles in order; only the last ones are ever removed.
    tiles: Vec<Placed>,
    /// The kind of the tile on the left of each placed edge, by its two ends in
    /// the direction the tile walks it.
    edges: HashMap<(Point, Point), PolygonKind>,
    /// How many tiles of each kind are placed.
    counts: [usize; 3],
    /// Tile numbers by the cell of a coarse grid their centres fall in.
    grid: HashMap<(i32, i32), Vec<usize>>,
}

impl Grower {
    fn new(region: Region, weights: [f64; 3]) -> Self {
        // A weight that is negative, infinite or not a number is no weight at all.
        let mut weights = weights.map(|w| if w.is_finite() && w > 0.0 { w } else { 0.0 });
        if weights.iter().all(|&w| w == 0.0) {
            weights = [1.0; 3];
        }
        // gap[n]: can the allowed angles add up to exactly n sectors?
        let mut fillable = [false; 13];
        fillable[0] = true;
        for n in 1..=12 {
            fillable[n] = KINDS.iter().zip(weights).any(|(kind, w)| {
                let angle = usize::from(kind.interior_steps());
                w > 0.0 && n >= angle && fillable[n - angle]
            });
        }
        let outline = Region {
            width: region.width + 2.0 * MARGIN,
            height: region.height + 2.0 * MARGIN,
            ..region
        };
        let area = outline.width * outline.height;
        Grower {
            outline,
            centre: (region.width / 2.0, region.height / 2.0),
            weights,
            fillable,
            // The average tile of an even mix is a little over one unit of area.
            expected: (area / 1.3).ceil() as usize,
            ids: HashMap::new(),
            points: Vec::new(),
            xy: Vec::new(),
            mask: Vec::new(),
            in_range: Vec::new(),
            frontier: BTreeSet::new(),
            tiles: Vec::new(),
            edges: HashMap::new(),
            counts: [0; 3],
            grid: HashMap::new(),
        }
    }

    fn run(&mut self, rng: &mut Rng) -> GrowStats {
        let mut stats = GrowStats::default();
        self.seed_tile(rng);
        let cap = WORK_PER_TILE * self.expected;
        // How many dead ends in a row, and how deep the patch was at the last.
        let mut failures = 0;
        let mut failed_at = 0;
        while let Some(&(_, _, vertex)) = self.frontier.iter().next() {
            if stats.steps >= cap {
                stats.capped = true;
                break;
            }
            stats.steps += 1;
            if self.fill_gap(vertex, rng) {
                // Growing past the depth of the last dead end ends the streak.
                if self.tiles.len() > failed_at {
                    failures = 0;
                }
                continue;
            }
            // No shape fits this corner, so an earlier choice was wrong. Remove a
            // few tiles, more the longer it keeps happening, and try again.
            failures += 1;
            failed_at = self.tiles.len();
            let most = (2 * failures).min(MAX_UNDO).min(self.tiles.len() - 1);
            let count = 1 + rng.below(most.max(1));
            for _ in 0..count.min(self.tiles.len() - 1) {
                self.undo();
                stats.undone += 1;
            }
            stats.backtracks += 1;
        }
        stats.tiles = self.tiles.len();
        stats
    }

    /// Places the first tile, a random kind with a corner near the centre.
    fn seed_tile(&mut self, rng: &mut Rng) {
        let kind = KINDS[self.weighted_order(rng)[0]];
        let corner = |c: f64| Q3::new(2 * c.round() as i64, 0);
        let start = Point::new(corner(self.centre.0), corner(self.centre.1));
        let vertices = walk(kind, start, Direction::new(0));
        self.place(kind, &vertices);
    }

    /// Tries to fill the smallest gap at `vertex`. False if no shape fits.
    fn fill_gap(&mut self, vertex: usize, rng: &mut Rng) -> bool {
        let (start, len) = smallest_gap(self.mask[vertex]);
        let here = self.points[vertex];
        let end = Direction::new(((start + len) % 12) as u8);
        // Every way to fill the gap, in the order to try them.
        let mut options: Vec<(PolygonKind, Vec<Point>)> = Vec::new();
        for index in self.weighted_order(rng) {
            let kind = KINDS[index];
            // The new tile can lean on either side of the gap: starting at the
            // vertex along the edge that opens the gap, or ending there along
            // the edge that closes it. Taking a random side first keeps the
            // patch from growing the same way round every time.
            let from_start = walk(kind, here, Direction::new(start as u8));
            let from_end = walk(kind, here + end.unit(), end.turn(6));
            if rng.chance(0.5) {
                options.push((kind, from_start));
                options.push((kind, from_end));
            } else {
                options.push((kind, from_end));
                options.push((kind, from_start));
            }
        }
        // Squares that share an edge with a square run on in a straight ribbon
        // that nothing can stop, so they are the last resort. Without this, a
        // free mix is mostly triangles and hexagons with a few ribbons of
        // squares across it.
        let (plain, ribbon): (Vec<_>, Vec<_>) = options
            .into_iter()
            .partition(|(kind, vertices)| !self.joins_alike(*kind, vertices));
        for (kind, vertices) in plain.into_iter().chain(ribbon) {
            if self.fits(kind, &vertices) {
                self.place(kind, &vertices);
                return true;
            }
        }
        false
    }

    /// Whether this tile is a square or hexagon that would share an edge with
    /// another of its own kind.
    fn joins_alike(&self, kind: PolygonKind, vertices: &[Point]) -> bool {
        kind != PolygonKind::Triangle
            && (0..vertices.len()).any(|i| {
                let (from, to) = (vertices[i], vertices[(i + 1) % vertices.len()]);
                self.edges.get(&(to, from)) == Some(&kind)
            })
    }

    /// The weights nudged toward the requested shares of tiles. Many gaps allow
    /// only one shape, so the shapes tried first would otherwise end up far from
    /// the ratio asked for: triangles and squares fill the forced gaps and
    /// hexagons, which need a wide gap, would be starved. A shape that is below
    /// its share is tried first more often, and one above it less often.
    fn steered_weights(&self) -> [f64; 3] {
        let placed = self.tiles.len().max(1) as f64;
        let total: f64 = self.weights.iter().sum();
        let mut steered = self.weights;
        for (k, weight) in steered.iter_mut().enumerate() {
            let wanted = *weight / total;
            let share = self.counts[k] as f64 / placed;
            *weight *= (STEERING * (wanted - share)).exp();
        }
        steered
    }

    /// Weights turned into an order: the kinds with a weight, each drawn without
    /// replacement with a chance in proportion to its weight.
    fn weighted_order(&self, rng: &mut Rng) -> Vec<usize> {
        let weights = self.steered_weights();
        let mut left: Vec<usize> = (0..3).filter(|&k| weights[k] > 0.0).collect();
        let mut order = Vec::with_capacity(3);
        while !left.is_empty() {
            let total: f64 = left.iter().map(|&k| weights[k]).sum();
            let mut ticket = rng.unit() * total;
            let mut pick = left.len() - 1;
            for (i, &k) in left.iter().enumerate() {
                if ticket < weights[k] {
                    pick = i;
                    break;
                }
                ticket -= weights[k];
            }
            order.push(left.remove(pick));
        }
        order
    }

    /// Whether a tile with these corners can be added: it must not overlap a
    /// placed tile, and no corner may end up with a gap that nothing can fill.
    fn fits(&self, kind: PolygonKind, vertices: &[Point]) -> bool {
        let sectors = sectors_at(kind, vertices);
        for (p, bits) in vertices.iter().zip(&sectors) {
            // A corner that is not in the patch yet has nothing round it.
            let old = self.ids.get(p).map_or(0, |&id| self.mask[id]);
            if old & bits != 0 || !self.gaps_fillable(old | bits) {
                return false;
            }
        }
        let corners: Vec<(f64, f64)> = vertices.iter().map(|p| p.to_f64()).collect();
        let centre = centre_of(&corners);
        let (cx, cy) = cell_of(centre);
        for gx in cx - 1..=cx + 1 {
            for gy in cy - 1..=cy + 1 {
                let Some(cell) = self.grid.get(&(gx, gy)) else {
                    continue;
                };
                for &t in cell {
                    let other = &self.tiles[t];
                    let near = (other.centre.0 - centre.0).hypot(other.centre.1 - centre.1);
                    if near < REACH && overlap(&corners, &other.corners) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Whether every gap in `mask` is a length the allowed kinds can fill. A gap
    /// of one sector never can, and with fewer kinds more lengths fail.
    fn gaps_fillable(&self, mask: u16) -> bool {
        gaps(mask).all(|(_, len)| self.fillable[len])
    }

    /// Adds a tile that `fits` already approved.
    fn place(&mut self, kind: PolygonKind, vertices: &[Point]) {
        let sectors = sectors_at(kind, vertices);
        let ids: Vec<usize> = vertices.iter().map(|&p| self.intern(p)).collect();
        for (&id, bits) in ids.iter().zip(&sectors) {
            self.set_mask(id, self.mask[id] | bits);
        }
        let corners: Vec<(f64, f64)> = vertices.iter().map(|p| p.to_f64()).collect();
        let centre = centre_of(&corners);
        self.grid
            .entry(cell_of(centre))
            .or_default()
            .push(self.tiles.len());
        for i in 0..vertices.len() {
            self.edges
                .insert((vertices[i], vertices[(i + 1) % vertices.len()]), kind);
        }
        self.counts[KINDS.iter().position(|&k| k == kind).unwrap_or(0)] += 1;
        self.tiles.push(Placed {
            kind,
            vertices: ids,
            sectors,
            corners,
            centre,
        });
    }

    /// Removes the last tile placed.
    fn undo(&mut self) {
        let tile = self.tiles.pop().expect("undo is only called with tiles");
        self.counts[KINDS.iter().position(|&k| k == tile.kind).unwrap_or(0)] -= 1;
        for i in 0..tile.vertices.len() {
            let (from, to) = (
                tile.vertices[i],
                tile.vertices[(i + 1) % tile.vertices.len()],
            );
            self.edges.remove(&(self.points[from], self.points[to]));
        }
        let number = self.tiles.len();
        if let Some(cell) = self.grid.get_mut(&cell_of(tile.centre)) {
            cell.retain(|&t| t != number);
        }
        for (&id, bits) in tile.vertices.iter().zip(&tile.sectors) {
            self.set_mask(id, self.mask[id] & !bits);
        }
    }

    /// The id of `p`, adding it as an empty vertex the first time.
    fn intern(&mut self, p: Point) -> usize {
        if let Some(&id) = self.ids.get(&p) {
            return id;
        }
        let id = self.points.len();
        let (x, y) = p.to_f64();
        self.ids.insert(p, id);
        self.points.push(p);
        self.xy.push((x, y));
        self.mask.push(0);
        self.in_range
            .push(self.outline.contains(x + MARGIN, y + MARGIN));
        id
    }

    /// Changes a vertex's mask and keeps the frontier in step with it.
    fn set_mask(&mut self, id: usize, mask: u16) {
        self.frontier.remove(&self.key(id));
        self.mask[id] = mask;
        if self.in_range[id] && mask != 0 && mask != FULL {
            self.frontier.insert(self.key(id));
        }
    }

    /// The frontier entry for a vertex. The distance is rounded to millionths so
    /// the key can be an integer and sort without comparing floats.
    fn key(&self, id: usize) -> (u8, u64, usize) {
        let filled = self.mask[id].count_ones() as u8;
        let (x, y) = self.xy[id];
        let distance = (x - self.centre.0).hypot(y - self.centre.1);
        // Most filled first, so the corner with the least left to decide goes first.
        (12 - filled, (distance * 1e6) as u64, id)
    }
}

/// The vertices of the tile that starts at `from` and runs anticlockwise with
/// its first edge pointing `first`, as `Prototile::vertices` does.
fn walk(kind: PolygonKind, from: Point, first: Direction) -> Vec<Point> {
    let mut vertices = Vec::with_capacity(kind.sides());
    let (mut p, mut d) = (from, first);
    for _ in 0..kind.sides() {
        vertices.push(p);
        p = p + d.unit();
        d = d.turn(kind.turn_steps());
    }
    vertices
}

/// The sectors a tile fills at each of its corners. Its interior lies to the
/// left of each edge, so at a corner it fills the sectors anticlockwise from
/// the outgoing edge.
fn sectors_at(kind: PolygonKind, vertices: &[Point]) -> Vec<u16> {
    let fill = (1u16 << kind.interior_steps()) - 1;
    (0..vertices.len())
        .map(|i| {
            let out = vertices[(i + 1) % vertices.len()] - vertices[i];
            // A tile's edges are unit vectors by construction.
            let steps = out.as_unit_direction().map_or(0, |d| u32::from(d.steps()));
            let turned = u32::from(fill) << steps;
            ((turned | turned >> 12) & u32::from(FULL)) as u16
        })
        .collect()
}

/// The runs of empty sectors in `mask` as `(first sector, length)`, going
/// round the vertex. A vertex with nothing filled has one run of twelve.
fn gaps(mask: u16) -> impl Iterator<Item = (usize, usize)> {
    let empty = move |s: usize| mask >> (s % 12) & 1 == 0;
    let whole = mask == 0;
    (0..12).filter_map(move |s| {
        if whole {
            return (s == 0).then_some((0, 12));
        }
        // A run starts where an empty sector follows a filled one.
        if !empty(s) || empty(s + 11) {
            return None;
        }
        let len = (1..12).take_while(|&n| empty(s + n)).count() + 1;
        Some((s, len))
    })
}

/// The shortest gap at a vertex, the first one if several tie. The vertex must
/// not be full.
fn smallest_gap(mask: u16) -> (usize, usize) {
    gaps(mask)
        .min_by(|a, b| match a.1.cmp(&b.1) {
            Ordering::Equal => a.0.cmp(&b.0),
            other => other,
        })
        // The frontier holds only vertices with an empty sector, so there is a gap.
        .expect("a frontier vertex has a gap")
}

fn centre_of(corners: &[(f64, f64)]) -> (f64, f64) {
    let n = corners.len() as f64;
    let (sx, sy) = corners
        .iter()
        .fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x, sy + y));
    (sx / n, sy / n)
}

/// The cell of the coarse grid a centre falls in. Cells are `REACH` wide, so
/// every tile that can overlap a new one has its centre in a neighbouring cell.
fn cell_of((x, y): (f64, f64)) -> (i32, i32) {
    ((x / REACH).floor() as i32, (y / REACH).floor() as i32)
}

/// Whether two convex polygons, both anticlockwise, overlap. Tiles that only
/// touch along an edge or at a corner do not.
fn overlap(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    !(separated(a, b) || separated(b, a))
}

/// Whether some edge of `a` has all of `b` on its outer side.
fn separated(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    (0..a.len()).any(|i| {
        let (x0, y0) = a[i];
        let (x1, y1) = a[(i + 1) % a.len()];
        // The outward normal of an anticlockwise edge; the edge has length one.
        let (nx, ny) = (y1 - y0, x0 - x1);
        let edge = nx * x0 + ny * y0;
        b.iter().all(|&(x, y)| nx * x + ny * y >= edge - TOLERANCE)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiling::checks::assert_valid;
    use crate::tiling::Shape;

    const REGION: Region = Region::rectangle(16.0, 12.0);

    /// The weight settings the tests cover, with a label.
    const MIXES: [[f64; 3]; 6] = [
        [1.0, 1.0, 1.0],
        [3.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [3.0, 1.0, 1.0],
    ];

    fn build(weights: [f64; 3], seed: u64) -> Tiling {
        RandomMix { weights }.build(REGION, &mut Rng::new(seed))
    }

    fn count(tiling: &Tiling, kind: PolygonKind) -> usize {
        tiling.faces.iter().filter(|f| f.kind == kind).count()
    }

    #[test]
    fn grown_tilings_are_valid() {
        for weights in MIXES {
            for seed in 0..8 {
                let tiling = build(weights, seed);
                assert_valid(&tiling, &format!("{weights:?} seed {seed}"));
                assert!(tiling.faces.len() > 20, "{weights:?} seed {seed}");
            }
        }
        for shape in [Shape::Hexagon, Shape::Circle] {
            let region = REGION.with_shape(shape);
            let tiling = RandomMix::default().build(region, &mut Rng::new(3));
            assert_valid(&tiling, &format!("{shape:?}"));
        }
    }

    /// Whether `(x, y)` is strictly inside the convex polygon `face`.
    fn covers(tiling: &Tiling, face: usize, (x, y): (f64, f64)) -> bool {
        let vertices = &tiling.faces[face].vertices;
        (0..vertices.len()).all(|i| {
            let (ax, ay) = tiling.points[vertices[i]].to_f64();
            let (bx, by) = tiling.points[vertices[(i + 1) % vertices.len()]].to_f64();
            (bx - ax) * (y - ay) - (by - ay) * (x - ax) > 0.0
        })
    }

    #[test]
    fn every_point_inside_is_covered_exactly_once() {
        // A gap would leave a sample uncovered and an overlap would cover it
        // twice. The samples keep clear of the ragged outline and of edges.
        for weights in MIXES {
            for seed in 0..4 {
                let tiling = build(weights, seed);
                for i in 0..40 {
                    for j in 0..25 {
                        let p = (
                            3.0371 + 0.2713 * f64::from(i),
                            3.0173 + 0.2917 * f64::from(j),
                        );
                        let n = (0..tiling.faces.len())
                            .filter(|&f| covers(&tiling, f, p))
                            .count();
                        assert_eq!(n, 1, "{weights:?} seed {seed} at {p:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_tiling() {
        for weights in [[1.0, 1.0, 1.0], [3.0, 1.0, 0.0]] {
            let (a, b, c) = (build(weights, 7), build(weights, 7), build(weights, 8));
            assert_eq!(a.points, b.points);
            for (x, y) in a.faces.iter().zip(&b.faces) {
                assert_eq!((x.kind, &x.vertices), (y.kind, &y.vertices));
            }
            assert_ne!(a.points, c.points);
        }
    }

    #[test]
    fn the_weights_matter() {
        let (mut triangles, mut squares) = (0, 0);
        for seed in 0..8 {
            let tiling = build([3.0, 1.0, 1.0], seed);
            triangles += count(&tiling, PolygonKind::Triangle);
            squares += count(&tiling, PolygonKind::Square);
        }
        assert!(
            triangles > squares,
            "{triangles} triangles, {squares} squares"
        );

        let tiling = build([0.0, 1.0, 0.0], 1);
        assert_eq!(count(&tiling, PolygonKind::Triangle), 0);
        assert_eq!(count(&tiling, PolygonKind::Hexagon), 0);
        assert!(count(&tiling, PolygonKind::Square) > 20);

        let tiling = build([1.0, 0.0, 0.0], 1);
        assert_eq!(count(&tiling, PolygonKind::Square), 0);
        assert_eq!(count(&tiling, PolygonKind::Hexagon), 0);

        let tiling = build([0.0, 1.0, 1.0], 1);
        assert_eq!(count(&tiling, PolygonKind::Triangle), 0);
        assert!(count(&tiling, PolygonKind::Hexagon) > 5);
    }

    #[test]
    fn an_even_mix_has_all_three_shapes() {
        for seed in 0..10 {
            let tiling = build([1.0, 1.0, 1.0], seed);
            for kind in KINDS {
                assert!(count(&tiling, kind) > 5, "seed {seed} {kind:?}");
            }
        }
    }

    #[test]
    fn it_is_not_rows_or_a_lattice() {
        // Rows put every square in a horizontal strip. In a free mix, squares
        // sit at many different heights even within one column of the region.
        let tiling = build([1.0, 1.0, 1.0], 2);
        let mut heights: Vec<i64> = (0..tiling.faces.len())
            .filter(|&f| tiling.faces[f].kind == PolygonKind::Square)
            .map(|f| (tiling.centre(f).1 * 100.0).round() as i64)
            .collect();
        heights.sort_unstable();
        heights.dedup();
        assert!(heights.len() > 12, "{} distinct heights", heights.len());
    }

    #[test]
    fn backing_up_is_rare_and_the_cap_is_not_hit() {
        let (mut backtracks, mut steps) = (0, 0);
        for weights in MIXES {
            for seed in 0..10 {
                let (_, stats) = RandomMix { weights }.grow(REGION, &mut Rng::new(seed));
                assert!(!stats.capped, "{weights:?} seed {seed}: {stats:?}");
                backtracks += stats.backtracks;
                steps += stats.steps;
            }
        }
        assert!(
            backtracks * 10 < steps,
            "{backtracks} backtracks in {steps} steps"
        );
    }

    #[test]
    fn a_big_region_builds_quickly() {
        let start = std::time::Instant::now();
        let tiling = RandomMix::default().build(Region::rectangle(30.0, 20.0), &mut Rng::new(1));
        assert_valid(&tiling, "30x20");
        // Debug builds are several times slower than release; this only catches
        // growth that has gone quadratic or worse.
        assert!(start.elapsed().as_secs_f64() < 5.0, "{:?}", start.elapsed());
    }

    #[test]
    fn gaps_are_found_round_the_vertex() {
        assert_eq!(gaps(0).collect::<Vec<_>>(), [(0, 12)]);
        assert_eq!(gaps(FULL).count(), 0);
        // Sectors 10, 11 and 0 filled leave one gap from 1 to 9.
        assert_eq!(gaps(0b1100_0000_0001).collect::<Vec<_>>(), [(1, 9)]);
        // Two gaps: sectors 2..=3 and 7.
        let mask = !(0b1100u16 | 1 << 7) & FULL;
        assert_eq!(gaps(mask).collect::<Vec<_>>(), [(2, 2), (7, 1)]);
    }

    #[test]
    fn a_tile_fills_the_sectors_left_of_each_edge() {
        // A unit square with its first edge pointing east fills the quarter
        // turn anticlockwise from east at its first corner.
        let square = walk(PolygonKind::Square, Point::ORIGIN, Direction::new(0));
        assert_eq!(sectors_at(PolygonKind::Square, &square)[0], 0b111);
        // The next corner's edge points north (sector 3), so it fills 3..=5.
        assert_eq!(sectors_at(PolygonKind::Square, &square)[1], 0b111_000);
    }

    #[test]
    fn touching_tiles_do_not_overlap_but_stacked_ones_do() {
        let a = walk(PolygonKind::Square, Point::ORIGIN, Direction::new(0));
        let b = walk(
            PolygonKind::Square,
            Point::new(Q3::new(2, 0), Q3::ZERO),
            Direction::new(0),
        );
        let c = walk(
            PolygonKind::Square,
            Point::new(Q3::new(1, 0), Q3::ZERO),
            Direction::new(0),
        );
        let xy = |v: &[Point]| v.iter().map(|p| p.to_f64()).collect::<Vec<_>>();
        assert!(!overlap(&xy(&a), &xy(&b)));
        assert!(overlap(&xy(&a), &xy(&c)));
    }
}
