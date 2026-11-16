//! Which tile states can be drawn cleanly (docs/design.md §5.4).
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
const SAMPLE_STEP: f64 = 0.02;

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
    /// Whether a tile may hold one bridge: two through-tracks crossing.
    crossings: bool,
    cache: HashMap<(PolygonKind, Vec<u8>), bool>,
}

impl Clearance {
    pub fn new(style: TrackStyle) -> Self {
        Clearance {
            style,
            crossings: false,
            cache: HashMap::new(),
        }
    }

    /// The same check, but letting a tile hold one bridge (see
    /// [`bridge_of`]).
    pub fn with_crossings(mut self, crossings: bool) -> Self {
        self.crossings = crossings;
        self.cache.clear();
        self
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
        let answer = if is_crossing(&key.1) {
            self.crossings
                && bridge_of(&key.1)
                    .is_some_and(|bridge| self.bridge_is_clear(kind, &key.1, bridge))
        } else {
            self.draws_cleanly(kind, &key.1, None)
        };
        self.cache.insert(key, answer);
        answer
    }

    /// Whether the two tracks of a bridge cross squarely enough to read as one
    /// passing over the other, well inside the tile, and everything else in
    /// the tile still draws cleanly.
    fn bridge_is_clear(&self, kind: PolygonKind, labels: &[u8], bridge: (usize, usize)) -> bool {
        let face = regular(kind);
        let blocks = blocks_of(labels);
        let line = |block: &Vec<usize>| face.centreline(block[0], block[1]).sample(SAMPLE_STEP);
        let (a, b) = (line(&blocks[bridge.0]), line(&blocks[bridge.1]));
        let Some((point, angle)) = crossing_point(&a, &b) else {
            return false;
        };
        let deep_enough =
            (0..face.sides()).all(|side| face.inside_distance(point, side) >= self.style.width);
        angle >= MIN_CROSSING_ANGLE && deep_enough && self.draws_cleanly(kind, labels, Some(bridge))
    }

