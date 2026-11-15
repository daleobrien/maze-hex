//! A maze: a tiling, and for each tile the groups of sides its tracks join.
//!
//! Every tile's sides are grouped into blocks. A block of one side is a dead
//! end; a block of two is a bend or a straight; three or more make a junction.
//! A tile may hold several blocks as long as they don't cross (pass-through).
//! Together the blocks form a single tree: one route between any two places.
//! Hub mode is the special case of one block per tile.

mod generate;
mod hub;
pub mod solution;

pub use generate::{GenerateOptions, Mode};
pub use hub::Algorithm;

use crate::rng::Rng;
use crate::tiling::Tiling;
use crate::track::clearance::{blocks_of, bridge_of, Clearance};
use crate::track::curve::V2;
use crate::track::face::FaceGeometry;
use crate::track::TrackStyle;

/// Two tracks crossing in one tile: indices into that tile's blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bridge {
    pub over: usize,
    pub under: usize,
}

/// A dead end: the tile it is in and the side it is entered by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cap {
    pub face: usize,
    pub side: usize,
}

#[derive(Debug)]
pub struct Maze {
    pub tiling: Tiling,
    /// Per face, its blocks; each block lists its sides anticlockwise.
    pub blocks: Vec<Vec<Vec<usize>>>,
    /// Per face, the bridge in it, if two of its tracks cross.
    pub bridges: Vec<Option<Bridge>>,
    /// Per edge, whether a track crosses it.
    pub open: Vec<bool>,
    pub entrance: Cap,
    pub exit: Cap,
    pub style: TrackStyle,
    /// The seed the maze grew from; the ink pen is seeded with it too, so one
    /// number repeats the whole drawing.
    pub seed: u64,
}

/// Everything that decides a maze, besides its tiling.
#[derive(Clone, Copy, Debug)]
pub struct MazeOptions {
    pub seed: u64,
    pub style: TrackStyle,
    pub generate: GenerateOptions,
}

impl Default for MazeOptions {
    fn default() -> Self {
        MazeOptions {
            seed: 1,
            style: TrackStyle::default(),
            generate: GenerateOptions::default(),
        }
    }
}

impl Maze {
    /// Grows a maze over `tiling`. Returns `None` if the tiling is too small
    /// to hold a track with two ends.
    pub fn generate(tiling: Tiling, options: &MazeOptions) -> Option<Maze> {
        // A separate stream from the tiling's, so changing one doesn't
        // reshuffle the other.
        let mut rng = Rng::new(options.seed ^ 0x6d61_7a65);
        let mut clearance =
            Clearance::new(options.style).with_crossings(options.generate.crossings > 0.0);
        let generated = match options.generate.mode {
            Mode::PassThrough => {
                generate::generate(&tiling, &mut clearance, &mut rng, &options.generate)
            }
            // Every single-block tile is drawable, so hub mode needs no clearance.
            Mode::Hub => hub::generate(&tiling, &mut rng, &options.generate),
        };

        let blocks: Vec<Vec<Vec<usize>>> = generated.labels.iter().map(|l| blocks_of(l)).collect();
        let mut open = vec![false; tiling.edges.len()];
        for (face, labels) in tiling.faces.iter().zip(&generated.labels) {
            for (&edge, &label) in face.edges.iter().zip(labels) {
                if label != 0 {
                    open[edge] = true;
                }
            }
        }

        // Which track of each bridge goes over: its own random numbers, so
        // choosing doesn't disturb the maze itself.
        let mut coin = Rng::new(options.seed ^ 0x6272_6964_6765);
        let bridges: Vec<Option<Bridge>> = generated
            .labels
            .iter()
            .map(|labels| {
                bridge_of(labels).map(|(a, b)| {
                    let (over, under) = if coin.chance(0.5) { (a, b) } else { (b, a) };
                    Bridge { over, under }
                })
            })
            .collect();

        let caps: Vec<Cap> = blocks
            .iter()
            .enumerate()
            .flat_map(|(face, bs)| {
                bs.iter()
                    .filter(|b| b.len() == 1)
                    .map(move |b| Cap { face, side: b[0] })
            })
            .collect();
        let (entrance, exit) = pick_ends(&tiling, &caps, options.style.cap_depth)?;
        Some(Maze {
            tiling,
            blocks,
            bridges,
            open,
            entrance,
            exit,
            style: options.style,
            seed: options.seed,
        })
    }

    /// The centre of a dead end's cap, where its marker goes.
    pub fn cap_centre(&self, cap: Cap) -> V2 {
        FaceGeometry::of(&self.tiling, cap.face).cap_centre(cap.side, self.style.cap_depth)
    }

    /// How many tiles hold no track.
    pub fn blank_tiles(&self) -> usize {
        self.blocks.iter().filter(|b| b.is_empty()).count()
    }
}

/// The dead ends nearest the bottom-left and top-right corners, as in Lubra.
fn pick_ends(tiling: &Tiling, caps: &[Cap], depth: f64) -> Option<(Cap, Cap)> {
    if caps.len() < 2 {
        return None;
    }
    let centre = |cap: &Cap| FaceGeometry::of(tiling, cap.face).cap_centre(cap.side, depth);
    let points: Vec<V2> = tiling.points.iter().map(|p| V2::from(p.to_f64())).collect();
    let min = points.iter().fold(V2::new(f64::MAX, f64::MAX), |m, p| {
        V2::new(m.x.min(p.x), m.y.min(p.y))
    });
    let max = points.iter().fold(V2::new(f64::MIN, f64::MIN), |m, p| {
        V2::new(m.x.max(p.x), m.y.max(p.y))
    });
    let nearest = |corner: V2, skip: Option<Cap>| {
        caps.iter()
            .filter(|&&c| Some(c) != skip)
            .min_by(|a, b| {
                centre(a)
                    .distance(corner)
                    .total_cmp(&centre(b).distance(corner))
            })
            .copied()
    };
    let entrance = nearest(min, None)?;
    let exit = nearest(max, Some(entrance))?;
    Some((entrance, exit))
}
