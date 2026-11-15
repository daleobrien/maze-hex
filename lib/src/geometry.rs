//! Exact geometry for tilings whose edges all have length 1 and point in a
//! multiple of 30°.
//!
//! Every polygon used here (triangle, square, hexagon) has interior angles of
//! 60°, 90° or 120°, so walking round any tile only ever turns by a multiple of
//! 30°. Every edge is then one of twelve unit vectors, and every vertex is a sum
//! of them. The components of those vectors are 0, ±1/2, ±1, ±√3/2, so every
//! coordinate is exactly `(a + b√3) / 2` for integers `a` and `b`.
//!
//! Keeping coordinates exact means two tiles that share a vertex compute the
//! *same* key for it: no rounding, no epsilon, and a `HashMap` can join tiles
//! together. Floats are used only for drawing and for clipping to a region.

use std::ops::{Add, Neg, Sub};

/// The exact number `(a + b√3) / 2`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Q3 {
    /// Twice the rational part.
    pub a: i64,
    /// Twice the coefficient of √3.
    pub b: i64,
}

impl Q3 {
    pub const ZERO: Q3 = Q3 { a: 0, b: 0 };

    /// The number `(a + b√3) / 2`.
    pub const fn new(a: i64, b: i64) -> Self {
        Q3 { a, b }
    }

    /// The nearest `f64`, for drawing.
    pub fn to_f64(self) -> f64 {
        (self.a as f64 + self.b as f64 * 3f64.sqrt()) / 2.0
    }

    /// `n` times this number.
    pub const fn times(self, n: i64) -> Self {
        Q3 {
            a: self.a * n,
            b: self.b * n,
        }
    }
}

impl Add for Q3 {
    type Output = Q3;
    fn add(self, other: Q3) -> Q3 {
        Q3::new(self.a + other.a, self.b + other.b)
    }
}

impl Sub for Q3 {
    type Output = Q3;
    fn sub(self, other: Q3) -> Q3 {
        Q3::new(self.a - other.a, self.b - other.b)
    }
}

impl Neg for Q3 {
    type Output = Q3;
    fn neg(self) -> Q3 {
        Q3::new(-self.a, -self.b)
    }
}

/// A point (or vector) with exact coordinates, in units of the edge length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: Q3,
    pub y: Q3,
}

impl Point {
    pub const ORIGIN: Point = Point {
        x: Q3::ZERO,
        y: Q3::ZERO,
    };

    pub const fn new(x: Q3, y: Q3) -> Self {
        Point { x, y }
    }

    /// `n` times this vector.
    pub const fn times(self, n: i64) -> Self {
        Point {
            x: self.x.times(n),
            y: self.y.times(n),
        }
    }

    /// The nearest `(x, y)` in floats, for drawing.
    pub fn to_f64(self) -> (f64, f64) {
        (self.x.to_f64(), self.y.to_f64())
    }

    /// The direction of this vector, if it is one of the twelve unit vectors.
    pub fn as_unit_direction(self) -> Option<Direction> {
        Direction::ALL.into_iter().find(|d| d.unit() == self)
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, other: Point) -> Point {
        Point::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, other: Point) -> Point {
        Point::new(self.x - other.x, self.y - other.y)
    }
}

impl Neg for Point {
    type Output = Point;
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}

/// One of the twelve directions `k × 30°`, counted anticlockwise from east.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Direction(u8);

/// The unit vectors for 0°, 30°, ..., 150°; the other six are their negatives.
const HALF_TURN_UNITS: [Point; 6] = [
    Point::new(Q3::new(2, 0), Q3::new(0, 0)),  // 0°: (1, 0)
    Point::new(Q3::new(0, 1), Q3::new(1, 0)),  // 30°: (√3/2, 1/2)
    Point::new(Q3::new(1, 0), Q3::new(0, 1)),  // 60°: (1/2, √3/2)
    Point::new(Q3::new(0, 0), Q3::new(2, 0)),  // 90°: (0, 1)
    Point::new(Q3::new(-1, 0), Q3::new(0, 1)), // 120°: (-1/2, √3/2)
    Point::new(Q3::new(0, -1), Q3::new(1, 0)), // 150°: (-√3/2, 1/2)
];

impl Direction {
    /// All twelve directions, anticlockwise from east.
    pub const ALL: [Direction; 12] = {
        let mut all = [Direction(0); 12];
        let mut k = 0;
        while k < 12 {
            all[k] = Direction(k as u8);
            k += 1;
        }
        all
    };

    /// The direction `k × 30°` (taken modulo 360°).
    pub const fn new(k: u8) -> Self {
        Direction(k % 12)
    }

    /// How many 30° steps anticlockwise from east.
    pub const fn steps(self) -> u8 {
        self.0
    }

    /// This direction turned anticlockwise by `steps × 30°`.
    pub const fn turn(self, steps: u8) -> Self {
        Direction::new(self.0 + steps)
    }

    /// The exact unit vector pointing this way.
    pub const fn unit(self) -> Point {
        let k = self.0 as usize;
        if k < 6 {
            HALF_TURN_UNITS[k]
        } else {
            let p = HALF_TURN_UNITS[k - 6];
            Point::new(Q3::new(-p.x.a, -p.x.b), Q3::new(-p.y.a, -p.y.b))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unit_vector_has_length_one() {
        for d in Direction::ALL {
            let (x, y) = d.unit().to_f64();
            assert!((x.hypot(y) - 1.0).abs() < 1e-12, "{d:?}");
            let angle = f64::from(d.steps()) * 30f64.to_radians();
            assert!((x - angle.cos()).abs() < 1e-12, "{d:?}");
            assert!((y - angle.sin()).abs() < 1e-12, "{d:?}");
        }
    }

    #[test]
    fn opposite_directions_cancel() {
        for d in Direction::ALL {
            assert_eq!(d.unit() + d.turn(6).unit(), Point::ORIGIN);
        }
    }

    #[test]
    fn walking_a_hexagon_returns_home_exactly() {
        let mut p = Point::ORIGIN;
        let mut d = Direction::new(0);
        for _ in 0..6 {
            p = p + d.unit();
            d = d.turn(2);
        }
        assert_eq!(p, Point::ORIGIN);
    }

    #[test]
    fn unit_directions_are_recognised() {
        for d in Direction::ALL {
            assert_eq!(d.unit().as_unit_direction(), Some(d));
        }
        assert_eq!(Direction::new(0).unit().times(2).as_unit_direction(), None);
    }
}
