//! The route from the entrance to the exit, and a check that the maze is a
//! single tree.
//!
//! The maze is a graph with two kinds of node: ports (open edges) and blocks
//! (groups of sides joined inside a tile). Each block links to the port on
//! each of its sides.

use std::collections::VecDeque;

use super::{Cap, Maze};

/// One tile on the route: entered by `from` and left by `to`. `None` means the
/// route starts or ends at a dead end's cap in this tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub face: usize,
    pub from: Option<usize>,
    pub to: Option<usize>,
}

struct Graph {
    /// For each node, its neighbours. Ports come first, then blocks.
    links: Vec<Vec<usize>>,
    /// For each block node: (face, block index).
    blocks: Vec<(usize, usize)>,
    ports: usize,
}

fn graph(maze: &Maze) -> Graph {
    let ports = maze.tiling.edges.len();
    let mut links: Vec<Vec<usize>> = vec![Vec::new(); ports];
    let mut blocks = Vec::new();
    for (face, face_blocks) in maze.blocks.iter().enumerate() {
        for (index, block) in face_blocks.iter().enumerate() {
            let node = links.len();
            links.push(Vec::new());
            blocks.push((face, index));
            for &side in block {
                let edge = maze.tiling.faces[face].edges[side];
                links[node].push(edge);
                links[edge].push(node);
            }
        }
    }
    Graph {
        links,
        blocks,
        ports,
    }
}

impl Graph {
    fn block_node(&self, maze: &Maze, cap: Cap) -> Option<usize> {
        let index = maze.blocks[cap.face]
            .iter()
            .position(|b| b == &[cap.side])?;
        self.blocks
            .iter()
            .position(|&b| b == (cap.face, index))
            .map(|k| self.ports + k)
    }
}

impl Maze {
    /// The tiles from the entrance to the exit, in order.
    pub fn solution(&self) -> Vec<Step> {
        let graph = graph(self);
        let (Some(start), Some(goal)) = (
            graph.block_node(self, self.entrance),
            graph.block_node(self, self.exit),
        ) else {
            return Vec::new();
        };
        let mut came_from = vec![usize::MAX; graph.links.len()];
        came_from[start] = start;
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            if node == goal {
                break;
            }
            for &next in &graph.links[node] {
                if came_from[next] == usize::MAX {
                    came_from[next] = node;
                    queue.push_back(next);
                }
            }
        }
        if came_from[goal] == usize::MAX {
            return Vec::new();
        }
        let mut path = vec![goal];
        while *path.last().unwrap_or(&start) != start {
            let last = path[path.len() - 1];
            path.push(came_from[last]);
        }
        path.reverse();

