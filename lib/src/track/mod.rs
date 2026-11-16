//! Tracks inside one tile: the curves, the walls and the clearance check.

pub mod clearance;
pub mod curve;
pub mod face;

/// How tracks are drawn, in edge lengths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackStyle {
    /// The track's width.
    pub width: f64,
    /// The least distance between two walls.
    pub gap: f64,
    /// The least distance from a wall to a side it doesn't cross.
    pub margin: f64,
    /// How far a dead end reaches into its tile, from the side to the centre
    /// of its round cap (never past the tile's centre). Short dead ends leave
    /// room for other tracks in the same tile.
    pub cap_depth: f64,
    /// Whether every dead end opens into a ring, as Lubra's entrance and exit
    /// do. The entrance and exit always have one where it fits.
    pub circles: bool,
    /// The ring's outer radius.
    pub ring_radius: f64,
    /// The radius of the curve that blends each wall into the ring, so the
    /// outline never has a corner.
    pub ring_blend: f64,
}

impl TrackStyle {
    /// The same style with rings at every dead end.
    pub fn with_circles(self) -> Self {
        TrackStyle {
            circles: true,
            ..self
        }
    }
}

impl Default for TrackStyle {
    fn default() -> Self {
        TrackStyle {
            width: 0.4,
            gap: 0.1,
            margin: 0.06,
            cap_depth: 0.3,
            circles: false,
            ring_radius: 0.32,
            ring_blend: 0.08,
        }
    }
}
