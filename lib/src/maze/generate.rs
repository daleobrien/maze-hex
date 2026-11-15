//! The pass-through generator (PLAN §4.1): randomised Kruskal over chords.
//!
//! The nodes are ports (interior edges). A chord joins two sides of one tile.
//! A chord is accepted when its two ports are in different trees (so no loop
//! forms) and every tile it changes still draws cleanly: the tile itself, and
//! each neighbour that gains a dead-end cap because a port has just opened.
//! Every intermediate state is therefore drawable, and the result is a forest.
//! The largest tree is kept; any others are cleared, leaving blank tiles.

use crate::rng::Rng;
use crate::tiling::Tiling;
use crate::track::clearance::Clearance;

/// Settings for the generator.
#[derive(Clone, Copy, Debug)]
pub struct GenerateOptions {
    /// How often a tile that already holds a track may take a second, separate
    /// one (0 never, 1 whenever it fits).
    pub pass_through: f64,
    /// How strongly to prefer going straight through a tile (0 no preference).
    pub straightness: f64,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        GenerateOptions {
            pass_through: 0.7,
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
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
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

    fn try_chord(
        &mut self,
        chord: &Chord,
        uf: &mut UnionFind,
        rng: &mut Rng,
        options: &GenerateOptions,
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

        let mut opened = self.open.clone();
        opened[ea] = true;
        opened[eb] = true;
        let open = |e: usize| opened[e];
        let kind = face.kind;
        let labels = self.labels(chord.face, &open, &chords);
        if !self.clearance.allows(kind, &labels) {
            return false;
        }
        // Neighbours that gain a cap where a port has just opened.
        for edge in [ea, eb] {
            if self.open[edge] {
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

    // Later sweeps pick up chords that only fit once their tile has changed.
    let mut pending: Vec<Chord> = chords.into_iter().map(|(_, c)| c).collect();
    for _ in 0..4 {
        let before = pending.len();
        pending.retain(|chord| !state.try_chord(chord, &mut uf, rng, options));
        if pending.len() == before {
            break;
        }
    }

    keep_largest_tree(&mut state, &mut uf);
    Generated {
        labels: (0..tiling.faces.len())
            .map(|f| state.final_labels(f))
            .collect(),
    }
}

/// Clears every tree but the largest, so the maze is one piece.
fn keep_largest_tree(state: &mut State, uf: &mut UnionFind) {
    let edges = state.open.len();
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
    let tiling = state.tiling;
    for (f, face) in tiling.faces.iter().enumerate() {
        for (side, &edge) in face.edges.iter().enumerate() {
            if !state.open[edge] {
                state.chords[f][side] = 0;
            }
        }
    }
}
