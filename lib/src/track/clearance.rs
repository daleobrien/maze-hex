//! Which tile states can be drawn cleanly (PLAN §5.4).
//!
//! A tile state says, for each side, which block (group of joined sides) the
//! side belongs to, or that it is closed. A state is allowed when its blocks
//! don't cross, every wall stays inside the tile by `margin` (except at its own
//! two sides), and walls are at least `gap` apart. Because every tile only ever
//! draws inside itself, tiles never need to be checked against each other and
//! there is no drawing order.
//!
//! Tiles of one shape are all the same regular polygon, so the answer depends
//! only on the shape and the state; it is worked out once and cached.

use std::collections::HashMap;

use super::curve::{distance_to_polyline, V2};
use super::face::FaceGeometry;
use super::TrackStyle;
use crate::tiling::PolygonKind;

/// How finely walls are sampled for the check.
const SAMPLE_STEP: f64 = 0.01;

/// The blocks of a state: each block's sides, anticlockwise. `labels[i]` is
/// side `i`'s block (any non-zero number) or 0 if the side is closed.
pub fn blocks_of(labels: &[u8]) -> Vec<Vec<usize>> {
    let mut order: Vec<u8> = Vec::new();
    for &label in labels {
        if label != 0 && !order.contains(&label) {
            order.push(label);
        }
    }
    order
        .iter()
        .map(|&label| (0..labels.len()).filter(|&i| labels[i] == label).collect())
        .collect()
}

/// The same state with its blocks numbered 1, 2, ... in order of first side,
/// so equal states compare equal.
pub fn canonical(labels: &[u8]) -> Vec<u8> {
    let mut map: Vec<(u8, u8)> = Vec::new();
    labels
        .iter()
        .map(|&label| {
            if label == 0 {
                return 0;
            }
            if let Some(&(_, new)) = map.iter().find(|(old, _)| *old == label) {
                return new;
            }
            let new = map.len() as u8 + 1;
            map.push((label, new));
            new
        })
        .collect()
}

