//! Hub mode (docs/design.md §4.2): a spanning tree over tiles.
//!
//! Every tile holds one block of all its open sides, so the maze is a spanning
//! tree of the dual graph: a node per tile, a link per interior edge. A tile
//! with one open side is a dead end. Because a single block is always
//! drawable, none of this needs the clearance table, and a spanning tree
//! visits every tile, so no tile is left blank.

use super::generate::{GenerateOptions, Generated, UnionFind};
use crate::rng::Rng;
use crate::tiling::Tiling;

/// How the tree is grown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Algorithm {
    /// Loop-erased random walks: a uniform spanning tree, with many short
    /// dead ends.
    #[default]
    Wilson,
    /// Depth-first with an explicit stack: long winding corridors, few dead ends.
    Backtracker,
    /// Joins random links that don't close a loop: many short branches.
    Kruskal,
    /// Grows from a random link of the frontier: many short branches.
    Prim,
}

impl Algorithm {
    pub const ALL: [Algorithm; 4] = [
        Algorithm::Wilson,
        Algorithm::Backtracker,
        Algorithm::Kruskal,
        Algorithm::Prim,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Wilson => "wilson",
            Algorithm::Backtracker => "backtracker",
            Algorithm::Kruskal => "kruskal",
            Algorithm::Prim => "prim",
        }
    }

    pub fn from_name(name: &str) -> Option<Algorithm> {
        Algorithm::ALL.into_iter().find(|a| a.name() == name)
    }
}

/// A link of the dual graph as seen from one tile.
#[derive(Clone, Copy)]
struct Link {
    /// The side of this tile the link leaves by.
    side: usize,
    edge: usize,
    to: usize,
}

/// For each tile, its links to neighbouring tiles.
fn links(tiling: &Tiling) -> Vec<Vec<Link>> {
    tiling
        .faces
        .iter()
        .enumerate()
        .map(|(f, face)| {
            face.edges
                .iter()
                .enumerate()
                .filter_map(|(side, &edge)| {
                    let across = tiling.edges[edge].across(f)?;
                    Some(Link {
                        side,
                        edge,
                        to: across.face,
                    })
                })
                .collect()
        })
        .collect()
}

pub(super) fn generate(tiling: &Tiling, rng: &mut Rng, options: &GenerateOptions) -> Generated {
    let links = links(tiling);
    let open = if links.is_empty() {
        vec![false; tiling.edges.len()]
    } else {
        match options.algorithm {
            Algorithm::Wilson => wilson(tiling, &links, rng),
            Algorithm::Backtracker => backtracker(tiling, &links, rng, options.straightness),
            Algorithm::Kruskal => kruskal(tiling, rng),
            Algorithm::Prim => prim(tiling, &links, rng),
        }
    };
    // One block per tile holding every open side.
    let labels = tiling
        .faces
        .iter()
        .map(|face| face.edges.iter().map(|&e| u8::from(open[e])).collect())
        .collect();
    Generated { labels }
}

/// Wilson's algorithm: from each tile not yet in the tree, wander at random
/// until the tree is hit, then keep the walk with its loops erased. Each
/// tile remembers only its latest exit, which is what erases the loops.
fn wilson(tiling: &Tiling, links: &[Vec<Link>], rng: &mut Rng) -> Vec<bool> {
    let faces = links.len();
    let mut open = vec![false; tiling.edges.len()];
    let mut in_tree = vec![false; faces];
    in_tree[rng.below(faces)] = true;
    let mut exit: Vec<Option<Link>> = vec![None; faces];
    for start in 0..faces {
        let mut at = start;
        while !in_tree[at] {
            let choices = &links[at];
            // A tile with no links can't be reached from the rest; the tiling
            // is one piece, so this only guards a one-tile tiling.
            if choices.is_empty() {
                break;
            }
            let link = choices[rng.below(choices.len())];
            exit[at] = Some(link);
            at = link.to;
        }
        let mut at = start;
        while !in_tree[at] {
            in_tree[at] = true;
            let Some(link) = exit[at] else { break };
            open[link.edge] = true;
            at = link.to;
        }
    }
    open
}

/// Recursive backtracker, run with an explicit stack because mazes can be
/// large. With `straightness`, leaving by the side opposite the one a tile
/// was entered by is weighted up, as in the pass-through generator.
fn backtracker(
    tiling: &Tiling,
    links: &[Vec<Link>],
    rng: &mut Rng,
    straightness: f64,
) -> Vec<bool> {
    let faces = links.len();
    let mut open = vec![false; tiling.edges.len()];
    let mut seen = vec![false; faces];
    // The side each tile was entered by, to find the one opposite it.
    let mut entered: Vec<Option<usize>> = vec![None; faces];
    let start = rng.below(faces);
    seen[start] = true;
    let mut stack = vec![start];
    while let Some(&at) = stack.last() {
        let fresh: Vec<Link> = links[at].iter().copied().filter(|l| !seen[l.to]).collect();
        if fresh.is_empty() {
            stack.pop();
            continue;
        }
        let sides = tiling.faces[at].kind.sides();
        let straight = entered[at]
            .filter(|_| sides.is_multiple_of(2))
            .map(|side| (side + sides / 2) % sides);
        let weights: Vec<f64> = fresh
            .iter()
            .map(|l| {
                if Some(l.side) == straight {
                    1.0 + 4.0 * straightness
                } else {
                    1.0
                }
            })
            .collect();
        let mut pick = rng.unit() * weights.iter().sum::<f64>();
        let mut chosen = fresh[fresh.len() - 1];
        for (link, weight) in fresh.iter().zip(&weights) {
            if pick < *weight {
                chosen = *link;
                break;
            }
            pick -= weight;
        }
        open[chosen.edge] = true;
        seen[chosen.to] = true;
        // The neighbour's side of the shared edge.
        entered[chosen.to] = tiling.edges[chosen.edge]
            .across(at)
            .map(|across| across.side);
        stack.push(chosen.to);
    }
    open
}

