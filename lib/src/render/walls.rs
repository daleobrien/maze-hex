//! Walls chained across tiles into long strokes (PLAN §5.3).
//!
//! Every wall starts at one side point and ends at another. A side point on an
//! open edge is where one tile's wall ends and the neighbour's wall begins, so
//! the walls join up into closed loops. The outline of a tree-shaped track is
//! a single loop, so a whole maze is usually drawn as one line.

use std::collections::HashMap;

use crate::maze::Maze;
use crate::track::curve::Curve;
use crate::track::face::FaceGeometry;

/// A run of curves drawn in one go.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub curves: Vec<Curve>,
    /// Whether the stroke ends where it started.
    pub closed: bool,
}

/// A side point, named by its edge and the vertex of that edge it is nearer.
/// Both tiles sharing the edge give the same name, which floats could not.
type SidePoint = (usize, usize);

struct Piece {
    curves: Vec<Curve>,
    end: SidePoint,
}

/// Every wall of the maze, joined into as few strokes as possible.
pub fn wall_strokes(maze: &Maze) -> Vec<Stroke> {
    let tiling = &maze.tiling;
    let mut pieces = Vec::new();
    let mut starting_at: HashMap<SidePoint, usize> = HashMap::new();
    for (f, face) in tiling.faces.iter().enumerate() {
        let geometry = FaceGeometry::of(tiling, f);
        let n = face.vertices.len();
        for wall in geometry.walls(&maze.blocks[f], &maze.style) {
            let start = (
                face.edges[wall.from_side],
                face.vertices[(wall.from_side + 1) % n],
            );
            let end = (face.edges[wall.to_side], face.vertices[wall.to_side]);
            starting_at.insert(start, pieces.len());
            pieces.push(Piece {
                curves: wall.curves,
                end,
            });
        }
    }

    let mut used = vec![false; pieces.len()];
    let mut strokes = Vec::new();
    for first in 0..pieces.len() {
        if used[first] {
            continue;
        }
        let mut curves = Vec::new();
        let mut current = first;
        let closed = loop {
            used[current] = true;
            curves.extend_from_slice(&pieces[current].curves);
            match starting_at.get(&pieces[current].end) {
                Some(&next) if next == first => break true,
                Some(&next) if !used[next] => current = next,
                _ => break false,
            }
        };
        strokes.push(Stroke { curves, closed });
    }
    strokes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::MazeOptions;
    use crate::rng::Rng;
    use crate::tiling::source::{source_by_name, source_names};
    use crate::tiling::Region;

    #[test]
    fn walls_join_into_closed_loops_that_follow_on() {
        for name in source_names() {
            let source = source_by_name(name).expect("a known tiling");
            let tiling = source.build(Region::rectangle(12.0, 9.0), &mut Rng::new(2));
            let maze = Maze::generate(tiling, &MazeOptions::default()).expect("a maze");
            let strokes = wall_strokes(&maze);
            assert!(strokes.iter().all(|s| s.closed), "{name}");
            // A single tree has a single outline.
            assert_eq!(strokes.len(), 1, "{name}");
            for stroke in &strokes {
                for pair in stroke.curves.windows(2) {
                    assert!(pair[0].end().distance(pair[1].start()) < 1e-9, "{name}");
                }
            }
        }
    }
}
