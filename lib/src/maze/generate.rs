//! The pass-through generator (docs/design.md §4.1): randomised Kruskal over chords.
//!
//! The nodes are ports (interior edges). A chord joins two sides of one tile.
//! A chord is accepted when its two ports are in different trees (so no loop
//! forms) and every tile it changes still draws cleanly: the tile itself, and
//! each neighbour that gains a dead-end cap because a port has just opened.
//! Every intermediate state is therefore drawable, and the result is a forest.
//! The largest tree is kept; any others are cleared, leaving blank tiles.

use super::hub::Algorithm;
use crate::rng::Rng;
use crate::tiling::Tiling;
use crate::track::clearance::{is_crossing, Clearance};

/// How tracks may share a tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// A tile may hold several separate tracks (docs/design.md §4.1).
    #[default]
    PassThrough,
    /// A tile holds one block of all its open sides (docs/design.md §4.2).
    Hub,
}

/// Settings for the generator.
#[derive(Clone, Copy, Debug)]
pub struct GenerateOptions {
    pub mode: Mode,
    /// Which spanning-tree algorithm hub mode uses; pass-through has its own.
    pub algorithm: Algorithm,
    /// How often a tile that already holds a track may take a second, separate
    /// one (0 never, 1 whenever it fits).
    pub pass_through: f64,
    /// How often a tile may take a track that crosses the one already there,
    /// as a bridge (0 never, 1 whenever it fits). Only tiles of four or more
    /// sides have room.
    pub crossings: f64,
    /// How readily tracks join into junctions, 0 to 1. Below 1, the first
    /// sweeps keep a junction of `n` sides with probability `junctions^(n-2)`,
    /// favouring bridges and through-tracks over hubs of many sides.
    pub junctions: f64,
    /// How strongly to prefer going straight through a tile (0 no preference).
    pub straightness: f64,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        GenerateOptions {
            mode: Mode::PassThrough,
            algorithm: Algorithm::Wilson,
            pass_through: 0.7,
            crossings: 0.8,
            junctions: 0.1,
            straightness: 0.3,
        }
    }
}

/// The result: for each face, a label per side (0 closed, otherwise a block),
/// with dead-end caps already given their own blocks.
pub(super) struct Generated {
    pub labels: Vec<Vec<u8>>,
}

/// A disjoint-set forest over ports.
pub(super) struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    pub(super) fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }

    pub(super) fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    pub(super) fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        self.parent[ra] = rb;
    }
}

struct Chord {
    face: usize,
    a: usize,
    b: usize,
}

struct State<'a> {
    tiling: &'a Tiling,
    clearance: &'a mut Clearance,
    /// Per face and side: the chord block the side is in (0 if none).
    chords: Vec<Vec<u8>>,
    /// Per edge: whether a track crosses it.
    open: Vec<bool>,
}

