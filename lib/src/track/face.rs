//! A track inside one tile: its walls and centreline (PLAN §5.1–5.2).
//!
//! A track crosses side `i` at the side's midpoint `Mᵢ`, at right angles. Its
//! two walls cross the side at the *side points* `Mᵢ ± (W/2)·tᵢ`, where `tᵢ` is
//! the side's direction (anticlockwise round the tile) and `W` the track width.
//! "Ahead" is the side point nearer the side's end vertex.
//!
//! Every curve between two sides `p` and `q` follows one rule: if the sides are
//! parallel it is a straight line; otherwise it is an arc round the point where
//! the two sides' lines meet. In a regular polygon that point is the same
//! distance from both sides' midpoints, so the arc meets both sides squarely.
//! For any other tile, a cubic Bézier with square ends is the fallback.

use super::curve::{Curve, V2};
use super::TrackStyle;
use crate::tiling::Tiling;

/// The float geometry of one tile, anticlockwise.
#[derive(Clone, Debug)]
pub struct FaceGeometry {
    vertices: Vec<V2>,
}

/// The ring at the end of a dead end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub centre: V2,
    pub radius: f64,
    /// How far before the ring's centre (towards the side) the blend curves'
    /// centres sit.
    rise: f64,
}

/// A wall from one side point to another, made of one or more curves.
#[derive(Clone, Debug)]
pub struct Wall {
    /// The side whose "ahead" side point the wall starts at.
    pub from_side: usize,
    /// The side whose "behind" side point the wall ends at.
    pub to_side: usize,
    pub curves: Vec<Curve>,
}

impl FaceGeometry {
    pub fn of(tiling: &Tiling, face: usize) -> Self {
        let vertices = tiling.faces[face]
            .vertices
            .iter()
            .map(|&v| V2::from(tiling.points[v].to_f64()))
            .collect();
        FaceGeometry { vertices }
    }

    pub fn from_vertices(vertices: Vec<V2>) -> Self {
        FaceGeometry { vertices }
    }

    pub fn sides(&self) -> usize {
        self.vertices.len()
    }

    /// Vertex `i` (taken round the tile, so `i` may be `sides()`).
    pub fn vertex(&self, i: usize) -> V2 {
        self.vertices[i % self.sides()]
    }

    /// The unit direction of side `i`, anticlockwise round the tile.
    pub fn direction(&self, i: usize) -> V2 {
        let d = self.vertex(i + 1).minus(self.vertex(i));
        d.scale(1.0 / d.length())
    }

    /// The unit normal of side `i`, pointing into the tile.
    pub fn inward(&self, i: usize) -> V2 {
        self.direction(i).perp()
    }

    pub fn midpoint(&self, i: usize) -> V2 {
        self.vertex(i).plus(self.vertex(i + 1)).scale(0.5)
    }

    pub fn centre(&self) -> V2 {
        let n = self.sides() as f64;
        let sum = self
            .vertices
            .iter()
            .fold(V2::new(0.0, 0.0), |s, &v| s.plus(v));
        sum.scale(1.0 / n)
    }

    /// How far `p` is inside side `i`'s line (negative means outside).
    pub fn inside_distance(&self, p: V2, i: usize) -> f64 {
        p.minus(self.vertex(i)).dot(self.inward(i))
    }

    pub fn contains(&self, p: V2) -> bool {
        (0..self.sides()).all(|i| self.inside_distance(p, i) >= -1e-9)
    }

    /// A wall's crossing point on side `i`: `ahead` is the one nearer the
    /// side's end vertex.
    pub fn side_point(&self, i: usize, ahead: bool, width: f64) -> V2 {
        let offset = self.direction(i).scale(width / 2.0);
        if ahead {
            self.midpoint(i).plus(offset)
        } else {
            self.midpoint(i).minus(offset)
        }
    }

    /// The centre of a dead end's cap, entered from side `i`: `depth` into
    /// the tile, but never past its centre.
    pub fn cap_centre(&self, i: usize, depth: f64) -> V2 {
        let depth = depth.min(self.inside_distance(self.centre(), i));
        self.midpoint(i).plus(self.inward(i).scale(depth))
    }

