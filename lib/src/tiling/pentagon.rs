//! Pentagons (PLAN §2): the "house", a unit square with a unit equilateral
//! triangle on one side. It is the only equilateral pentagon whose angles are
//! all multiples of 30°, so it keeps coordinates exact, and it appears by
//! merging a square and a triangle that share an edge.

use super::{Face, FaceSide, PolygonKind, Tiling};
use crate::geometry::Point;
use crate::rng::Rng;

/// Merges some square–triangle pairs of `tiling` into house pentagons.
///
/// Every interior edge between a square and a triangle is a candidate. They
/// are visited in random order, and each is merged with probability `fraction`
/// unless its square or its triangle already belongs to a pentagon. So `0.0`
/// changes nothing, and `1.0` merges as many as can fit. Any tiling with both
/// squares and triangles works.
///
/// Each pentagon's vertices start at its 60° apex (the triangle's far corner)
/// and run anticlockwise, so corner and side numbers mean the same on every
/// pentagon and one clearance table serves them all.
pub fn merge_pentagons(tiling: &Tiling, fraction: f64, rng: &mut Rng) -> Tiling {
    let mut candidates: Vec<usize> = (0..tiling.edges.len())
        .filter(|&e| square_triangle_pair(tiling, e).is_some())
        .collect();
    // Edge ids are in a fixed order, so shuffling with a seeded rng is too.
    rng.shuffle(&mut candidates);

    let mut merged = vec![false; tiling.faces.len()];
    let mut pentagons: Vec<(PolygonKind, Vec<Point>)> = Vec::new();
    for e in candidates {
        let Some((square, triangle)) = square_triangle_pair(tiling, e) else {
            continue;
        };
        if merged[square.face] || merged[triangle.face] || !rng.chance(fraction) {
            continue;
        }
        merged[square.face] = true;
        merged[triangle.face] = true;
        pentagons.push((PolygonKind::Pentagon, house(tiling, square, triangle)));
    }

    let mut faces: Vec<_> = tiling
        .polygons()
        .into_iter()
        .zip(&merged)
        .filter(|(_, &gone)| !gone)
        .map(|(polygon, _)| polygon)
        .collect();
    faces.extend(pentagons);
    // Merging only joins neighbours, so smoothing would find nothing to remove.
    Tiling::from_faces(&faces, false)
}

/// The square's and the triangle's side on edge `e`, if it lies between a
/// square and a triangle.
fn square_triangle_pair(tiling: &Tiling, e: usize) -> Option<(FaceSide, FaceSide)> {
    let [Some(a), Some(b)] = tiling.edges[e].faces else {
        return None;
    };
    match (tiling.faces[a.face].kind, tiling.faces[b.face].kind) {
        (PolygonKind::Square, PolygonKind::Triangle) => Some((a, b)),
        (PolygonKind::Triangle, PolygonKind::Square) => Some((b, a)),
        _ => None,
    }
}

