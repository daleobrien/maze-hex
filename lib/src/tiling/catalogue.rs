//! The periodic tilings maze-hex can build, each as a unit cell: two lattice
//! vectors and the tiles inside one cell.
//!
//! Every tiling here uses only triangles, squares and hexagons with edges of
//! length 1, so all coordinates stay exact (see [`crate::geometry`]). A test
//! checks that each cell's tiles cover exactly the cell's area, which catches a
//! missing or overlapping tile.

use super::{PolygonKind, Prototile, UnitCell};
use crate::geometry::{Direction, Point, Q3};

/// The unit vector `k × 30°`.
const fn u(k: u8) -> Point {
    Direction::new(k).unit()
}

/// The sum of the unit vectors in `steps` (a short walk along edges).
const fn walk(steps: &[u8]) -> Point {
    let mut p = Point::ORIGIN;
    let mut i = 0;
    while i < steps.len() {
        let d = u(steps[i]);
        p = Point::new(
            Q3::new(p.x.a + d.x.a, p.x.b + d.x.b),
            Q3::new(p.y.a + d.y.a, p.y.b + d.y.b),
        );
        i += 1;
    }
    p
}

const fn tile(kind: PolygonKind, start: Point, direction: u8) -> Prototile {
    Prototile {
        kind,
        start,
        direction: Direction::new(direction),
    }
}

use PolygonKind::{Hexagon, Square, Triangle};

/// 6.6.6: the hexagonal tiling, the main target.
pub const HEXAGONAL: UnitCell = UnitCell {
    name: "hexagonal",
    notation: "6.6.6",
    a: walk(&[2, 4]),
    b: walk(&[0, 2]),
    tiles: &[tile(Hexagon, Point::ORIGIN, 0)],
};

/// 4.4.4.4: the square tiling.
pub const SQUARE: UnitCell = UnitCell {
    name: "square",
    notation: "4.4.4.4",
    a: walk(&[0]),
    b: walk(&[3]),
    tiles: &[tile(Square, Point::ORIGIN, 0)],
};

/// 3.3.3.3.3.3: the triangular tiling (Lubra's grid).
pub const TRIANGULAR: UnitCell = UnitCell {
    name: "triangular",
    notation: "3.3.3.3.3.3",
    a: walk(&[0]),
    b: walk(&[2]),
    tiles: &[
        tile(Triangle, Point::ORIGIN, 0),
        tile(Triangle, walk(&[0]), 2),
    ],
};

/// 3.6.3.6: the trihexagonal tiling, hexagons joined corner to corner.
pub const TRIHEXAGONAL: UnitCell = UnitCell {
    name: "trihexagonal",
    notation: "3.6.3.6",
    a: walk(&[0, 0]),
    b: walk(&[2, 2]),
    tiles: &[
        tile(Hexagon, Point::ORIGIN, 0),
        tile(Triangle, walk(&[0]), 0),
        tile(Triangle, walk(&[0, 2]), 2),
    ],
};

/// 3.4.6.4: the rhombitrihexagonal tiling, hexagons ringed by squares.
pub const RHOMBITRIHEXAGONAL: UnitCell = UnitCell {
    name: "rhombitrihexagonal",
    notation: "3.4.6.4",
    a: walk(&[3, 2, 4]),
    b: walk(&[1, 0, 2]),
    tiles: &[
        tile(Hexagon, Point::ORIGIN, 0),
        // A square on each of the hexagon's first three edges ...
        tile(Square, Point::ORIGIN, 9),
        tile(Square, walk(&[0]), 11),
        tile(Square, walk(&[0, 2]), 1),
        // ... and a triangle in each of the two kinds of gap between them.
        tile(Triangle, walk(&[0]), 9),
        tile(Triangle, walk(&[0, 2]), 11),
    ],
};

/// 3.3.3.4.4: the elongated triangular tiling, rows of squares and triangles.
pub const ELONGATED_TRIANGULAR: UnitCell = UnitCell {
    name: "elongated-triangular",
    notation: "3.3.3.4.4",
    a: walk(&[0]),
    b: walk(&[3, 2]),
    tiles: &[
        tile(Square, Point::ORIGIN, 0),
        tile(Triangle, walk(&[3]), 0),
        tile(Triangle, walk(&[3, 0]), 2),
    ],
};

/// Every tiling, in the order the documents show them.
pub const ALL: [&UnitCell; 6] = [
    &HEXAGONAL,
    &SQUARE,
    &TRIANGULAR,
    &TRIHEXAGONAL,
    &RHOMBITRIHEXAGONAL,
    &ELONGATED_TRIANGULAR,
];

/// The tiling called `name`, if there is one.
pub fn by_name(name: &str) -> Option<&'static UnitCell> {
    ALL.into_iter().find(|cell| cell.name == name)
}