impl State<'_> {
    /// The full labels of a face: chord blocks, plus a cap for each open side
    /// that no chord in this face uses.
    fn labels(&self, face: usize, open: &dyn Fn(usize) -> bool, chords: &[u8]) -> Vec<u8> {
        let mut next = chords.iter().copied().max().unwrap_or(0);
        let edges = &self.tiling.faces[face].edges;
        chords
            .iter()
            .zip(edges)
            .map(|(&label, &edge)| {
                if label != 0 {
                    label
                } else if open(edge) {
                    next += 1;
                    next
                } else {
                    0
                }
            })
            .collect()
    }

    fn final_labels(&self, face: usize) -> Vec<u8> {
        self.labels(face, &|e| self.open[e], &self.chords[face])
    }

    /// How many blocks of two or more sides a face's chords make.
    fn tracks(chords: &[u8]) -> usize {
        let mut seen: Vec<u8> = chords.iter().copied().filter(|&l| l != 0).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }

    /// Tries to add `chord`. With `check_neighbours`, a chord is refused if a
    /// neighbour couldn't draw the dead end it would gain; without, such
    /// neighbours are left for [`State::repair`].
    fn try_chord(
        &mut self,
        chord: &Chord,
        uf: &mut UnionFind,
        rng: &mut Rng,
        options: &GenerateOptions,
        check_neighbours: bool,
    ) -> bool {
        let tiling = self.tiling;
        let face = &tiling.faces[chord.face];
        let (ea, eb) = (face.edges[chord.a], face.edges[chord.b]);
        if uf.find(ea) == uf.find(eb) {
            return false;
        }

        // The face's chord labels after joining sides a and b.
        let mut chords = self.chords[chord.face].clone();
        let (la, lb) = (chords[chord.a], chords[chord.b]);
        match (la, lb) {
            (0, 0) => {
                let fresh = chords.iter().copied().max().unwrap_or(0) + 1;
                chords[chord.a] = fresh;
                chords[chord.b] = fresh;
            }
            (0, l) => chords[chord.a] = l,
            (l, 0) => chords[chord.b] = l,
            (keep, gone) => {
                for label in chords.iter_mut() {
                    if *label == gone {
                        *label = keep;
                    }
                }
            }
        }

        // A second, separate track in this tile happens only sometimes.
        let before = Self::tracks(&self.chords[chord.face]);
        if Self::tracks(&chords) > before && before >= 1 && !rng.chance(options.pass_through) {
            return false;
        }

        let was_open = &self.open;
        let open = |e: usize| was_open[e] || e == ea || e == eb;
        let kind = face.kind;
        let labels = self.labels(chord.face, &open, &chords);
        // Checked after pass-through and without touching the random numbers
        // when crossings are off, so that setting repeats older mazes exactly.
        if is_crossing(&labels) && (options.crossings <= 0.0 || !rng.chance(options.crossings)) {
            return false;
        }
        // In the first sweeps a chord that makes a junction (a block of three
        // or more sides) is kept only sometimes, and less often the bigger the
        // junction, so tracks pass through or cross instead of branching. The
        // later sweeps take any chord they need to finish the tree. At 1 this
        // draws no random numbers, which repeats older mazes exactly.
        if check_neighbours && options.junctions < 1.0 {
            let label = chords[chord.a];
            let size = chords.iter().filter(|&&l| l == label).count();
            if size >= 3 && !rng.chance(options.junctions.powi(size as i32 - 2)) {
                return false;
            }
        }
        if !self.clearance.allows(kind, &labels) {
            return false;
        }
        // Neighbours that gain a cap where a port has just opened.
        for edge in [ea, eb] {
            if !check_neighbours || self.open[edge] {
                continue;
            }
            if let Some(across) = tiling.edges[edge].across(chord.face) {
                let other = across.face;
                let labels = self.labels(other, &open, &self.chords[other]);
                if !self.clearance.allows(tiling.faces[other].kind, &labels) {
                    return false;
                }
            }
        }

        self.chords[chord.face] = chords;
        self.open[ea] = true;
        self.open[eb] = true;
        uf.union(ea, eb);
        true
    }
}