    /// The curve from `a` on side `p` to `b` on side `q`, square to both.
    fn between(&self, p: usize, a: V2, q: usize, b: V2) -> Curve {
        let (tp, tq) = (self.direction(p), self.direction(q));
        let square_ends = || {
            let k = a.distance(b) / 3.0;
            Curve::Cubic {
                from: a,
                c1: a.plus(self.inward(p).scale(k)),
                c2: b.plus(self.inward(q).scale(k)),
                to: b,
            }
        };
        let cross = tp.cross(tq);
        if cross.abs() < 1e-9 {
            // Parallel sides: straight across if the points face each other.
            return if b.minus(a).dot(tp).abs() < 1e-9 {
                Curve::Line { from: a, to: b }
            } else {
                square_ends()
            };
        }
        // Where the lines of sides p and q meet.
        let s = self.vertex(q).minus(self.vertex(p)).cross(tq) / cross;
        let centre = self.vertex(p).plus(tp.scale(s));
        if (centre.distance(a) - centre.distance(b)).abs() > 1e-9 {
            return square_ends();
        }
        Curve::arc_through(centre, a, b, |m| self.contains(m))
    }

    /// The centreline of a track from side `p` to side `q`.
    pub fn centreline(&self, p: usize, q: usize) -> Curve {
        self.between(p, self.midpoint(p), q, self.midpoint(q))
    }

    /// The centreline of a dead end from side `p` to its end: the centre of
    /// its ring or cap.
    pub fn centreline_to_cap(&self, p: usize, style: &TrackStyle) -> Curve {
        Curve::Line {
            from: self.midpoint(p),
            to: self.dead_end_centre(p, style),
        }
    }

    /// Where a dead end entered by side `p` ends: its ring's centre if it has
    /// one, otherwise its round cap's.
    pub fn dead_end_centre(&self, p: usize, style: &TrackStyle) -> V2 {
        match self.ring(p, style) {
            Some(ring) => ring.centre,
            None => self.cap_centre(p, style.cap_depth),
        }
    }

    /// The ring a dead end entered by side `p` opens into, if `style` asks
    /// for rings and one fits in this tile. Triangles are too small for one.
    pub fn ring(&self, p: usize, style: &TrackStyle) -> Option<Ring> {
        if !style.circles {
            return None;
        }
        let (half, radius, blend) = (style.width / 2.0, style.ring_radius, style.ring_blend);
        if radius <= half {
            return None;
        }
        // The blend circle touches the wall line and the ring from outside;
        // `rise` is how far its centre sits before the ring's centre.
        let rise = ((radius + blend).powi(2) - (half + blend).powi(2)).sqrt();
        let depth = style.cap_depth.max(rise + 0.02);
        let centre = self.midpoint(p).plus(self.inward(p).scale(depth));
        let fits = (0..self.sides())
            .filter(|&side| side != p)
            .all(|side| self.inside_distance(centre, side) >= radius + style.margin);
        let deep_enough =
            self.inside_distance(centre, p) <= self.inside_distance(self.centre(), p) + 1e-9;
        (fits && deep_enough).then_some(Ring {
            centre,
            radius,
            rise,
        })
    }

    /// The wall of a dead end with a ring: straight in, a small blend, round
    /// the far side of the ring, the mirror blend and straight out.
    fn ring_wall(&self, p: usize, ring: Ring, style: &TrackStyle) -> Vec<Curve> {
        let (t, n) = (self.direction(p), self.inward(p));
        let (half, blend) = (style.width / 2.0, style.ring_blend);
        let m = self.midpoint(p);
        // A point `u` along the side and `v` into the tile from its midpoint.
        let at = |u: f64, v: f64| m.plus(t.scale(u)).plus(n.scale(v));
        let depth = ring.centre.minus(m).dot(n);
        let mut curves = Vec::new();
        let mut last = at(half, 0.0);
        for (k, sign) in [1.0, -1.0].into_iter().enumerate() {
            let blend_centre = at(sign * (half + blend), depth - ring.rise);
            let on_line = at(sign * half, depth - ring.rise);
            let towards = blend_centre.minus(ring.centre);
            let on_ring = ring
                .centre
                .plus(towards.scale(ring.radius / towards.length()));
            if k == 0 {
                curves.push(Curve::Line {
                    from: last,
                    to: on_line,
                });
                curves.push(Curve::arc_short(blend_centre, on_line, on_ring));
                last = on_ring;
            } else {
                // Round the far side of the ring, away from side p.
                curves.push(Curve::arc_through(ring.centre, last, on_ring, |q| {
                    q.minus(ring.centre).dot(n) > 0.0
                }));
                curves.push(Curve::arc_short(blend_centre, on_ring, on_line));
                curves.push(Curve::Line {
                    from: on_line,
                    to: at(-half, 0.0),
                });
            }
        }
        curves
    }

