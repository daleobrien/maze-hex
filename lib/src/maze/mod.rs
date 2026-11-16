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
use crate::track::face::{FaceGeometry, Ring};
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
    /// Whether the entrance and exit have room for a ring.
    pub ringed: [bool; 2],
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
        // The entrance and exit open into rings, as in Lubra. A ring needs more
        // room than a plain dead end, so a dead end whose tile still draws
        // cleanly with one is preferred if it is nearly as close to the
        // corner; otherwise the nearest dead end keeps its plain cap.
        let mut ringed = Clearance::new(options.style.with_circles())
            .with_crossings(options.generate.crossings > 0.0);
        let roomy: Vec<bool> = caps
            .iter()
            .map(|cap| {
                options.style.circles
                    || ringed.allows(tiling.faces[cap.face].kind, &generated.labels[cap.face])
            })
            .collect();
        let (entrance, exit) = pick_ends(&tiling, &caps, &roomy, options.style.cap_depth)?;
        Some(Maze {
            tiling,
            blocks,
            bridges,
            open,
            entrance: entrance.0,
            exit: exit.0,
            ringed: [entrance.1, exit.1],
            style: options.style,
            seed: options.seed,
        })
    }

    /// The centre of a dead end's ring or cap, where its marker goes.
    pub fn cap_centre(&self, cap: Cap) -> V2 {
        FaceGeometry::of(&self.tiling, cap.face)
            .dead_end_centre(cap.side, &self.style_for(cap.face))
    }

    /// How tracks are drawn in tile `face`: the maze's style, with rings at
    /// the dead ends of the entrance and exit tiles.
    pub fn style_for(&self, face: usize) -> TrackStyle {
        let ends = [self.entrance, self.exit];
        if ends
            .iter()
            .zip(self.ringed)
            .any(|(cap, ringed)| ringed && cap.face == face)
        {
            self.style.with_circles()
        } else {
            self.style
        }
    }

    /// The ring the entrance or exit opens into, if one fits its tile.
    pub fn ring(&self, cap: Cap) -> Option<Ring> {
        FaceGeometry::of(&self.tiling, cap.face).ring(cap.side, &self.style_for(cap.face))
    }

    /// How many tiles hold no track.
    pub fn blank_tiles(&self) -> usize {
        self.blocks.iter().filter(|b| b.is_empty()).count()
    }
}

/// How much further from its corner a dead end with room for a ring may be
/// than the nearest dead end, and still be chosen instead, in edge lengths.
const RING_SLACK: f64 = 2.5;

/// The dead ends nearest the bottom-left and top-right corners, as in Lubra,
/// each with whether it gets a ring: one that has room for a ring (`roomy`)
/// wins if it is within [`RING_SLACK`] of the nearest.
fn pick_ends(
    tiling: &Tiling,
    caps: &[Cap],
    roomy: &[bool],
    depth: f64,
) -> Option<((Cap, bool), (Cap, bool))> {
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
    let nearest = |corner: V2, skip: Option<Cap>, only_roomy: bool| {
        caps.iter()
            .zip(roomy)
            .filter(|&(&c, &r)| Some(c) != skip && (r || !only_roomy))
            .map(|(&c, &r)| (c, r, centre(&c).distance(corner)))
            .min_by(|a, b| a.2.total_cmp(&b.2))
    };
    let pick = |corner: V2, skip: Option<Cap>| {
        let any = nearest(corner, skip, false)?;
        match nearest(corner, skip, true) {
            Some((cap, _, distance)) if distance <= any.2 + RING_SLACK => Some((cap, true)),
            _ => Some((any.0, any.1)),
        }
    };
    let entrance = pick(min, None)?;
    let exit = pick(max, Some(entrance.0))?;
    Some((entrance, exit))
}
