//! Free mix of squares and triangles in horizontal rows (docs/design.md §2.1, item 2,
//! fallback form). Each row is either a strip of unit squares or a strip of
//! alternating up and down triangles, chosen at random, so squares, triangles
//! and (after merging) hexagons all appear in one maze.
//!
//! This is not the Stampfli square–triangle inflation (docs/design.md §6, Future work): that
//! needs five marked prototiles and patches that overlap their neighbours, and
//! the exact rules are only published as pictures. Rows are exact and always
//! valid, but the pattern is stripes, not a quasicrystal.

use super::{
    centre_is_inside, merge::merge_hexagons, source::TilingSource, PolygonKind, Region, Tiling,
};
use crate::geometry::{Point, Q3};
use crate::rng::Rng;

/// The height of a triangle row, √3/2.
const TRIANGLE_HEIGHT: Q3 = Q3::new(0, 1);
/// The height of a square row, 1.
const SQUARE_HEIGHT: Q3 = Q3::new(2, 0);

/// Fills `region` with random rows of squares and triangles.
///
/// A row is a strip of squares with probability `squares`, otherwise a strip
/// of triangles. Rows must meet corner to corner, or an edge would end in the
/// middle of another and the angles at that point would not add up to 360°. A
/// square row keeps the horizontal position of the row below. A triangle row
/// sits half a unit over, because its apexes land between its base corners.
/// So the only freedom left is which kind each row is, and where the first row
/// starts.
pub fn row_mix(region: Region, squares: f64, rng: &mut Rng) -> Tiling {
    let columns = region.width.ceil() as i64 + 2;
    // Twice the horizontal shift of the current row's lower corners, 0 or 1.
    let mut half = i64::from(rng.chance(0.5));
    let mut y = Q3::ZERO;
    let mut faces: Vec<(PolygonKind, Vec<Point>)> = Vec::new();
    while y.to_f64() < region.height + 1.0 {
        let is_squares = rng.chance(squares);
        for column in -2..=columns {
            let x = Q3::new(2 * column + half, 0);
            if is_squares {
                faces.push((PolygonKind::Square, square(x, y)));
            } else {
                faces.extend(triangles(x, y));
            }
        }
        if is_squares {
            y = y + SQUARE_HEIGHT;
        } else {
            y = y + TRIANGLE_HEIGHT;
            half = 1 - half;
        }
    }
    faces.retain(|(_, vertices)| centre_is_inside(vertices, region));
    Tiling::from_faces(&faces, region.smooth)
}

/// The unit square whose lower left corner is `(x, y)`, anticlockwise.
fn square(x: Q3, y: Q3) -> Vec<Point> {
    let one = Q3::new(2, 0);
    vec![
        Point::new(x, y),
        Point::new(x + one, y),
        Point::new(x + one, y + one),
        Point::new(x, y + one),
    ]
}

/// The up triangle on the base from `(x, y)` and the down triangle to its
/// right, anticlockwise. Together they are one cell of a triangle row.
fn triangles(x: Q3, y: Q3) -> [(PolygonKind, Vec<Point>); 2] {
    let (one, half) = (Q3::new(2, 0), Q3::new(1, 0));
    let top = y + TRIANGLE_HEIGHT;
    let up = vec![
        Point::new(x, y),
        Point::new(x + one, y),
        Point::new(x + half, top),
    ];
    let down = vec![
        Point::new(x + half, top),
        Point::new(x + one, y),
        Point::new(x + one + half, top),
    ];
    [(PolygonKind::Triangle, up), (PolygonKind::Triangle, down)]
}

/// Rows of squares and triangles.
#[derive(Clone, Copy, Debug)]
pub struct RowMix {
    /// The chance that a row is squares rather than triangles.
    pub squares: f64,
}

impl Default for RowMix {
    fn default() -> Self {
        RowMix { squares: 0.35 }
    }
}

impl TilingSource for RowMix {
    fn name(&self) -> &str {
        "row-mix"
    }

    fn build(&self, region: Region, rng: &mut Rng) -> Tiling {
        row_mix(region, self.squares, rng)
    }
}

