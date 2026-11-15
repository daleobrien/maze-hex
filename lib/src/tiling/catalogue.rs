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

/// A point of the triangular lattice: `m` steps east and `n` steps at 60°.
const fn lattice(m: i64, n: i64) -> Point {
    Point::new(Q3::new(2 * m + n, 0), Q3::new(0, n))
}

/// 3.3.4.3.4: the snub square tiling. Two squares, turned 30° from each other,
/// and four triangles in a cell. The lattice vectors are the 15° and 105°
/// diagonals of two unit steps, because the squares sit at 0° and 30°.
pub const SNUB_SQUARE: UnitCell = UnitCell {
    name: "snub-square",
    notation: "3.3.4.3.4",
    a: walk(&[0, 1]),
    b: walk(&[3, 4]),
    tiles: &[
        tile(Square, Point::ORIGIN, 0),
        tile(Square, walk(&[0]), 10),
        // The triangles round the first square: below, right, above and left.
        tile(Triangle, walk(&[0]), 6),
        tile(Triangle, walk(&[0, 3]), 9),
        tile(Triangle, walk(&[3]), 0),
        tile(Triangle, Point::ORIGIN, 3),
    ],
};

/// 3.3.3.3.6: the snub hexagonal tiling, which comes in a left- and a
/// right-handed form; this is one of them. It is the triangular tiling with one
/// vertex in seven grown into a hexagon, so a cell holds one hexagon and eight
/// triangles. The lattice vectors are `2·e0 + e60` and its turn by 60°, which
/// have length √7. The triangles are the ones nearest the hexagon, so that
/// clipping never misses a tile that starts outside its own cell.
pub const SNUB_HEXAGONAL: UnitCell = UnitCell {
    name: "snub-hexagonal",
    notation: "3.3.3.3.6",
    a: walk(&[0, 0, 2]),
    b: walk(&[2, 2, 4]),
    tiles: &[
        tile(Hexagon, Point::ORIGIN, 0),
        // Upward triangles, named by their lowest-left corner ...
        tile(Triangle, lattice(1, 0), 0),
        tile(Triangle, lattice(-1, 2), 0),
        tile(Triangle, lattice(0, 2), 0),
        tile(Triangle, lattice(-1, 0), 0),
        // ... and downward ones, starting at their right-hand corner.
        tile(Triangle, lattice(-1, 2), 2),
        tile(Triangle, lattice(1, -1), 2),
        tile(Triangle, lattice(-1, 1), 2),
        tile(Triangle, lattice(1, 1), 2),
    ],
};

/// Every tiling, in the order the documents show them.
pub const ALL: [&UnitCell; 8] = [
    &HEXAGONAL,
    &SQUARE,
    &TRIANGULAR,
    &TRIHEXAGONAL,
    &RHOMBITRIHEXAGONAL,
    &ELONGATED_TRIANGULAR,
    &SNUB_SQUARE,
    &SNUB_HEXAGONAL,
];

/// The tiling called `name`, if there is one.
pub fn by_name(name: &str) -> Option<&'static UnitCell> {
    ALL.into_iter().find(|cell| cell.name == name)
}
