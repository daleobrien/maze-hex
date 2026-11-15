//! Tilings: tiles (faces), the edges between them and the vertices they share.
//!
//! A tiling is built from a [`UnitCell`] repeated over a rectangle and then
//! clipped: a tile is kept when its centre lies inside the rectangle, and only
//! the largest group of tiles joined edge to edge is kept, so the maze has no
//! stray islands. Edges and vertices are shared between neighbouring tiles by
//! their exact coordinates.

pub mod catalogue;

use std::collections::{HashMap, VecDeque};

use crate::geometry::{Direction, Point};

/// The regular polygons maze-hex uses. All have edges of length 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolygonKind {
    Triangle,
    Square,
    Hexagon,
}

impl PolygonKind {
    /// How many sides (and edges a track may leave by).
    pub const fn sides(self) -> usize {
        match self {
            PolygonKind::Triangle => 3,
            PolygonKind::Square => 4,
            PolygonKind::Hexagon => 6,
        }
    }

    /// The exterior angle in 30° steps: how far to turn at each corner.
    pub const fn turn_steps(self) -> u8 {
        (12 / self.sides()) as u8
    }

    /// The interior angle in 30° steps.
    pub const fn interior_steps(self) -> u8 {
        6 - self.turn_steps()
    }
}

/// One tile of a unit cell: its shape, its first vertex and the direction of
/// its first edge. Its vertices run anticlockwise from `start`.
#[derive(Clone, Copy, Debug)]
pub struct Prototile {
    pub kind: PolygonKind,
    pub start: Point,
    pub direction: Direction,
}

impl Prototile {
    /// The tile's vertices, anticlockwise, after moving it by `offset`.
    pub fn vertices(&self, offset: Point) -> Vec<Point> {
        let mut vertices = Vec::with_capacity(self.kind.sides());
        let mut p = self.start + offset;
        let mut d = self.direction;
        for _ in 0..self.kind.sides() {
            vertices.push(p);
            p = p + d.unit();
            d = d.turn(self.kind.turn_steps());
        }
        vertices
    }
}

/// A periodic tiling: tiles repeated along the lattice vectors `a` and `b`.
#[derive(Debug)]
pub struct UnitCell {
    /// The name used on the command line, e.g. `hexagonal`.
    pub name: &'static str,
    /// The vertex configuration, e.g. `3.4.6.4`.
    pub notation: &'static str,
    pub a: Point,
    pub b: Point,
    pub tiles: &'static [Prototile],
}

/// Where an edge sits on a face: the face and which of its sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceSide {
    pub face: usize,
    pub side: usize,
}

/// An edge between two vertices, with the one or two faces it borders.
#[derive(Clone, Debug)]
pub struct Edge {
    pub vertices: [usize; 2],
    pub faces: [Option<FaceSide>; 2],
}

impl Edge {
    /// Whether a tile lies on both sides, so a track can cross it.
    pub fn is_interior(&self) -> bool {
        self.faces.iter().all(Option::is_some)
    }

    /// The face on the other side from `face`, and which side of it this is.
    pub fn across(&self, face: usize) -> Option<FaceSide> {
        match self.faces {
            [Some(a), b] if a.face == face => b,
            [a, Some(b)] if b.face == face => a,
            _ => None,
        }
    }
}

/// A tile: its shape, its vertices anticlockwise and the edge on each side.
/// Side `i` runs from `vertices[i]` to `vertices[i + 1]`.
#[derive(Clone, Debug)]
pub struct Face {
    pub kind: PolygonKind,
    pub vertices: Vec<usize>,
    pub edges: Vec<usize>,
}

/// A finished tiling, clipped to a region.
#[derive(Debug, Default)]
pub struct Tiling {
    pub points: Vec<Point>,
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
}

/// The rectangle a tiling fills, in edge lengths, from the origin.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub width: f64,
    pub height: f64,
}

impl Tiling {
    /// Repeats `cell` over `region` and keeps the tiles whose centres are inside.
    pub fn build(cell: &UnitCell, region: Region) -> Tiling {
        let mut builder = Builder::default();
        for offset in lattice_offsets(cell, region) {
            for prototile in cell.tiles {
                let vertices = prototile.vertices(offset);
                if centre_is_inside(&vertices, region) {
                    builder.add_face(prototile.kind, &vertices);
                }
            }
        }
        builder.finish().largest_component()
    }

    /// The centre of a face, for drawing.
    pub fn centre(&self, face: usize) -> (f64, f64) {
        let face = &self.faces[face];
        let n = face.vertices.len() as f64;
        let (sx, sy) = face
            .vertices
            .iter()
            .map(|&v| self.points[v].to_f64())
            .fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
        (sx / n, sy / n)
    }

