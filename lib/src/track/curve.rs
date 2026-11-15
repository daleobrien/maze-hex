//! The three kinds of curve a track is drawn with: straight lines, circular
//! arcs and (as a fallback for irregular tiles) cubic Béziers.
//!
//! Curves are in floats and in edge lengths, with `y` pointing up. They are only
//! ever used for drawing and for the clearance check; the maze itself is built
//! from exact geometry.

use std::f64::consts::PI;

/// A point or vector in floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

impl V2 {
    pub const fn new(x: f64, y: f64) -> Self {
        V2 { x, y }
    }

    pub fn plus(self, o: V2) -> V2 {
        V2::new(self.x + o.x, self.y + o.y)
    }

    pub fn minus(self, o: V2) -> V2 {
        V2::new(self.x - o.x, self.y - o.y)
    }

    pub fn scale(self, k: f64) -> V2 {
        V2::new(self.x * k, self.y * k)
    }

    pub fn dot(self, o: V2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    pub fn cross(self, o: V2) -> f64 {
        self.x * o.y - self.y * o.x
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn distance(self, o: V2) -> f64 {
        self.minus(o).length()
    }

    /// This vector turned a quarter turn anticlockwise.
    pub fn perp(self) -> V2 {
        V2::new(-self.y, self.x)
    }

    /// The point at `angle` (radians) on the circle of `radius` round `self`.
    pub fn polar(self, radius: f64, angle: f64) -> V2 {
        V2::new(self.x + radius * angle.cos(), self.y + radius * angle.sin())
    }
}

impl From<(f64, f64)> for V2 {
    fn from((x, y): (f64, f64)) -> Self {
        V2::new(x, y)
    }
}

/// One piece of a track's wall or centreline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Line {
        from: V2,
        to: V2,
    },
    /// An arc round `centre` from angle `start`, turning by `sweep` radians
    /// (positive is anticlockwise).
    Arc {
        centre: V2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
    Cubic {
        from: V2,
        c1: V2,
        c2: V2,
        to: V2,
    },
}

impl Curve {
    /// The point a fraction `t` (0 to 1) of the way along.
    pub fn at(&self, t: f64) -> V2 {
        match *self {
            Curve::Line { from, to } => from.plus(to.minus(from).scale(t)),
            Curve::Arc {
                centre,
                radius,
                start,
                sweep,
            } => centre.polar(radius, start + sweep * t),
            Curve::Cubic { from, c1, c2, to } => {
                let s = 1.0 - t;
                from.scale(s * s * s)
                    .plus(c1.scale(3.0 * s * s * t))
                    .plus(c2.scale(3.0 * s * t * t))
                    .plus(to.scale(t * t * t))
            }
        }
    }

    pub fn start(&self) -> V2 {
        self.at(0.0)
    }

    pub fn end(&self) -> V2 {
        self.at(1.0)
    }

    /// Roughly how long the curve is.
    pub fn length(&self) -> f64 {
        match *self {
            Curve::Line { from, to } => from.distance(to),
            Curve::Arc { radius, sweep, .. } => radius * sweep.abs(),
            Curve::Cubic { .. } => {
                let points = self.points(16);
                points.windows(2).map(|w| w[0].distance(w[1])).sum()
            }
        }
    }

    /// `n + 1` points evenly spaced along the curve, both ends included.
    pub fn points(&self, n: usize) -> Vec<V2> {
        (0..=n).map(|i| self.at(i as f64 / n as f64)).collect()
    }

    /// Points no more than `step` apart, both ends included.
    pub fn sample(&self, step: f64) -> Vec<V2> {
        let n = (self.length() / step).ceil().max(1.0) as usize;
        self.points(n)
    }

    /// The same curve, walked the other way.
    pub fn reversed(&self) -> Curve {
        match *self {
            Curve::Line { from, to } => Curve::Line { from: to, to: from },
            Curve::Arc {
                centre,
                radius,
                start,
                sweep,
            } => Curve::Arc {
                centre,
                radius,
                start: start + sweep,
                sweep: -sweep,
            },
            Curve::Cubic { from, c1, c2, to } => Curve::Cubic {
                from: to,
                c1: c2,
                c2: c1,
                to: from,
            },
        }
    }

    /// An arc round `centre` from `from` to `to` (both on the same circle),
    /// going the way that passes through `via_side`: the arc whose midpoint
    /// makes `via_side` return true, trying the shorter way first.
    pub fn arc_through(centre: V2, from: V2, to: V2, via_side: impl Fn(V2) -> bool) -> Curve {
        let radius = from.distance(centre);
        let start = (from.y - centre.y).atan2(from.x - centre.x);
        let end = (to.y - centre.y).atan2(to.x - centre.x);
        let mut short = end - start;
        while short > PI {
            short -= 2.0 * PI;
        }
        while short <= -PI {
            short += 2.0 * PI;
        }
        let long = if short > 0.0 {
            short - 2.0 * PI
        } else {
            short + 2.0 * PI
        };
        let arc = |sweep: f64| Curve::Arc {
            centre,
            radius,
            start,
            sweep,
        };
        if via_side(arc(short).at(0.5)) {
            arc(short)
        } else {
            arc(long)
        }
    }
}

/// The shortest distance from `p` to the polyline through `points`.
pub fn distance_to_polyline(p: V2, points: &[V2]) -> f64 {
    if points.len() == 1 {
        return p.distance(points[0]);
    }
    points
        .windows(2)
        .map(|w| distance_to_segment(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

fn distance_to_segment(p: V2, a: V2, b: V2) -> f64 {
    let ab = b.minus(a);
    let len2 = ab.dot(ab);
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = (p.minus(a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a.plus(ab.scale(t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_run_from_start_to_end_and_reverse() {
        let arc = Curve::arc_through(
            V2::new(0.0, 0.0),
            V2::new(1.0, 0.0),
            V2::new(0.0, 1.0),
            |_| true,
        );
        assert!(arc.start().distance(V2::new(1.0, 0.0)) < 1e-12);
        assert!(arc.end().distance(V2::new(0.0, 1.0)) < 1e-12);
        let back = arc.reversed();
        assert!(back.start().distance(arc.end()) < 1e-12);
        assert!(back.end().distance(arc.start()) < 1e-12);
        assert!((arc.length() - PI / 2.0).abs() < 1e-12);
    }

    #[test]
    fn arc_through_takes_the_long_way_when_asked() {
        // Only points below the x axis are allowed, so the arc from east to
        // north must go round through west and south.
        let arc = Curve::arc_through(
            V2::new(0.0, 0.0),
            V2::new(1.0, 0.0),
            V2::new(0.0, 1.0),
            |p| p.y < 0.0,
        );
        assert!((arc.length() - 1.5 * PI).abs() < 1e-12);
    }

    #[test]
    fn samples_are_close_enough() {
        let line = Curve::Line {
            from: V2::new(0.0, 0.0),
            to: V2::new(1.0, 0.0),
        };
        let points = line.sample(0.1);
        assert_eq!(points.len(), 11);
        assert!(points
            .windows(2)
            .all(|w| w[0].distance(w[1]) <= 0.1 + 1e-12));
    }

    #[test]
    fn polyline_distance() {
        let points = [V2::new(0.0, 0.0), V2::new(1.0, 0.0)];
        assert!((distance_to_polyline(V2::new(0.5, 0.3), &points) - 0.3).abs() < 1e-12);
        assert!((distance_to_polyline(V2::new(2.0, 0.0), &points) - 1.0).abs() < 1e-12);
    }
}