impl State<'_> {
    /// Before the sweeps, gives each tile of four or more sides a bridge with
    /// probability `crossings`: two through-tracks that cross, chosen at
    /// random from those that draw cleanly.
    ///
    /// Bridges are laid first because they can't easily appear later. Once
    /// the sweeps have joined a tile's ports into one tree, a crossing track
    /// would close a loop, and the tile is usually crowded with dead ends by
    /// then anyway. Laid first, each bridge just joins four separate ports into
    /// two pairs, and the sweeps grow the rest of the maze round it.
    fn lay_bridges(&mut self, uf: &mut UnionFind, rng: &mut Rng, crossings: f64) {
        let tiling = self.tiling;
        for (f, face) in tiling.faces.iter().enumerate() {
            let n = face.kind.sides();
            if n < 4 || !rng.chance(crossings) {
                continue;
            }
            let interior = |s: usize| tiling.edges[face.edges[s]].is_interior();
            // Every pair of crossing chords a < b < c < d: (a, c) and (b, d).
            let mut choices = Vec::new();
            for a in 0..n {
                for b in a + 1..n {
                    for c in b + 1..n {
                        for d in c + 1..n {
                            if [a, b, c, d].iter().all(|&s| interior(s)) {
                                choices.push([a, b, c, d]);
                            }
                        }
                    }
                }
            }
            rng.shuffle(&mut choices);
            for [a, b, c, d] in choices {
                let edges = [a, b, c, d].map(|s| face.edges[s]);
                if uf.find(edges[0]) == uf.find(edges[2]) || uf.find(edges[1]) == uf.find(edges[3])
                {
                    continue;
                }
                let mut chords = self.chords[f].clone();
                if [a, b, c, d].iter().any(|&s| chords[s] != 0) {
                    continue;
                }
                let fresh = chords.iter().copied().max().unwrap_or(0);
                chords[a] = fresh + 1;
                chords[c] = fresh + 1;
                chords[b] = fresh + 2;
                chords[d] = fresh + 2;
                let open = |e: usize| self.open[e] || edges.contains(&e);
                let labels = self.labels(f, &open, &chords);
                if self.clearance.allows(face.kind, &labels) {
                    self.chords[f] = chords;
                    for e in edges {
                        self.open[e] = true;
                    }
                    uf.union(edges[0], edges[2]);
                    uf.union(edges[1], edges[3]);
                    break;
                }
            }
        }
    }

    /// Makes every tile drawable again after the unchecked sweeps.
    ///
    /// A crowded tile's dead ends are first offered to its existing tracks:
    /// joining one in links two trees, if they are different trees. Any dead
    /// end that can't be joined is cut back: the neighbour's track stops short
    /// of the edge instead. Cutting a dead end never splits a tree.
    fn repair(&mut self, uf: &mut UnionFind) {
        let tiling = self.tiling;
        let mut changed = true;
        while changed {
            changed = false;
            for f in 0..tiling.faces.len() {
                let kind = tiling.faces[f].kind;
                if self.clearance.allows(kind, &self.final_labels(f)) {
                    continue;
                }
                let caps: Vec<usize> = (0..kind.sides())
                    .filter(|&s| self.chords[f][s] == 0 && self.open[tiling.faces[f].edges[s]])
                    .collect();
                for side in caps {
                    if self.clearance.allows(kind, &self.final_labels(f)) {
                        break;
                    }
                    if !self.join_cap(f, side, uf) {
                        self.cut_cap(f, side);
                    }
                    changed = true;
                }
            }
        }
    }

    /// Tries to join dead end `side` of face `f` into one of the face's
    /// tracks from another tree.
    fn join_cap(&mut self, f: usize, side: usize, uf: &mut UnionFind) -> bool {
        let face = &self.tiling.faces[f];
        let edge = face.edges[side];
        let mut labels: Vec<u8> = self.chords[f].iter().copied().filter(|&l| l != 0).collect();
        labels.sort_unstable();
        labels.dedup();
        for label in labels {
            let Some(member) = self.chords[f].iter().position(|&l| l == label) else {
                continue;
            };
            if uf.find(face.edges[member]) == uf.find(edge) {
                continue;
            }
            let mut chords = self.chords[f].clone();
            chords[side] = label;
            let labels = self.labels(f, &|e| self.open[e], &chords);
            if self.clearance.allows(face.kind, &labels) {
                self.chords[f] = chords;
                uf.union(edge, face.edges[member]);
                return true;
            }
        }
        false
    }

    /// Cuts back the track that pokes into face `f` through `side`.
    fn cut_cap(&mut self, f: usize, side: usize) {
        let tiling = self.tiling;
        let edge = tiling.faces[f].edges[side];
        self.open[edge] = false;
        let Some(across) = tiling.edges[edge].across(f) else {
            return;
        };
        let (h, t) = (across.face, across.side);
        let label = self.chords[h][t];
        self.chords[h][t] = 0;
        let rest: Vec<usize> = (0..self.chords[h].len())
            .filter(|&s| label != 0 && self.chords[h][s] == label)
            .collect();
        if let [only] = rest[..] {
            // A track of one side is no track: that side becomes a dead end
            // if a track still comes in from the other side, or closes.
            self.chords[h][only] = 0;
            let other_edge = tiling.faces[h].edges[only];
            let still_used = tiling.edges[other_edge]
                .across(h)
                .is_some_and(|o| self.chords[o.face][o.side] != 0);
            if !still_used {
                self.open[other_edge] = false;
            }
        }
    }
}

