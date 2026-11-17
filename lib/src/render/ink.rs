//! The hand-drawn "ink" pen, copied from Lubra's `lib/src/renders/ink.rs`
//! (which took it from maze-rs's `-I`) and adapted to maze-hex.
//!
//! Lubra flattens every tile's fragment and wobbles each one. Here the walls
//! are already chained into long closed strokes (`walls.rs`), so the caller
//! samples each stroke into a polyline and this pen only has to nudge the
//! points, vary its width and write the paths. Every point of a closed loop is
//! visited once, and the nudge depends only on where the point is on the page,
//! so the wobble is the same from both sides of a tile edge.
//!
//! All coordinates here are pixels. The sizes below are Lubra's, scaled down
//! because a maze-hex edge is 40 pixels where a Lubra tile is 120.
//!
//! Unlike the plain renderer, nothing is shared between tiles: the SVG is a
//! few long paths, but each point is written out.

use std::f64::consts::PI;
use std::fmt::Write as _;

use super::number::push_fixed;

/// The ink, the pencil used for the route and end markers, and the paper
/// inside a track.
const INK: &str = "#2b2118";
pub const PENCIL: &str = "#c0392b";
pub const PAPER: &str = "#ffffff";
/// The entrance and exit dots: Lubra's red, solid and without an outline, so
/// the three engines on maze.do.id.au share one look.
pub const DOT: &str = "#E02020";

/// The middle pen width in pixels (the plain walls are 2.5).
pub const PEN_WIDTH: f64 = 3.0;

/// The pen widths, as fractions of the middle one.
const WIDTHS: [f64; 3] = [0.8, 1.0, 1.22];

/// How many pieces of a wall are drawn with one width before the pen changes.
const CHUNK: usize = 5;

/// How far a point may be nudged and the size of a wobble, in pixels. A track
/// is about 17 pixels wide, so walls stay well clear of each other.
const AMPLITUDE: f64 = 1.1;
const FEATURE: f64 = 16.0;

/// The longest piece a wall is flattened into, in pixels. Short enough that the
/// wobble reads as a smooth wander and arcs stay round.
pub const STEP: f64 = 3.0;

/// Different noise fields from the one seed.
const SALT_X: u64 = 0x5851_F42D_4C95_7F2D;
const SALT_Y: u64 = 0x1405_7B7E_F767_814F;
const SALT_WIDTH: u64 = 0x2545_F491_4F6C_DD1D;
const SALT_PENCIL: [u64; 3] = [
    0x9E37_79B9_7F4A_7C15,
    0xBF58_476D_1CE4_E5B9,
    0x94D0_49BB_1331_11EB,
];

pub type Point = (f64, f64);

/// A value in [-1, 1) for each lattice point.
fn lattice(ix: i64, iy: i64, seed: u64) -> f64 {
    // splitmix64's finaliser over the mixed coordinates
    let mut z = seed
        ^ (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
}

/// Smooth value noise in [-1, 1].
fn noise(x: f64, y: f64, seed: u64) -> f64 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (ix, iy) = (x0 as i64, y0 as i64);
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = lattice(ix, iy, seed) * (1.0 - u) + lattice(ix + 1, iy, seed) * u;
    let bottom = lattice(ix, iy + 1, seed) * (1.0 - u) + lattice(ix + 1, iy + 1, seed) * u;
    top * (1.0 - v) + bottom * v
}

/// The pen: where it puts a point and how wide it is there.
pub struct Pen {
    seed: u64,
}

impl Pen {
    /// A pen whose wobble is fixed by `seed` (the maze's seed).
    pub fn new(seed: u64) -> Pen {
        Pen { seed }
    }

    /// `p` nudged by the noise field `salt` picks (0 for the walls).
    fn wobble(&self, (x, y): Point, salt: u64) -> Point {
        let (nx, ny) = (x / FEATURE, y / FEATURE);
        (
            x + AMPLITUDE * noise(nx, ny, self.seed ^ salt ^ SALT_X),
            y + AMPLITUDE * noise(nx, ny, self.seed ^ salt ^ SALT_Y),
        )
    }

    /// Which of the [`WIDTHS`] the pen has at `p`.
    fn width_at(&self, (x, y): Point) -> usize {
        let n = noise(
            x / (FEATURE * 2.0),
            y / (FEATURE * 2.0),
            self.seed ^ SALT_WIDTH,
        );
        if n < -0.2 {
            0
        } else if n > 0.2 {
            2
        } else {
            1
        }
    }
}

/// Appends a polyline as path data.
fn push_polyline(out: &mut String, points: &[Point]) {
    for (k, &(x, y)) in points.iter().enumerate() {
        out.push(if k == 0 { 'M' } else { 'L' });
        push_fixed(out, x, 1);
        out.push(' ');
        push_fixed(out, y, 1);
    }
}

fn wobbled(pen: &Pen, points: &[Point], salt: u64) -> Vec<Point> {
    points.iter().map(|&p| pen.wobble(p, salt)).collect()
}

/// The insides of closed loops in the paper colour, wobbled exactly as the
/// walls are so no paper shows outside the ink.
///
/// A maze's walls are one loop whose inside is the track. `evenodd` makes a
/// loop nested in another a hole, which is right if a maze ever has islands,
/// and costs nothing for a single loop.
pub fn fill_markup(loops: &[Vec<Point>], pen: &Pen) -> String {
    let d: String = loops
        .iter()
        .map(|points| outline_path(points, pen))
        .collect();
    format!("<path fill='{PAPER}' fill-rule='evenodd' stroke='none' d='{d}'/>\n")
}