    /// Checks walls against the tile and each other. The walls of the two
    /// tracks in `bridge` (block indices) are allowed to cross each other.
    fn draws_cleanly(
        &self,
        kind: PolygonKind,
        labels: &[u8],
        bridge: Option<(usize, usize)>,
    ) -> bool {
        let face = regular(kind);
        let style = self.style;
        let blocks = blocks_of(labels);
        let mut walls = Vec::new();
        let mut owner = Vec::new();
        for (k, block) in blocks.iter().enumerate() {
            for wall in face.walls(std::slice::from_ref(block), &style) {
                walls.push(wall);
                owner.push(k);
            }
        }
        let exempt = |i: usize, j: usize| {
            bridge.is_some_and(|(x, y)| {
                (owner[i], owner[j]) == (x, y) || (owner[i], owner[j]) == (y, x)
            })
        };
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
        // Clear of each other. Walls whose bounding boxes are already far
        // apart are skipped; this check is most of the cost of a new state.
        let boxes: Vec<(V2, V2)> = samples.iter().map(|points| bounds(points)).collect();
        for i in 0..samples.len() {
            for j in i + 1..samples.len() {
                if exempt(i, j) || box_gap(boxes[i], boxes[j]) >= style.gap {
                    continue;
                }
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

/// The least angle at which a bridge's two tracks may cross.
const MIN_CROSSING_ANGLE: f64 = 40.0 * std::f64::consts::PI / 180.0;

/// The two blocks (as indices into [`blocks_of`]) of a tile's one bridge: two
/// tracks of two sides each that cross. `None` if nothing crosses, or if the
/// crossing is anything else (more than one, or a junction involved), which
/// can't be drawn as one track passing over another.
pub fn bridge_of(labels: &[u8]) -> Option<(usize, usize)> {
    let labels = canonical(labels);
    let blocks = blocks_of(&labels);
    let mut pairs = Vec::new();
    for i in 0..blocks.len() {
        for j in i + 1..blocks.len() {
            if is_crossing(&pair_labels(labels.len(), &blocks[i], &blocks[j])) {
                pairs.push((i, j));
            }
        }
    }
    match pairs[..] {
        [(i, j)] if blocks[i].len() == 2 && blocks[j].len() == 2 => Some((i, j)),
        _ => None,
    }
}

/// Labels holding only two blocks, for testing whether that pair crosses.
fn pair_labels(n: usize, a: &[usize], b: &[usize]) -> Vec<u8> {
    let mut labels = vec![0; n];
    for &s in a {
        labels[s] = 1;
    }
    for &s in b {
        labels[s] = 2;
    }
    labels
}

/// Where two polylines cross, and the angle between them there (0 to 90°).
fn crossing_point(a: &[V2], b: &[V2]) -> Option<(V2, f64)> {
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            let (p, r) = (a[i], a[i + 1].minus(a[i]));
            let (q, s) = (b[j], b[j + 1].minus(b[j]));
            let denom = r.cross(s);
            if denom.abs() < 1e-12 {
                continue;
            }
            let t = q.minus(p).cross(s) / denom;
            let u = q.minus(p).cross(r) / denom;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                let cos = (r.dot(s) / (r.length() * s.length())).abs().min(1.0);
                return Some((p.plus(r.scale(t)), cos.acos()));
            }
        }
    }
    None
}

/// The smallest box holding every point.
fn bounds(points: &[V2]) -> (V2, V2) {
    points.iter().fold(
        (V2::new(f64::MAX, f64::MAX), V2::new(f64::MIN, f64::MIN)),
        |(lo, hi), p| {
            (
                V2::new(lo.x.min(p.x), lo.y.min(p.y)),
                V2::new(hi.x.max(p.x), hi.y.max(p.y)),
            )
        },
    )
}

/// How far apart two boxes are (0 if they overlap).
fn box_gap((alo, ahi): (V2, V2), (blo, bhi): (V2, V2)) -> f64 {
    let dx = (blo.x - ahi.x).max(alo.x - bhi.x).max(0.0);
    let dy = (blo.y - ahi.y).max(alo.y - bhi.y).max(0.0);
    dx.hypot(dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::TrackStyle;
    use PolygonKind::{Hexagon, Square, Triangle};

    /// Every state of an `n`-sided tile: each side closed or in a block.
    fn all_states(n: usize) -> Vec<Vec<u8>> {
        let mut states = all_states_with_crossings(n);
        states.retain(|s| !is_crossing(s));
        states
    }

    /// Every state, crossing or not.
    fn all_states_with_crossings(n: usize) -> Vec<Vec<u8>> {
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
        unique
    }

    #[test]
    fn the_numbers_of_non_crossing_states_match_the_design() {
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
    fn every_single_block_state_is_allowed_with_rings() {
        let mut clearance = Clearance::new(TrackStyle::default().with_circles());
        for kind in [Triangle, Square, Hexagon] {
            let n = kind.sides();
            for mask in 1u32..(1 << n) {
                let labels: Vec<u8> = (0..n).map(|i| ((mask >> i) & 1) as u8).collect();
                assert!(clearance.allows(kind, &labels), "{kind:?} {labels:?}");
            }
        }
    }

    #[test]
    fn the_tight_cases_from_the_design() {
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
    fn bridges_need_crossings_turned_on() {
        let straight_cross_square = [1, 2, 1, 2];
        let straight_cross_hexagon = [1, 2, 0, 1, 2, 0];
        let mut off = Clearance::new(TrackStyle::default());
        assert!(!off.allows(Square, &straight_cross_square));
        assert!(!off.allows(Hexagon, &straight_cross_hexagon));
        let mut on = Clearance::new(TrackStyle::default()).with_crossings(true);
        assert!(on.allows(Square, &straight_cross_square));
        assert!(on.allows(Hexagon, &straight_cross_hexagon));
    }

    #[test]
    fn only_two_through_tracks_can_make_a_bridge() {
        assert_eq!(bridge_of(&[1, 2, 1, 2]), Some((0, 1)));
        assert_eq!(bridge_of(&[1, 1, 0, 2, 2, 0]), None, "nothing crosses");
        // A junction crossing a track can't be drawn as a bridge.
        assert_eq!(bridge_of(&[1, 2, 1, 2, 0, 1]), None);
        // Two crossings in one tile are too many.
        assert_eq!(bridge_of(&[1, 2, 3, 1, 2, 3]), None);
        let mut on = Clearance::new(TrackStyle::default()).with_crossings(true);
        assert!(!on.allows(Hexagon, &[1, 2, 1, 2, 0, 1]));
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
            let mut bridges = Clearance::new(TrackStyle::default()).with_crossings(true);
            let with_bridges = all_states_with_crossings(kind.sides())
                .iter()
                .filter(|s| is_crossing(s) && bridges.allows(kind, s))
                .count();
            println!(
                "{kind:?}: {allowed} of {} states allowed, {with_bridges} with bridges",
                states.len()
            );
        }
    }
}