/// The weight of a chord: straight-through chords get more when
/// `straightness` is high.
fn weight(sides: usize, a: usize, b: usize, straightness: f64) -> f64 {
    let opposite = sides.is_multiple_of(2) && b - a == sides / 2;
    if opposite {
        1.0 + 4.0 * straightness
    } else {
        1.0
    }
}

pub(super) fn generate(
    tiling: &Tiling,
    clearance: &mut Clearance,
    rng: &mut Rng,
    options: &GenerateOptions,
) -> Generated {
    let mut chords = Vec::new();
    for (f, face) in tiling.faces.iter().enumerate() {
        let n = face.kind.sides();
        for a in 0..n {
            for b in a + 1..n {
                let interior = |s: usize| tiling.edges[face.edges[s]].is_interior();
                if interior(a) && interior(b) {
                    // A weighted shuffle: sort by u^(1/w), largest first.
                    let key = rng.unit().powf(1.0 / weight(n, a, b, options.straightness));
                    chords.push((key, Chord { face: f, a, b }));
                }
            }
        }
    }
    chords.sort_by(|x, y| y.0.total_cmp(&x.0));

    let mut state = State {
        tiling,
        clearance,
        chords: tiling
            .faces
            .iter()
            .map(|f| vec![0; f.kind.sides()])
            .collect(),
        open: vec![false; tiling.edges.len()],
    };
    let mut uf = UnionFind::new(tiling.edges.len());

    if options.crossings > 0.0 {
        state.lay_bridges(&mut uf, rng, options.crossings);
    }

    // The first sweeps keep every tile drawable as they go, which suits
    // hexagons. In triangles and squares that can deadlock: two neighbouring
    // tracks can never meet, because opening the edge between them first
    // gives one a dead end it has no room for. So the later sweeps ignore the
    // neighbours, and `repair` sorts out any tile left crowded.
    let mut pending: Vec<Chord> = chords.into_iter().map(|(_, c)| c).collect();
    for check_neighbours in [true, true, false, false, false] {
        let before = pending.len();
        pending.retain(|chord| !state.try_chord(chord, &mut uf, rng, options, check_neighbours));
        if pending.len() == before && !check_neighbours {
            break;
        }
    }
    state.repair(&mut uf);
    keep_largest_tree(&mut state);
    Generated {
        labels: (0..tiling.faces.len())
            .map(|f| state.final_labels(f))
            .collect(),
    }
}

/// Clears every tree but the largest, so the maze is one piece. The trees
/// are found afresh from the chords, because cutting dead ends in `repair`
/// can't be undone in a union-find.
fn keep_largest_tree(state: &mut State) {
    let tiling = state.tiling;
    let edges = state.open.len();
    let mut uf = UnionFind::new(edges);
    for (f, face) in tiling.faces.iter().enumerate() {
        let chords = &state.chords[f];
        for a in 0..chords.len() {
            for b in a + 1..chords.len() {
                if chords[a] != 0 && chords[a] == chords[b] {
                    uf.union(face.edges[a], face.edges[b]);
                }
            }
        }
    }
    let mut size = vec![0usize; edges];
    for e in 0..edges {
        if state.open[e] {
            size[uf.find(e)] += 1;
        }
    }
    let Some(best) = (0..edges).max_by_key(|&root| size[root]) else {
        return;
    };
    for e in 0..edges {
        if state.open[e] && uf.find(e) != best {
            state.open[e] = false;
        }
    }
    // Drop chord labels on sides whose ports were closed.
    for (f, face) in tiling.faces.iter().enumerate() {
        for (side, &edge) in face.edges.iter().enumerate() {
            if !state.open[edge] {
                state.chords[f][side] = 0;
            }
        }
    }
}