    /// The faces across each interior edge of `face`.
    pub fn neighbours(&self, face: usize) -> impl Iterator<Item = FaceSide> + '_ {
        self.faces[face]
            .edges
            .iter()
            .filter_map(move |&e| self.edges[e].across(face))
    }

    /// Keeps only the largest set of faces joined edge to edge.
    fn largest_component(self) -> Tiling {
        let mut component = vec![usize::MAX; self.faces.len()];
        let mut best = (0, 0); // (size, component id)
        for start in 0..self.faces.len() {
            if component[start] != usize::MAX {
                continue;
            }
            let mut size = 0;
            let mut queue = VecDeque::from([start]);
            component[start] = start;
            while let Some(face) = queue.pop_front() {
                size += 1;
                for next in self.neighbours(face) {
                    if component[next.face] == usize::MAX {
                        component[next.face] = start;
                        queue.push_back(next.face);
                    }
                }
            }
            if size > best.0 {
                best = (size, start);
            }
        }
        if best.0 == self.faces.len() {
            return self;
        }
        let mut builder = Builder::default();
        for (face, &id) in self.faces.iter().zip(&component) {
            if id == best.1 {
                let vertices: Vec<Point> = face.vertices.iter().map(|&v| self.points[v]).collect();
                builder.add_face(face.kind, &vertices);
            }
        }
        builder.finish()
    }
}

/// Every lattice offset `i·a + j·b` whose cell could reach into `region`.
fn lattice_offsets(cell: &UnitCell, region: Region) -> Vec<Point> {
    let (ax, ay) = cell.a.to_f64();
    let (bx, by) = cell.b.to_f64();
    let det = ax * by - ay * bx;
    // Lattice coordinates of the region's corners bound the cells needed.
    let corners = [
        (0.0, 0.0),
        (region.width, 0.0),
        (0.0, region.height),
        (region.width, region.height),
    ];
    let to_lattice = |(x, y): (f64, f64)| ((x * by - y * bx) / det, (ax * y - ay * x) / det);
    let (mut i_min, mut i_max, mut j_min, mut j_max) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for corner in corners {
        let (i, j) = to_lattice(corner);
        i_min = i_min.min(i);
        i_max = i_max.max(i);
        j_min = j_min.min(j);
        j_max = j_max.max(j);
    }
    // A margin of two cells covers tiles that start outside their own cell.
    let range = |lo: f64, hi: f64| (lo.floor() as i64 - 2)..=(hi.ceil() as i64 + 2);
    let mut offsets = Vec::new();
    for i in range(i_min, i_max) {
        for j in range(j_min, j_max) {
            offsets.push(cell.a.times(i) + cell.b.times(j));
        }
    }
    offsets
}

fn centre_is_inside(vertices: &[Point], region: Region) -> bool {
    let n = vertices.len() as f64;
    let (sx, sy) = vertices
        .iter()
        .map(|p| p.to_f64())
        .fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
    let (x, y) = (sx / n, sy / n);
    (0.0..=region.width).contains(&x) && (0.0..=region.height).contains(&y)
}

/// Joins faces into a tiling, sharing vertices and edges by exact position.
#[derive(Default)]
struct Builder {
    tiling: Tiling,
    point_ids: HashMap<Point, usize>,
    edge_ids: HashMap<(usize, usize), usize>,
}

impl Builder {
    fn point(&mut self, p: Point) -> usize {
        *self.point_ids.entry(p).or_insert_with(|| {
            self.tiling.points.push(p);
            self.tiling.points.len() - 1
        })
    }

    fn add_face(&mut self, kind: PolygonKind, vertices: &[Point]) {
        let face = self.tiling.faces.len();
        let ids: Vec<usize> = vertices.iter().map(|&p| self.point(p)).collect();
        let mut edges = Vec::with_capacity(ids.len());
        for side in 0..ids.len() {
            let (from, to) = (ids[side], ids[(side + 1) % ids.len()]);
            let key = (from.min(to), from.max(to));
            let here = Some(FaceSide { face, side });
            let edge = match self.edge_ids.get(&key) {
                Some(&edge) => {
                    // The neighbour walked this edge the other way round.
                    debug_assert!(self.tiling.edges[edge].faces[1].is_none());
                    self.tiling.edges[edge].faces[1] = here;
                    edge
                }
                None => {
                    self.tiling.edges.push(Edge {
                        vertices: [from, to],
                        faces: [here, None],
                    });
                    self.edge_ids.insert(key, self.tiling.edges.len() - 1);
                    self.tiling.edges.len() - 1
                }
            };
            edges.push(edge);
        }
        self.tiling.faces.push(Face {
            kind,
            vertices: ids,
            edges,
        });
    }