/// Whether two blocks cross: sides `a < b < c < d` with `a, c` in one block
/// and `b, d` in another.
pub fn is_crossing(labels: &[u8]) -> bool {
    let n = labels.len();
    for a in 0..n {
        for b in a + 1..n {
            for c in b + 1..n {
                for d in c + 1..n {
                    let (la, lb) = (labels[a], labels[b]);
                    if la != 0 && lb != 0 && la != lb && labels[c] == la && labels[d] == lb {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// The regular polygon of `kind` with edges of length 1.
pub fn regular(kind: PolygonKind) -> FaceGeometry {
    let n = kind.sides();
    let mut vertices = Vec::with_capacity(n);
    let mut p = V2::new(0.0, 0.0);
    for i in 0..n {
        vertices.push(p);
        p = p.polar(1.0, 2.0 * std::f64::consts::PI * i as f64 / n as f64);
    }
    FaceGeometry::from_vertices(vertices)
}

/// The cached answers to "can this state be drawn?".
#[derive(Debug)]
pub struct Clearance {
    style: TrackStyle,
    cache: HashMap<(PolygonKind, Vec<u8>), bool>,
}

impl Clearance {
    pub fn new(style: TrackStyle) -> Self {
        Clearance {
            style,
            cache: HashMap::new(),
        }
    }

    pub fn style(&self) -> TrackStyle {
        self.style
    }

    /// Whether a tile of `kind` in state `labels` draws cleanly.
    pub fn allows(&mut self, kind: PolygonKind, labels: &[u8]) -> bool {
        let key = (kind, canonical(labels));
        if let Some(&answer) = self.cache.get(&key) {
            return answer;
        }
        let answer = !is_crossing(&key.1) && self.draws_cleanly(kind, &key.1);
        self.cache.insert(key, answer);
        answer
    }

    fn draws_cleanly(&self, kind: PolygonKind, labels: &[u8]) -> bool {
        let face = regular(kind);
        let style = self.style;
        let walls = face.walls(&blocks_of(labels), &style);
        let samples: Vec<Vec<V2>> = walls
            .iter()
            .map(|wall| {
                wall.curves
                    .iter()
                    .flat_map(|c| c.sample(SAMPLE_STEP))
                    .collect()
            })
            .collect();

        // Inside the tile, clear of every side the wall doesn't cross.
        for (wall, points) in walls.iter().zip(&samples) {
            for side in 0..face.sides() {
                if side == wall.from_side || side == wall.to_side {
                    continue;
                }
                if points
                    .iter()
                    .any(|&p| face.inside_distance(p, side) < style.margin)
                {
                    return false;
                }
            }
        }
        // Clear of each other.
        for i in 0..samples.len() {
            for j in i + 1..samples.len() {
                if samples[i]
                    .iter()
                    .any(|&p| distance_to_polyline(p, &samples[j]) < style.gap)
                {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::TrackStyle;
    use PolygonKind::{Hexagon, Square, Triangle};

    /// Every state of an `n`-sided tile: each side closed or in a block.
    fn all_states(n: usize) -> Vec<Vec<u8>> {
        let mut states = vec![vec![]];
        for _ in 0..n {
            let mut next = Vec::new();
            for state in &states {
                for label in 0..=n as u8 {
                    let mut s: Vec<u8> = state.clone();
                    s.push(label);
                    next.push(s);
                }
            }
            states = next;
        }
        let mut unique: Vec<Vec<u8>> = states.iter().map(|s| canonical(s)).collect();
        unique.sort();
        unique.dedup();
        unique.retain(|s| !is_crossing(s));
        unique
    }

    #[test]
    fn the_numbers_of_non_crossing_states_match_the_plan() {
        assert_eq!(all_states(3).len(), 15);
        assert_eq!(all_states(4).len(), 51);
        assert_eq!(all_states(6).len(), 731);
    }

    #[test]
    fn crossing_is_detected() {
        assert!(is_crossing(&[1, 2, 1, 2, 0, 0]));
        assert!(!is_crossing(&[1, 1, 0, 2, 2, 0]));
        assert!(!is_crossing(&[1, 2, 2, 1, 0, 0]));
        assert!(is_crossing(&[1, 0, 2, 1, 0, 2]));
    }

    #[test]
    fn every_single_block_state_is_allowed() {
        // Hub-style tiles (one block, any sides) must always draw.
        let mut clearance = Clearance::new(TrackStyle::default());
        for kind in [Triangle, Square, Hexagon] {
            let n = kind.sides();
            for mask in 1u32..(1 << n) {
                let labels: Vec<u8> = (0..n).map(|i| ((mask >> i) & 1) as u8).collect();
                assert!(clearance.allows(kind, &labels), "{kind:?} {labels:?}");
            }
        }
    }

    #[test]
    fn the_tight_cases_from_the_plan() {
        let mut clearance = Clearance::new(TrackStyle::default());
        // Two wide arcs back to back: too tight at width 0.4.
        assert!(!clearance.allows(Hexagon, &[1, 0, 1, 2, 0, 2]));
        // A straight with a corner arc beside it: fits.
        assert!(clearance.allows(Hexagon, &[1, 2, 2, 1, 0, 0]));
        // Corners at opposite ends: fits.
        assert!(clearance.allows(Hexagon, &[1, 1, 0, 2, 2, 0]));
        // Three corners: fits.
        assert!(clearance.allows(Hexagon, &[1, 1, 2, 2, 3, 3]));
        // A dead end into a straight track: the cap hits it.
        assert!(!clearance.allows(Square, &[1, 2, 1, 0]));
    }

    #[test]
    fn report_how_many_states_are_allowed() {
        let mut clearance = Clearance::new(TrackStyle::default());
        for kind in [Triangle, Square, Hexagon] {
            let states = all_states(kind.sides());
            let allowed = states.iter().filter(|s| clearance.allows(kind, s)).count();
            // Multi-block states must be possible in hexagons, or there is no
            // pass-through look at all.
            if kind == Hexagon {
                assert!(allowed > 100, "{kind:?}: {allowed} of {}", states.len());
            }
            println!("{kind:?}: {allowed} of {} states allowed", states.len());
        }
    }
}