/// The pentagon made of `square` and `triangle` across their shared side,
/// starting at the triangle's apex and running anticlockwise.
fn house(tiling: &Tiling, square: FaceSide, triangle: FaceSide) -> Vec<Point> {
    let at = |face: &Face, i: usize| tiling.points[face.vertices[i % face.vertices.len()]];
    let (sq, tri) = (&tiling.faces[square.face], &tiling.faces[triangle.face]);
    // The shared side runs s → s+1 on the square and the other way on the
    // triangle. Round the outside the pentagon goes: the apex, then the square's
    // corners from s+1 on round to s (the 150° corner at s+1 comes first).
    let apex = at(tri, triangle.side + 2);
    let mut vertices = vec![apex];
    vertices.extend((1..=4).map(|k| at(sq, square.side + k)));
    vertices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiling::checks::assert_valid;
    use crate::tiling::grow::RandomMix;
    use crate::tiling::rows::RowMix;
    use crate::tiling::{catalogue, Region, Shape, TilingSource};

    const REGION: Region = Region::rectangle(16.0, 12.0);

    fn count(tiling: &Tiling, kind: PolygonKind) -> usize {
        tiling.faces.iter().filter(|f| f.kind == kind).count()
    }

    /// Triangles count 1 and squares 2 and pentagons 3, so merging keeps this
    /// total whatever is merged.
    fn weight(tiling: &Tiling) -> usize {
        count(tiling, PolygonKind::Triangle)
            + 2 * count(tiling, PolygonKind::Square)
            + 3 * count(tiling, PolygonKind::Pentagon)
    }

    fn bases() -> Vec<(&'static str, Tiling)> {
        let mut rng = Rng::new(5);
        vec![
            (
                "elongated-triangular",
                Tiling::build(&catalogue::ELONGATED_TRIANGULAR, REGION),
            ),
            (
                "snub-square",
                Tiling::build(&catalogue::SNUB_SQUARE, REGION),
            ),
            (
                "rhombitrihexagonal",
                Tiling::build(&catalogue::RHOMBITRIHEXAGONAL, REGION),
            ),
            ("random", RandomMix::default().build(REGION, &mut rng)),
            ("row-mix", RowMix::default().build(REGION, &mut rng)),
        ]
    }

    #[test]
    fn merged_tilings_are_valid_for_many_seeds_and_sources() {
        for (name, base) in bases() {
            for seed in 0..8 {
                for fraction in [0.3, 1.0] {
                    let tiling = merge_pentagons(&base, fraction, &mut Rng::new(seed));
                    assert_valid(&tiling, &format!("{name} seed {seed} fraction {fraction}"));
                    assert_eq!(weight(&tiling), weight(&base), "{name}: area changed");
                    assert_eq!(
                        tiling.faces.len(),
                        base.faces.len() - count(&tiling, PolygonKind::Pentagon)
                    );
                }
            }
        }
        for shape in [Shape::Hexagon, Shape::Circle] {
            let base = RandomMix::default().build(REGION.with_shape(shape), &mut Rng::new(2));
            assert_valid(
                &merge_pentagons(&base, 0.6, &mut Rng::new(3)),
                &format!("{shape:?}"),
            );
        }
    }

    #[test]
    fn fraction_zero_makes_no_pentagons() {
        for (name, base) in bases() {
            let tiling = merge_pentagons(&base, 0.0, &mut Rng::new(1));
            assert_eq!(count(&tiling, PolygonKind::Pentagon), 0, "{name}");
            assert_eq!(tiling.faces.len(), base.faces.len(), "{name}");
        }
    }

    #[test]
    fn fraction_one_makes_many_pentagons() {
        for (name, base) in bases().into_iter().take(4) {
            let tiling = merge_pentagons(&base, 1.0, &mut Rng::new(1));
            let pentagons = count(&tiling, PolygonKind::Pentagon);
            assert!(pentagons >= 8, "{name}: {pentagons} pentagons");
            let half = merge_pentagons(&base, 0.3, &mut Rng::new(1));
            assert!(pentagons > count(&half, PolygonKind::Pentagon), "{name}");
        }
    }

    #[test]
    fn tilings_without_both_squares_and_triangles_are_left_alone() {
        for cell in [
            &catalogue::HEXAGONAL,
            &catalogue::TRIANGULAR,
            &catalogue::SQUARE,
        ] {
            let base = Tiling::build(cell, REGION);
            let tiling = merge_pentagons(&base, 1.0, &mut Rng::new(1));
            assert_eq!(tiling.faces.len(), base.faces.len(), "{}", cell.name);
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_tiling() {
        let base = Tiling::build(&catalogue::ELONGATED_TRIANGULAR, REGION);
        let make = |seed| merge_pentagons(&base, 0.5, &mut Rng::new(seed));
        let (a, b, c) = (make(7), make(7), make(8));
        assert_eq!(a.points, b.points);
        assert_eq!(a.faces.len(), b.faces.len());
        for (x, y) in a.faces.iter().zip(&b.faces) {
            assert_eq!((x.kind, &x.vertices), (y.kind, &y.vertices));
        }
        let kinds = |t: &Tiling| {
            t.faces
                .iter()
                .map(|f| f.vertices.clone())
                .collect::<Vec<_>>()
        };
        assert_ne!(kinds(&a), kinds(&c));
    }

    #[test]
    fn every_pentagon_starts_at_its_apex_and_is_the_same_shape() {
        // The apex is the one 60° corner, and the edge leaving it points
        // somewhere, so what must match between pentagons is the turning: the
        // first edge, then turns of 1, 3, 3, 1 and 4 steps back to the apex.
        for (name, base) in bases() {
            let tiling = merge_pentagons(&base, 1.0, &mut Rng::new(4));
            for face in tiling
                .faces
                .iter()
                .filter(|f| f.kind == PolygonKind::Pentagon)
            {
                assert_eq!(face.vertices.len(), 5, "{name}");
                let turns: Vec<u8> = (0..5)
                    .map(|i| {
                        let edge = |k: usize| {
                            let a = tiling.points[face.vertices[k % 5]];
                            let b = tiling.points[face.vertices[(k + 1) % 5]];
                            (b - a).as_unit_direction().expect("unit edge").steps()
                        };
                        (edge(i) + 12 - edge(i + 4)) % 12
                    })
                    .collect();
                assert_eq!(
                    turns,
                    [4, 1, 3, 3, 1],
                    "{name}: not apex first, anticlockwise"
                );
            }
        }
    }

    #[test]
    fn a_prototile_pentagon_walks_the_same_house() {
        use crate::geometry::Direction;
        use crate::tiling::Prototile;
        let house = Prototile {
            kind: PolygonKind::Pentagon,
            start: Point::ORIGIN,
            direction: Direction::new(0),
        };
        let vertices = house.vertices(Point::ORIGIN);
        // The fifth edge turns by every corner's angle but the apex's and must
        // land back on the start, or the angles are wrong.
        let steps: u8 = (1..5).map(|c| PolygonKind::Pentagon.turn_steps_at(c)).sum();
        let last = Direction::new(0).turn(steps).unit();
        assert_eq!(vertices[4] + last, vertices[0]);
        assert_eq!(vertices.len(), 5);
    }

    #[test]
    #[should_panic(expected = "pentagon")]
    fn a_pentagon_has_no_single_corner_angle() {
        let _ = PolygonKind::Pentagon.interior_steps();
    }
}