/// Rows of squares and triangles, with the triangles merged into hexagons.
#[derive(Clone, Copy, Debug)]
pub struct RowMixHexagon {
    /// The chance that a row is squares rather than triangles.
    pub squares: f64,
    /// How many hexagon merges to try, from `0.0` to `1.0`.
    pub hexagons: f64,
}

impl Default for RowMixHexagon {
    fn default() -> Self {
        RowMixHexagon {
            squares: 0.35,
            hexagons: 0.6,
        }
    }
}

impl TilingSource for RowMixHexagon {
    fn name(&self) -> &str {
        "row-mix-hexagon"
    }

    fn build(&self, region: Region, rng: &mut Rng) -> Tiling {
        let rows = row_mix(region, self.squares, rng);
        merge_hexagons(&rows, self.hexagons, rng)
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

    fn build(squares: f64, hexagons: Option<f64>, seed: u64) -> Tiling {
        let mut rng = Rng::new(seed);
        match hexagons {
            None => RowMix { squares }.build(REGION, &mut rng),
            Some(hexagons) => RowMixHexagon { squares, hexagons }.build(REGION, &mut rng),
        }
    }

    #[test]
    fn rows_are_valid_for_many_seeds_and_shapes() {
        for seed in 0..20 {
            for hexagons in [None, Some(0.6), Some(1.0)] {
                let tiling = build(0.35, hexagons, seed);
                assert_valid(&tiling, &format!("seed {seed} {hexagons:?}"));
            }
        }
        for shape in [Shape::Hexagon, Shape::Circle] {
            let region = REGION.with_shape(shape);
            let tiling = RowMixHexagon::default().build(region, &mut Rng::new(3));
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
        // Gaps would leave a sample uncovered and overlaps would cover it
        // twice. The samples keep clear of the ragged outline and of edges.
        for seed in 0..6 {
            for hexagons in [None, Some(1.0)] {
                let tiling = build(0.4, hexagons, seed);
                for i in 0..40 {
                    for j in 0..25 {
                        let p = (3.0371 + 0.2713 * f64::from(i), 3.0 + 0.2917 * f64::from(j));
                        let n = (0..tiling.faces.len())
                            .filter(|&f| covers(&tiling, f, p))
                            .count();
                        assert_eq!(n, 1, "seed {seed} {hexagons:?} at {p:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn squares_and_triangles_both_appear() {
        for seed in 0..10 {
            let tiling = build(0.4, None, seed);
            assert!(count(&tiling, PolygonKind::Square) > 10, "seed {seed}");
            assert!(count(&tiling, PolygonKind::Triangle) > 10, "seed {seed}");
            assert_eq!(count(&tiling, PolygonKind::Hexagon), 0);
        }
    }

    #[test]
    fn the_merged_mix_has_all_three_shapes() {
        for seed in 0..10 {
            let tiling = build(0.4, Some(1.0), seed);
            for kind in [
                PolygonKind::Square,
                PolygonKind::Triangle,
                PolygonKind::Hexagon,
            ] {
                assert!(count(&tiling, kind) > 0, "seed {seed} {kind:?}");
            }
        }
    }

    #[test]
    fn the_extremes_are_pure() {
        let squares = build(1.0, None, 1);
        assert_eq!(count(&squares, PolygonKind::Triangle), 0);
        let triangles = build(0.0, None, 1);
        assert_eq!(count(&triangles, PolygonKind::Square), 0);
    }

    #[test]
    fn the_same_seed_gives_the_same_tiling() {
        for hexagons in [None, Some(0.6)] {
            let (a, b, c) = (
                build(0.4, hexagons, 7),
                build(0.4, hexagons, 7),
                build(0.4, hexagons, 8),
            );
            assert_eq!(a.points, b.points);
            for (x, y) in a.faces.iter().zip(&b.faces) {
                assert_eq!((x.kind, &x.vertices), (y.kind, &y.vertices));
            }
            assert_ne!(a.points, c.points);
        }
    }
}
