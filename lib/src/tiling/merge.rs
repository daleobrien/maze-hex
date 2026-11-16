//! Free mix of triangles and hexagons (docs/design.md §2.1, item 1): start from the
//! triangular tiling and, in random order, merge the six triangles round a
//! vertex into one hexagon. Every hexagon is made of real triangles, so the
//! result is always valid and exact.

use std::collections::HashMap;

use super::{catalogue, source::TilingSource, PolygonKind, Region, Tiling};
use crate::geometry::Point;
use crate::rng::Rng;

/// Merges the six triangles round some vertices of `tiling` into hexagons.
///
/// Every vertex with exactly six triangles round it is a candidate. They are
/// visited in random order, and each is merged with probability `fraction`
/// unless one of its triangles already belongs to a hexagon. So `0.0` changes
/// nothing, and `1.0` merges as many as can fit.
pub fn merge_hexagons(tiling: &Tiling, fraction: f64, rng: &mut Rng) -> Tiling {
    // The triangles at each vertex, with the edge of each that faces away from it.
    let mut round: Vec<Vec<(usize, [usize; 2])>> = vec![Vec::new(); tiling.points.len()];
    for (f, face) in tiling.faces.iter().enumerate() {
        if face.kind != PolygonKind::Triangle {
            continue;
        }
        for i in 0..3 {
            // Anticlockwise, so the far edge runs from the next vertex to the one after.
            let far = [face.vertices[(i + 1) % 3], face.vertices[(i + 2) % 3]];
            round[face.vertices[i]].push((f, far));
        }
    }
    let mut candidates: Vec<usize> = (0..round.len()).filter(|&v| round[v].len() == 6).collect();
    // The order of vertex ids is fixed, so shuffling with a seeded rng is too.
    rng.shuffle(&mut candidates);

    let mut merged = vec![false; tiling.faces.len()];
    let mut hexagons: Vec<(PolygonKind, Vec<Point>)> = Vec::new();
    for v in candidates {
        if round[v].iter().any(|&(f, _)| merged[f]) || !rng.chance(fraction) {
            continue;
        }
        // Follow the far edges round: each ends where the next one starts.
        let next: HashMap<usize, usize> = round[v].iter().map(|&(_, [a, b])| (a, b)).collect();
        let first = round[v][0].1[0];
        let mut ring = vec![first];
        let mut at = next[&first];
        while at != first {
            ring.push(at);
            at = next[&at];
        }
        // Six triangles round an interior vertex always close into one ring of six.
        debug_assert_eq!(ring.len(), 6);
        for &(f, _) in &round[v] {
            merged[f] = true;
        }
        hexagons.push((
            PolygonKind::Hexagon,
            ring.iter().map(|&p| tiling.points[p]).collect(),
        ));
    }

    let mut faces: Vec<_> = tiling
        .polygons()
        .into_iter()
        .zip(&merged)
        .filter(|(_, &gone)| !gone)
        .map(|(polygon, _)| polygon)
        .collect();
    faces.extend(hexagons);
    // Smoothing was already done on the triangles, and merging only joins
    // neighbours, so it would find nothing to remove.
    Tiling::from_faces(&faces, false)
}

/// The triangular tiling with some triangle sextets merged into hexagons.
#[derive(Clone, Copy, Debug)]
pub struct TriangleHexagon {
    /// How many merges to try, from `0.0` (all triangles) to `1.0`.
    pub hexagons: f64,
}

impl Default for TriangleHexagon {
    fn default() -> Self {
        TriangleHexagon { hexagons: 0.6 }
    }
}

impl TilingSource for TriangleHexagon {
    fn name(&self) -> &str {
        "triangle-hexagon"
    }

    fn build(&self, region: Region, rng: &mut Rng) -> Tiling {
        let triangles = Tiling::build(&catalogue::TRIANGULAR, region);
        merge_hexagons(&triangles, self.hexagons, rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiling::checks::assert_valid;
    use crate::tiling::Shape;

    const REGION: Region = Region::rectangle(16.0, 12.0);

    fn count(tiling: &Tiling, kind: PolygonKind) -> usize {
        tiling.faces.iter().filter(|f| f.kind == kind).count()
    }

    fn build(hexagons: f64, seed: u64) -> Tiling {
        TriangleHexagon { hexagons }.build(REGION, &mut Rng::new(seed))
    }

    #[test]
    fn merged_tilings_are_valid_for_many_seeds_and_shapes() {
        for seed in 0..20 {
            for hexagons in [0.3, 0.6, 1.0] {
                let tiling = build(hexagons, seed);
                assert_valid(&tiling, &format!("seed {seed} fraction {hexagons}"));
            }
        }
        for shape in [Shape::Hexagon, Shape::Circle] {
            let region = REGION.with_shape(shape);
            let tiling = TriangleHexagon::default().build(region, &mut Rng::new(3));
            assert_valid(&tiling, &format!("{shape:?}"));
        }
    }

    #[test]
    fn hexagons_never_overlap() {
        // Every triangle of the base tiling is used by at most one hexagon, so
        // the areas must add up exactly: 6 triangles per hexagon, 1 per triangle.
        let base = Tiling::build(&catalogue::TRIANGULAR, REGION);
        for seed in 0..10 {
            let tiling = build(1.0, seed);
            let area =
                count(&tiling, PolygonKind::Triangle) + 6 * count(&tiling, PolygonKind::Hexagon);
            assert_eq!(area, base.faces.len(), "seed {seed}");
        }
    }

    #[test]
    fn fraction_zero_leaves_only_triangles() {
        let tiling = build(0.0, 1);
        assert_eq!(count(&tiling, PolygonKind::Hexagon), 0);
        assert_eq!(
            tiling.faces.len(),
            Tiling::build(&catalogue::TRIANGULAR, REGION).faces.len()
        );
    }

    #[test]
    fn fraction_one_makes_many_hexagons() {
        let tiling = build(1.0, 1);
        let hexagons = count(&tiling, PolygonKind::Hexagon);
        assert!(hexagons >= 8, "{hexagons} hexagons");
        assert!(count(&tiling, PolygonKind::Triangle) > 0);
        assert_eq!(count(&tiling, PolygonKind::Square), 0);
        assert!(hexagons > count(&build(0.3, 1), PolygonKind::Hexagon));
    }

    #[test]
    fn the_same_seed_gives_the_same_tiling() {
        let (a, b, c) = (build(0.6, 7), build(0.6, 7), build(0.6, 8));
        assert_eq!(a.points, b.points);
        assert_eq!(a.faces.len(), b.faces.len());
        for (x, y) in a.faces.iter().zip(&b.faces) {
            assert_eq!((x.kind, &x.vertices), (y.kind, &y.vertices));
        }
        assert_ne!(a.points, c.points);
    }
}