    fn finish(self) -> Tiling {
        self.tiling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGION: Region = Region {
        width: 16.0,
        height: 12.0,
    };

    fn polygon_area(points: &[(f64, f64)]) -> f64 {
        let n = points.len();
        (0..n)
            .map(|i| {
                let (x0, y0) = points[i];
                let (x1, y1) = points[(i + 1) % n];
                x0 * y1 - x1 * y0
            })
            .sum::<f64>()
            / 2.0
    }

    #[test]
    fn each_unit_cell_is_covered_exactly_once() {
        for cell in catalogue::ALL {
            let (ax, ay) = cell.a.to_f64();
            let (bx, by) = cell.b.to_f64();
            let cell_area = (ax * by - ay * bx).abs();
            let tiles_area: f64 = cell
                .tiles
                .iter()
                .map(|t| {
                    let points: Vec<_> = t
                        .vertices(Point::ORIGIN)
                        .iter()
                        .map(|p| p.to_f64())
                        .collect();
                    let area = polygon_area(&points);
                    assert!(area > 0.0, "{}: a tile runs clockwise", cell.name);
                    area
                })
                .sum();
            assert!(
                (cell_area - tiles_area).abs() < 1e-9,
                "{}: cell {cell_area} vs tiles {tiles_area}",
                cell.name
            );
        }
    }

    #[test]
    fn every_edge_has_length_one_exactly() {
        for cell in catalogue::ALL {
            let tiling = Tiling::build(cell, REGION);
            for edge in &tiling.edges {
                let [a, b] = edge.vertices;
                let v = tiling.points[b] - tiling.points[a];
                assert!(v.as_unit_direction().is_some(), "{}: {v:?}", cell.name);
            }
        }
    }

    #[test]
    fn edges_join_at_most_two_faces_and_faces_have_one_edge_per_side() {
        for cell in catalogue::ALL {
            let tiling = Tiling::build(cell, REGION);
            assert!(
                tiling.faces.len() > 20,
                "{}: only {} faces",
                cell.name,
                tiling.faces.len()
            );
            for face in &tiling.faces {
                assert_eq!(face.edges.len(), face.kind.sides());
            }
            for (e, edge) in tiling.edges.iter().enumerate() {
                for side in edge.faces.iter().flatten() {
                    assert_eq!(tiling.faces[side.face].edges[side.side], e);
                }
            }
        }
    }

    #[test]
    fn interior_vertices_are_surrounded_by_exactly_360_degrees() {
        for cell in catalogue::ALL {
            let tiling = Tiling::build(cell, REGION);
            let mut angle = vec![0u32; tiling.points.len()];
            let mut on_boundary = vec![false; tiling.points.len()];
            for face in &tiling.faces {
                for &v in &face.vertices {
                    angle[v] += u32::from(face.kind.interior_steps());
                }
            }
            for edge in tiling.edges.iter().filter(|e| !e.is_interior()) {
                for v in edge.vertices {
                    on_boundary[v] = true;
                }
            }
            for v in 0..tiling.points.len() {
                if on_boundary[v] {
                    assert!(angle[v] < 12, "{}: boundary vertex over 360°", cell.name);
                } else {
                    assert_eq!(angle[v], 12, "{}: vertex {v}", cell.name);
                }
            }
        }
    }

    #[test]
    fn the_tiling_is_one_piece() {
        for cell in catalogue::ALL {
            let tiling = Tiling::build(cell, REGION);
            let mut seen = vec![false; tiling.faces.len()];
            let mut stack = vec![0];
            seen[0] = true;
            while let Some(face) = stack.pop() {
                for next in tiling.neighbours(face) {
                    if !seen[next.face] {
                        seen[next.face] = true;
                        stack.push(next.face);
                    }
                }
            }
            assert!(seen.iter().all(|&s| s), "{}", cell.name);
        }
    }

    #[test]
    fn hexagonal_vertices_meet_three_tiles() {
        let tiling = Tiling::build(&catalogue::HEXAGONAL, REGION);
        let mut count = vec![0; tiling.points.len()];
        for face in &tiling.faces {
            for &v in &face.vertices {
                count[v] += 1;
            }
        }
        assert!(count.iter().all(|&c| (1..=3).contains(&c)));
        assert!(count.contains(&3));
    }
}