    /// The wall from side `p`'s ahead point to side `q`'s behind point, going
    /// anticlockwise past the closed sides between them. When `p == q` the
    /// track is a dead end and the wall is a U-turn with a round cap.
    pub fn wall(&self, p: usize, q: usize, style: &TrackStyle) -> Wall {
        let width = style.width;
        let a = self.side_point(p, true, width);
        let b = self.side_point(q, false, width);
        let ring = if p == q { self.ring(p, style) } else { None };
        let curves = if let Some(ring) = ring {
            self.ring_wall(p, ring, style)
        } else if p == q {
            let centre = self.cap_centre(p, style.cap_depth);
            let tp = self.direction(p);
            let half = width / 2.0;
            vec![
                Curve::Line {
                    from: a,
                    to: centre.plus(tp.scale(half)),
                },
                Curve::Arc {
                    centre,
                    radius: half,
                    start: tp.y.atan2(tp.x),
                    sweep: std::f64::consts::PI,
                },
                Curve::Line {
                    from: centre.minus(tp.scale(half)),
                    to: b,
                },
            ]
        } else {
            vec![self.between(p, a, q, b)]
        };
        Wall {
            from_side: p,
            to_side: q,
            curves,
        }
    }

    /// Every wall of a tile whose open sides are grouped into `blocks`, each
    /// listing its sides in anticlockwise order. Consecutive sides of a block
    /// are joined by a wall; a block of one side is a dead end.
    pub fn walls(&self, blocks: &[Vec<usize>], style: &TrackStyle) -> Vec<Wall> {
        let mut walls = Vec::new();
        for block in blocks {
            for (k, &p) in block.iter().enumerate() {
                let q = block[(k + 1) % block.len()];
                walls.push(self.wall(p, q, style));
            }
        }
        walls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(width: f64) -> TrackStyle {
        TrackStyle {
            width,
            ..TrackStyle::default()
        }
    }

    fn regular(n: usize) -> FaceGeometry {
        let mut vertices = Vec::new();
        let mut p = V2::new(0.0, 0.0);
        for i in 0..n {
            vertices.push(p);
            let angle = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            p = p.polar(1.0, angle);
        }
        FaceGeometry::from_vertices(vertices)
    }

    fn arc_radius(curve: &Curve) -> f64 {
        match curve {
            Curve::Arc { radius, .. } => *radius,
            other => panic!("expected an arc, got {other:?}"),
        }
    }

    #[test]
    fn neighbouring_sides_curve_round_their_shared_corner() {
        let w = 0.4;
        for n in [3, 4, 6] {
            let face = regular(n);
            let inner = face.wall(0, 1, &style(w));
            let outer = face.wall(1, 0, &style(w));
            assert!((arc_radius(&inner.curves[0]) - (0.5 - w / 2.0)).abs() < 1e-9);
            assert!((arc_radius(&outer.curves[0]) - (0.5 + w / 2.0)).abs() < 1e-9);
            assert!((arc_radius(&face.centreline(0, 1)) - 0.5).abs() < 1e-9);
        }
    }

    #[test]
    fn hexagon_sides_two_apart_use_the_wide_arc() {
        let face = regular(6);
        assert!((arc_radius(&face.centreline(0, 2)) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn opposite_sides_are_joined_by_straight_lines() {
        for n in [4, 6] {
            let face = regular(n);
            let half = n / 2;
            assert!(matches!(face.centreline(0, half), Curve::Line { .. }));
            assert!(matches!(
                face.wall(0, half, &style(0.4)).curves[0],
                Curve::Line { .. }
            ));
        }
    }

    #[test]
    fn rings_fit_squares_and_hexagons_but_not_triangles() {
        let ringed = TrackStyle::default().with_circles();
        assert!(regular(3).ring(0, &ringed).is_none());
        for n in [4, 6] {
            let face = regular(n);
            let ring = face.ring(0, &ringed).expect("a ring fits");
            assert!((ring.radius - ringed.ring_radius).abs() < 1e-12);
            assert!(
                face.ring(0, &TrackStyle::default()).is_none(),
                "rings are opt-in"
            );
            // The ringed dead end is one unbroken wall, inside the tile, and
            // its end is the ring's centre.
            let wall = face.wall(0, 0, &ringed);
            assert_eq!(wall.curves.len(), 5);
            for pair in wall.curves.windows(2) {
                assert!(pair[0].end().distance(pair[1].start()) < 1e-9);
            }
            for curve in &wall.curves {
                assert!(curve.sample(0.02).into_iter().all(|pt| face.contains(pt)));
            }
            assert!(face.dead_end_centre(0, &ringed).distance(ring.centre) < 1e-12);
        }
    }

    #[test]
    fn walls_start_and_end_on_their_side_points() {
        let w = 0.4;
        for n in [3, 4, 6] {
            let face = regular(n);
            for p in 0..n {
                for q in 0..n {
                    let wall = face.wall(p, q, &style(w));
                    let first = wall.curves[0].start();
                    let last = wall.curves[wall.curves.len() - 1].end();
                    assert!(first.distance(face.side_point(p, true, w)) < 1e-9);
                    assert!(last.distance(face.side_point(q, false, w)) < 1e-9);
                    // Each piece follows on from the last.
                    for pair in wall.curves.windows(2) {
                        assert!(pair[0].end().distance(pair[1].start()) < 1e-9);
                    }
                    // And the wall stays inside the tile.
                    for curve in &wall.curves {
                        assert!(curve.sample(0.02).into_iter().all(|pt| face.contains(pt)));
                    }
                }
            }
        }
    }

    fn house() -> FaceGeometry {
        crate::track::clearance::reference(crate::tiling::PolygonKind::Pentagon)
    }

    #[test]
    fn pentagon_tracks_leave_every_side_squarely() {
        // Sides of the house are not all alike, so some pairs fall back to a
        // Bézier curve; either way a track must cross each side at right angles.
        let face = house();
        let (mut arcs, mut beziers, mut lines) = (0, 0, 0);
        for p in 0..5 {
            for q in 0..5 {
                if p == q {
                    continue;
                }
                let curve = face.centreline(p, q);
                match curve {
                    Curve::Arc { .. } => arcs += 1,
                    Curve::Cubic { .. } => beziers += 1,
                    Curve::Line { .. } => lines += 1,
                }
                let eps = 1e-4;
                let start = curve.at(eps).minus(curve.start());
                let end = curve.end().minus(curve.at(1.0 - eps));
                let along =
                    |v: V2, side: usize| v.scale(1.0 / v.length()).dot(face.direction(side)).abs();
                assert!(along(start, p) < 1e-3, "{p}->{q} start");
                assert!(along(end, q) < 1e-3, "{p}->{q} end");
                assert!(start.dot(face.inward(p)) > 0.0, "{p}->{q} leaves inwards");
                assert!(end.dot(face.inward(q)) < 0.0, "{p}->{q} arrives outwards");
                assert!(curve.sample(0.02).into_iter().all(|pt| face.contains(pt)));
            }
        }
        // The two vertical sides face each other across the square part.
        assert!(matches!(face.centreline(1, 3), Curve::Line { .. }));
        assert!(
            arcs > 0 && beziers > 0 && lines > 0,
            "{arcs} {beziers} {lines}"
        );
    }

    #[test]
    fn pentagon_walls_start_end_and_stay_inside() {
        let face = house();
        let w = 0.4;
        for p in 0..5 {
            for q in 0..5 {
                let wall = face.wall(p, q, &style(w));
                let first = wall.curves[0].start();
                let last = wall.curves[wall.curves.len() - 1].end();
                assert!(first.distance(face.side_point(p, true, w)) < 1e-9);
                assert!(last.distance(face.side_point(q, false, w)) < 1e-9);
                for pair in wall.curves.windows(2) {
                    assert!(pair[0].end().distance(pair[1].start()) < 1e-9);
                }
                for curve in &wall.curves {
                    assert!(
                        curve.sample(0.02).into_iter().all(|pt| face.contains(pt)),
                        "wall {p}->{q} leaves the pentagon"
                    );
                }
            }
        }
    }
}