        // Turn block, port, block, ... into steps through each block's tile.
        let side_of = |face: usize, port: usize| {
            self.tiling.faces[face]
                .edges
                .iter()
                .position(|&e| e == port)
        };
        path.iter()
            .enumerate()
            .filter(|(_, &node)| node >= graph.ports)
            .map(|(i, &node)| {
                let (face, _) = graph.blocks[node - graph.ports];
                let from = i.checked_sub(1).and_then(|j| side_of(face, path[j]));
                let to = path.get(i + 1).and_then(|&port| side_of(face, port));
                Step { face, from, to }
            })
            .collect()
    }

    /// Whether the tracks form exactly one tree: connected, with no loops.
    pub fn is_one_tree(&self) -> bool {
        let graph = graph(self);
        // Only open ports and blocks take part.
        let nodes: Vec<usize> = (0..graph.links.len())
            .filter(|&n| !graph.links[n].is_empty())
            .collect();
        let links: usize = nodes.iter().map(|&n| graph.links[n].len()).sum::<usize>() / 2;
        if nodes.is_empty() || links + 1 != nodes.len() {
            return false;
        }
        let mut seen = vec![false; graph.links.len()];
        let mut stack = vec![nodes[0]];
        seen[nodes[0]] = true;
        let mut count = 0;
        while let Some(n) = stack.pop() {
            count += 1;
            for &next in &graph.links[n] {
                if !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        count == nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use crate::maze::{Maze, MazeOptions};
    use crate::rng::Rng;
    use crate::tiling::source::{source_by_name, source_names};
    use crate::tiling::Region;

    fn maze(name: &str, seed: u64) -> Maze {
        let source = source_by_name(name).expect("a known tiling");
        let tiling = source.build(Region::rectangle(14.0, 10.0), &mut Rng::new(seed));
        let options = MazeOptions {
            seed,
            ..MazeOptions::default()
        };
        Maze::generate(tiling, &options).expect("room for a maze")
    }

    #[test]
    fn every_tiling_gives_one_tree_with_a_route_through_it() {
        for name in source_names() {
            for seed in 1..4 {
                let maze = maze(name, seed);
                assert!(maze.is_one_tree(), "{name} seed {seed}");
                let route = maze.solution();
                assert!(route.len() >= 2, "{name} seed {seed}");
                let first = route[0];
                let last = route[route.len() - 1];
                assert_eq!((first.face, first.from), (maze.entrance.face, None));
                assert_eq!((last.face, last.to), (maze.exit.face, None));
                // Each step leaves by an open edge into the next step's tile.
                for pair in route.windows(2) {
                    let edge = maze.tiling.faces[pair[0].face].edges
                        [pair[0].to.expect("leaves by a side")];
                    assert!(maze.open[edge]);
                    assert_eq!(
                        maze.tiling.faces[pair[1].face].edges
                            [pair[1].from.expect("enters by a side")],
                        edge
                    );
                }
            }
        }
    }

    #[test]
    fn grown_mixes_give_one_tree_with_a_route() {
        use crate::tiling::grow::RandomMix;
        use crate::tiling::TilingSource;
        for weights in [
            [1.0, 1.0, 1.0],
            [3.0, 1.0, 0.0],
            [1.0, 3.0, 1.0],
            [1.0, 0.0, 1.0],
        ] {
            for seed in 1..5 {
                let tiling =
                    RandomMix { weights }.build(Region::rectangle(20.0, 14.0), &mut Rng::new(seed));
                let options = MazeOptions {
                    seed,
                    ..MazeOptions::default()
                };
                let maze = Maze::generate(tiling, &options).expect("room for a maze");
                assert!(maze.is_one_tree(), "{weights:?} seed {seed}");
                assert!(maze.solution().len() >= 2, "{weights:?} seed {seed}");
            }
        }
    }

    #[test]
    fn pentagon_mixes_give_one_tree_with_a_route() {
        use crate::maze::generate::Mode;
        use crate::tiling::pentagon::merge_pentagons;
        for mode in [Mode::PassThrough, Mode::Hub] {
            for seed in 1..5 {
                let region = Region::rectangle(20.0, 14.0);
                let base = source_by_name("random")
                    .expect("random exists")
                    .build(region, &mut Rng::new(seed));
                let tiling = merge_pentagons(&base, 0.6, &mut Rng::new(seed ^ 0x70656e74));
                let pentagons = tiling
                    .faces
                    .iter()
                    .filter(|f| f.kind == crate::tiling::PolygonKind::Pentagon)
                    .count();
                assert!(pentagons > 3, "seed {seed}: {pentagons} pentagons");
                let mut options = MazeOptions {
                    seed,
                    ..MazeOptions::default()
                };
                options.generate.mode = mode;
                let maze = Maze::generate(tiling, &options).expect("room for a maze");
                assert!(maze.is_one_tree(), "{mode:?} seed {seed}");
                assert!(maze.solution().len() >= 2, "{mode:?} seed {seed}");
            }
        }
    }

    /// A maze with crossings off, which repeats the mazes made before
    /// bridges existed.
    fn maze_without_crossings(name: &str, seed: u64) -> Maze {
        let source = source_by_name(name).expect("a known tiling");
        let tiling = source.build(Region::rectangle(14.0, 10.0), &mut Rng::new(seed));
        let mut options = MazeOptions {
            seed,
            ..MazeOptions::default()
        };
        options.generate.crossings = 0.0;
        options.generate.junctions = 1.0;
        Maze::generate(tiling, &options).expect("room for a maze")
    }

    /// FNV-1a over the debug text of every tile's blocks.
    fn fingerprint(maze: &Maze) -> u64 {
        format!("{:?}", maze.blocks)
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
            })
    }

    #[test]
    fn pass_through_mazes_are_unchanged_by_new_modes() {
        // Values taken before hub mode existed, so adding a mode can't quietly
        // reshuffle the default maze for a seed.
        let pinned = [
            ("hexagonal", 8780091197363656907u64),
            ("square", 10172740568377355009),
            ("triangular", 4386330192581049373),
            ("snub-square", 16616791671595646887),
            ("triangle-hexagon", 186290123505539694),
        ];
        for (name, want) in pinned {
            assert_eq!(
                fingerprint(&maze_without_crossings(name, 3)),
                want,
                "{name}"
            );
        }
    }

    #[test]
    fn bridges_appear_only_in_tiles_of_four_or_more_sides() {
        for name in source_names() {
            let source = source_by_name(name).expect("a known tiling");
            let tiling = source.build(Region::rectangle(14.0, 10.0), &mut Rng::new(2));
            let mut options = MazeOptions {
                seed: 2,
                ..MazeOptions::default()
            };
            options.generate.crossings = 0.8;
            let maze = Maze::generate(tiling, &options).expect("room for a maze");
            assert!(maze.is_one_tree(), "{name}");
            for (f, bridge) in maze.bridges.iter().enumerate() {
                if let Some(bridge) = bridge {
                    assert!(maze.tiling.faces[f].kind.sides() >= 4, "{name}");
                    assert_eq!(maze.blocks[f][bridge.over].len(), 2, "{name}");
                    assert_eq!(maze.blocks[f][bridge.under].len(), 2, "{name}");
                }
            }
            // Where every neighbour of a hexagon is a triangle (3.6.3.6), a
            // bridge's ends can't fit beside the triangles' own tracks and
            // are cut back, so only some tilings are sure to keep bridges.
            let bridges = maze.bridges.iter().flatten().count();
            if ["hexagonal", "square", "rhombitrihexagonal"].contains(&name) {
                assert!(bridges > 0, "{name}: no bridges");
            }
            if maze.tiling.faces.iter().all(|f| f.kind.sides() < 4) {
                assert_eq!(bridges, 0, "{name}");
            }
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_maze() {
        let a = maze("hexagonal", 9);
        let b = maze("hexagonal", 9);
        assert_eq!(a.blocks, b.blocks);
        assert_ne!(a.blocks, maze("hexagonal", 10).blocks);
    }

    #[test]
    fn hexagon_mazes_have_tiles_with_two_tracks_and_few_blanks() {
        let maze = maze("hexagonal", 1);
        let doubles = maze
            .blocks
            .iter()
            .filter(|b| b.iter().filter(|x| x.len() > 1).count() > 1)
            .count();
        assert!(doubles > 0);
        assert!(
            maze.blank_tiles() * 10 < maze.tiling.faces.len(),
            "{} blank",
            maze.blank_tiles()
        );
    }
}