/// Path data for a closed outline (each point once), wobbled exactly as the
/// walls are, so a fill or mask made from it meets the ink.
pub fn outline_path(points: &[Point], pen: &Pen) -> String {
    let mut d = String::new();
    push_polyline(&mut d, &wobbled(pen, points, 0));
    d.push('Z');
    d
}

/// Closed loops (each point once; the loop is closed here) or open lines in ink,
/// the pen width changing every few pieces without the pen lifting.
pub fn wall_markup(lines: &[(Vec<Point>, bool)], pen: &Pen) -> String {
    let mut paths: [String; 3] = Default::default();
    for (points, closed) in lines {
        let mut points = wobbled(pen, points, 0);
        if *closed {
            // Repeat the first point so the last piece runs back to the start.
            if let Some(&first) = points.first() {
                points.push(first);
            }
        }
        // Consecutive chunks share a point, so the pen never lifts.
        let mut start = 0;
        while start + 1 < points.len() {
            let end = (start + CHUNK).min(points.len() - 1);
            let width = pen.width_at(points[(start + end) / 2]);
            push_polyline(&mut paths[width], &points[start..=end]);
            start = end;
        }
    }
    let mut out = String::new();
    for (factor, d) in WIDTHS.iter().zip(&paths) {
        if !d.is_empty() {
            let _ = writeln!(
                out,
                "<path fill='none' stroke='{INK}' stroke-width='{:.2}' stroke-linecap='round' stroke-linejoin='round' d='{d}'/>",
                PEN_WIDTH * factor
            );
        }
    }
    out
}

/// A route pencilled in `colour` and gone over twice, each time a little
/// differently, as in Lubra. The solution uses [`PENCIL`] red; the player's
/// trail and previews use their own colours.
pub fn pencil_markup(route: &[Point], pen: &Pen, colour: &str) -> String {
    let mut out = String::new();
    for (salt, width) in SALT_PENCIL[..2]
        .iter()
        .zip([PEN_WIDTH * 2.2, PEN_WIDTH * 1.5])
    {
        let mut d = String::new();
        push_polyline(&mut d, &wobbled(pen, route, *salt));
        let _ = writeln!(
            out,
            "<path fill='none' stroke='{colour}' stroke-opacity='0.45' stroke-width='{width:.2}' stroke-linecap='round' stroke-linejoin='round' d='{d}'/>"
        );
    }
    out
}

/// The route where it passes under a bridge: pencilled once, lightly, over
/// the track on top, as Lubra (and maze-rs before it) does.
pub fn faint_pencil_markup(routes: &[Vec<Point>], pen: &Pen, colour: &str) -> String {
    let mut d = String::new();
    for route in routes {
        push_polyline(&mut d, &wobbled(pen, route, SALT_PENCIL[2]));
    }
    if d.is_empty() {
        return d;
    }
    format!(
        "<path fill='none' stroke='{colour}' stroke-opacity='0.18' stroke-width='{:.2}' stroke-linecap='round' stroke-linejoin='round' d='{d}'/>\n",
        PEN_WIDTH * 1.5
    )
}

/// Solid red discs, wobbled like the walls, for the entrance and
/// exit.
pub fn dot_markup(centres: &[Point], radius: f64, pen: &Pen) -> String {
    let n = (2.0 * PI * radius / STEP).ceil().max(12.0) as usize;
    let mut d = String::new();
    for &(cx, cy) in centres {
        let ring: Vec<Point> = (0..n)
            .map(|k| {
                let angle = 2.0 * PI * k as f64 / n as f64;
                (cx + radius * angle.cos(), cy + radius * angle.sin())
            })
            .collect();
        push_polyline(&mut d, &wobbled(pen, &ring, 0));
        d.push('Z');
    }
    format!("<path fill='{DOT}' fill-opacity='1' stroke='none' d='{d}'/>\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Point> {
        vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]
    }

    #[test]
    fn noise_is_smooth_and_bounded() {
        for k in 0..1000 {
            let (x, y) = (k as f64 * 0.137, k as f64 * 0.071);
            let n = noise(x, y, 7);
            assert!((-1.0..=1.0).contains(&n));
            assert!((noise(x + 1e-6, y, 7) - n).abs() < 1e-3);
        }
    }

    #[test]
    fn a_point_wobbles_the_same_wherever_it_is_drawn_from() {
        // The wobble depends only on where a point is, so two strokes that
        // pass through one point agree on it.
        let pen = Pen::new(3);
        assert_eq!(pen.wobble((12.5, 40.0), 0), pen.wobble((12.5, 40.0), 0));
        let (x, y) = pen.wobble((12.5, 40.0), 0);
        assert!((x - 12.5).abs() <= AMPLITUDE && (y - 40.0).abs() <= AMPLITUDE);
    }

    #[test]
    fn the_seed_changes_the_drawing() {
        let a = wall_markup(&[(square(), true)], &Pen::new(5));
        assert_eq!(a, wall_markup(&[(square(), true)], &Pen::new(5)));
        assert_ne!(a, wall_markup(&[(square(), true)], &Pen::new(6)));
    }

    #[test]
    fn a_closed_loop_runs_back_to_its_start() {
        let pen = Pen::new(1);
        let d = wall_markup(&[(square(), true)], &pen);
        let (x, y) = pen.wobble((0.0, 0.0), 0);
        let start = format!("M{x:.1} {y:.1}");
        let end = format!("L{x:.1} {y:.1}'");
        assert!(d.contains(&start) && d.contains(&end), "{d}");
    }

    #[test]
    fn paper_comes_from_the_same_wobble_as_the_ink() {
        let pen = Pen::new(2);
        let fill = fill_markup(&[square()], &pen);
        let (x, y) = pen.wobble((30.0, 0.0), 0);
        assert!(fill.contains(&format!("L{x:.1} {y:.1}")), "{fill}");
    }
}