/// Randomised Kruskal: take the interior edges in random order and keep each
/// one that joins two tiles not yet connected.
fn kruskal(tiling: &Tiling, rng: &mut Rng) -> Vec<bool> {
    let mut open = vec![false; tiling.edges.len()];
    let mut order: Vec<usize> = (0..tiling.edges.len())
        .filter(|&e| tiling.edges[e].is_interior())
        .collect();
    rng.shuffle(&mut order);
    let mut sets = UnionFind::new(tiling.faces.len());
    for edge in order {
        if let [Some(a), Some(b)] = tiling.edges[edge].faces {
            if sets.find(a.face) != sets.find(b.face) {
                sets.union(a.face, b.face);
                open[edge] = true;
            }
        }
    }
    open
}

/// Randomised Prim: keep a frontier of links out of the tree and add a
/// random one each time.
fn prim(tiling: &Tiling, links: &[Vec<Link>], rng: &mut Rng) -> Vec<bool> {
    let faces = links.len();
    let mut open = vec![false; tiling.edges.len()];
    let mut in_tree = vec![false; faces];
    let start = rng.below(faces);
    in_tree[start] = true;
    let mut frontier: Vec<Link> = links[start].clone();
    while !frontier.is_empty() {
        let link = frontier.swap_remove(rng.below(frontier.len()));
        // The far tile may have joined by another link since this was queued.
        if in_tree[link.to] {
            continue;
        }
        in_tree[link.to] = true;
        open[link.edge] = true;
        frontier.extend(links[link.to].iter().filter(|l| !in_tree[l.to]));
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::{Maze, MazeOptions, Mode};
    use crate::tiling::source::{source_by_name, source_names};
    use crate::tiling::Region;
    use crate::track::clearance::Clearance;
    use crate::track::TrackStyle;

    fn hub_maze(name: &str, seed: u64, algorithm: Algorithm, w: f64, h: f64) -> Maze {
        let source = source_by_name(name).expect("a known tiling");
        let tiling = source.build(Region::rectangle(w, h), &mut Rng::new(seed));
        let options = MazeOptions {
            seed,
            generate: GenerateOptions {
                mode: Mode::Hub,
                algorithm,
                ..GenerateOptions::default()
            },
            ..MazeOptions::default()
        };
        Maze::generate(tiling, &options).expect("room for a maze")
    }

    fn dead_ends(maze: &Maze) -> usize {
        maze.blocks
            .iter()
            .flatten()
            .filter(|b| b.len() == 1)
            .count()
    }

    #[test]
    fn every_algorithm_spans_every_tiling_with_a_route() {
        let mut clearance = Clearance::new(TrackStyle::default());
        for name in source_names() {
            for algorithm in Algorithm::ALL {
                for seed in 1..4 {
                    let maze = hub_maze(name, seed, algorithm, 14.0, 10.0);
                    let what = format!("{name} {algorithm:?} seed {seed}");
                    assert!(maze.is_one_tree(), "{what}");
                    assert_eq!(maze.blank_tiles(), 0, "{what}");
                    assert!(maze.blocks.iter().all(|b| b.len() == 1), "{what}");
                    assert!(maze.solution().len() >= 2, "{what}");
                    // One link fewer than tiles: a spanning tree.
                    let links = maze.open.iter().filter(|&&o| o).count();
                    assert_eq!(links + 1, maze.tiling.faces.len(), "{what}");
                    for (face, blocks) in maze.blocks.iter().enumerate() {
                        let mut labels = vec![0u8; maze.tiling.faces[face].kind.sides()];
                        for &side in &blocks[0] {
                            labels[side] = 1;
                        }
                        assert!(
                            clearance.allows(maze.tiling.faces[face].kind, &labels),
                            "{what} face {face}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_hub_maze() {
        for algorithm in Algorithm::ALL {
            let a = hub_maze("hexagonal", 9, algorithm, 14.0, 10.0);
            let b = hub_maze("hexagonal", 9, algorithm, 14.0, 10.0);
            assert_eq!(a.blocks, b.blocks, "{algorithm:?}");
            let c = hub_maze("hexagonal", 10, algorithm, 14.0, 10.0);
            assert_ne!(a.blocks, c.blocks, "{algorithm:?}");
        }
    }

    #[test]
    fn the_backtracker_makes_fewer_dead_ends_than_wilson() {
        let total = |algorithm| -> usize {
            (1..6)
                .map(|seed| dead_ends(&hub_maze("hexagonal", seed, algorithm, 30.0, 20.0)))
                .sum()
        };
        let (wilson, backtracker) = (total(Algorithm::Wilson), total(Algorithm::Backtracker));
        assert!(
            backtracker * 10 < wilson * 8,
            "backtracker {backtracker}, wilson {wilson}"
        );
    }

    #[test]
    fn algorithm_names_round_trip() {
        for algorithm in Algorithm::ALL {
            assert_eq!(Algorithm::from_name(algorithm.name()), Some(algorithm));
        }
        assert_eq!(Algorithm::from_name("growing-tree"), None);
    }
}
