//! maze-hex: mazes on tilings of triangles, squares and hexagons whose edges
//! all have the same length. Tracks only ever join a tile at the middle of an
//! edge. See `PLAN.md` for where this is going.

pub mod debug_svg;
pub mod geometry;
pub mod maze;
pub mod rng;
pub mod tiling;
pub mod track;
