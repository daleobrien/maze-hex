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
    start: SidePoint,
    end: SidePoint,
}

/// Every wall of the maze, joined into as few strokes as possible. With
/// `skip_bridges`, the walls of each bridge's upper track are left out (they
/// are drawn last, by [`bridges`]), so some strokes end where a bridge begins.
pub fn wall_strokes(maze: &Maze, skip_bridges: bool) -> Vec<Stroke> {
    let tiling = &maze.tiling;
    let mut pieces = Vec::new();
    let mut starting_at: HashMap<SidePoint, usize> = HashMap::new();
    for (f, face) in tiling.faces.iter().enumerate() {
        let geometry = FaceGeometry::of(tiling, f);
        let n = face.vertices.len();
        let over = maze.bridges[f].filter(|_| skip_bridges).map(|b| b.over);
        let blocks: Vec<Vec<usize>> = maze.blocks[f]
            .iter()
            .enumerate()
            .filter(|&(k, _)| Some(k) != over)
            .map(|(_, b)| b.clone())
            .collect();
        for wall in geometry.walls(&blocks, &maze.style) {
            let start = (
                face.edges[wall.from_side],
                face.vertices[(wall.from_side + 1) % n],
            );
            let end = (face.edges[wall.to_side], face.vertices[wall.to_side]);
            starting_at.insert(start, pieces.len());
            pieces.push(Piece {
                curves: wall.curves,
                start,
                end,
            });
        }
    }

    // Open strokes are walked from their first piece: one no other piece
    // leads into. Closed loops can start anywhere.
    let ends: std::collections::HashSet<SidePoint> = pieces.iter().map(|p| p.end).collect();
    let mut order: Vec<usize> = (0..pieces.len())
        .filter(|&i| !ends.contains(&pieces[i].start))
        .collect();
    order.extend(0..pieces.len());

    let mut used = vec![false; pieces.len()];
    let mut strokes = Vec::new();
    for first in order {
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

/// The upper track of a bridge: the outline of its band, to paint over the
/// track beneath, and its two walls.
#[derive(Clone, Debug)]
pub struct BridgeShape {
    pub outline: Vec<Curve>,
    pub walls: Vec<Stroke>,
}

/// Every bridge's upper track, to be drawn after everything else in its tile.
pub fn bridges(maze: &Maze) -> Vec<BridgeShape> {
    let mut shapes = Vec::new();
    for (f, bridge) in maze.bridges.iter().enumerate() {
        let Some(bridge) = bridge else {
            continue;
        };
        let geometry = FaceGeometry::of(&maze.tiling, f);
        let block = &maze.blocks[f][bridge.over];
        let (p, q) = (block[0], block[1]);
        let there = geometry.wall(p, q, &maze.style);
        let back = geometry.wall(q, p, &maze.style);
        let width = maze.style.width;
        // Round the band: one wall, across side q, the other wall, across p.
        let mut outline = there.curves.clone();
        outline.push(Curve::Line {
            from: geometry.side_point(q, false, width),
            to: geometry.side_point(q, true, width),
        });
        outline.extend(back.curves.iter().copied());
        outline.push(Curve::Line {
            from: geometry.side_point(p, false, width),
            to: geometry.side_point(p, true, width),
        });
        shapes.push(BridgeShape {
            outline,
            walls: vec![
                Stroke {
                    curves: there.curves,
                    closed: false,
                },
                Stroke {
                    curves: back.curves,
                    closed: false,
                },
            ],
        });
    }
    shapes
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
            let strokes = wall_strokes(&maze, false);
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
