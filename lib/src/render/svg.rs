//! Plain SVG output: even black walls, a red route and red end markers.
//!
//! Everything inside the flipped group is in maze coordinates scaled to
//! pixels, with `y` pointing up; only text is placed outside it, so it isn't
//! drawn upside down.

use std::fmt::Write;

use super::walls::wall_strokes;
use crate::maze::Maze;
use crate::track::curve::{Curve, V2};
use crate::track::face::FaceGeometry;

/// What to draw besides the walls.
#[derive(Clone, Copy, Debug, Default)]
pub struct SvgOptions {
    /// The route from entrance to exit, in red (`-S`).
    pub solution: bool,
    /// Faint tile outlines (`-B`).
    pub boundaries: bool,
    /// Each tile's number (`-N`).
    pub numbers: bool,
    /// A dot on every open port (`-T`).
    pub ports: bool,
}

/// Pixels per edge length.
const SCALE: f64 = 40.0;
const PADDING: f64 = 20.0;
const WALL: &str = "#1d1d1d";
const RED: &str = "#c0392b";
const INK: &str = "#2b2118";

/// Maps maze coordinates to the flipped group's pixels.
struct Frame {
    min: V2,
    width: f64,
    height: f64,
}

impl Frame {
    fn of(maze: &Maze) -> Frame {
        let points: Vec<V2> = maze
            .tiling
            .points
            .iter()
            .map(|p| V2::from(p.to_f64()))
            .collect();
        let fold = |f: fn(f64, f64) -> f64, start: f64| {
            points.iter().fold(V2::new(start, start), |m, p| {
                V2::new(f(m.x, p.x), f(m.y, p.y))
            })
        };
        let (min, max) = (fold(f64::min, f64::MAX), fold(f64::max, f64::MIN));
        Frame {
            min,
            width: (max.x - min.x) * SCALE + 2.0 * PADDING,
            height: (max.y - min.y) * SCALE + 2.0 * PADDING,
        }
    }

    fn at(&self, p: V2) -> V2 {
        V2::new(
            (p.x - self.min.x) * SCALE + PADDING,
            (p.y - self.min.y) * SCALE + PADDING,
        )
    }

    /// The same point on screen, `y` down, for text.
    fn screen(&self, p: V2) -> V2 {
        let q = self.at(p);
        V2::new(q.x, self.height - q.y)
    }

    /// SVG path data for curves joined end to end.
    fn path(&self, curves: &[Curve], closed: bool) -> String {
        let mut d = String::new();
        let Some(first) = curves.first() else {
            return d;
        };
        let s = self.at(first.start());
        let _ = write!(d, "M{:.2} {:.2}", s.x, s.y);
        for curve in curves {
            let e = self.at(curve.end());
            match *curve {
                Curve::Line { .. } => {
                    let _ = write!(d, "L{:.2} {:.2}", e.x, e.y);
                }
                Curve::Arc { radius, sweep, .. } => {
                    let r = radius * SCALE;
                    let large = u8::from(sweep.abs() > std::f64::consts::PI + 1e-9);
                    let positive = u8::from(sweep > 0.0);
                    let _ = write!(
                        d,
                        "A{r:.2} {r:.2} 0 {large} {positive} {:.2} {:.2}",
                        e.x, e.y
                    );
                }
                Curve::Cubic { c1, c2, .. } => {
                    let (c1, c2) = (self.at(c1), self.at(c2));
                    let _ = write!(
                        d,
                        "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
                        c1.x, c1.y, c2.x, c2.y, e.x, e.y
                    );
                }
            }
        }
        if closed {
            d.push('Z');
        }
        d
    }
}

/// The maze as a complete SVG document.
pub fn render(maze: &Maze, options: &SvgOptions) -> String {
    let frame = Frame::of(maze);
    let (w, h) = (frame.width, frame.height);
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w:.1} {h:.1}' width='{w:.0}' height='{h:.0}'>"
    );
    let _ = writeln!(svg, "<rect width='100%' height='100%' fill='white'/>");
    let _ = writeln!(svg, "<g transform='translate(0,{h:.2}) scale(1,-1)'>");

    if options.boundaries {
        let _ = writeln!(svg, "<g fill='none' stroke='#cfcfcf' stroke-width='0.8'>");
        for face in &maze.tiling.faces {
            let points: Vec<String> = face
                .vertices
                .iter()
                .map(|&v| {
                    let p = frame.at(V2::from(maze.tiling.points[v].to_f64()));
                    format!("{:.2},{:.2}", p.x, p.y)
                })
                .collect();
            let _ = writeln!(svg, "<polygon points='{}'/>", points.join(" "));
        }
        svg.push_str("</g>\n");
    }

    if options.ports {
        let _ = writeln!(svg, "<g fill='#2a7ab9'>");
        for (edge, open) in maze.tiling.edges.iter().zip(&maze.open) {
            if *open {
                let [a, b] = edge.vertices;
                let (pa, pb) = (
                    maze.tiling.points[a].to_f64(),
                    maze.tiling.points[b].to_f64(),
                );
                let mid = frame.at(V2::new((pa.0 + pb.0) / 2.0, (pa.1 + pb.1) / 2.0));
                let _ = writeln!(svg, "<circle cx='{:.2}' cy='{:.2}' r='2'/>", mid.x, mid.y);
            }
        }
        svg.push_str("</g>\n");
    }

    let walls: Vec<String> = wall_strokes(maze)
        .iter()
        .map(|s| frame.path(&s.curves, s.closed))
        .collect();
    let _ = writeln!(
        svg,
        "<path fill='none' stroke='{WALL}' stroke-width='2.5' stroke-linejoin='round' stroke-linecap='round' d='{}'/>",
        walls.join("")
    );

    if options.solution {
        let route = solution_curves(maze);
        let _ = writeln!(
            svg,
            "<path fill='none' stroke='{RED}' stroke-opacity='0.85' stroke-width='{:.2}' stroke-linejoin='round' stroke-linecap='round' d='{}'/>",
            maze.style.width * SCALE * 0.3,
            frame.path(&route, false)
        );
    }

    for cap in [maze.entrance, maze.exit] {
        let c = frame.at(maze.cap_centre(cap));
        let _ = writeln!(
            svg,
            "<circle cx='{:.2}' cy='{:.2}' r='{:.2}' fill='{RED}' stroke='{INK}' stroke-width='1.5'/>",
            c.x,
            c.y,
            maze.style.width * SCALE * 0.38
        );
    }
    svg.push_str("</g>\n");

    if options.numbers {
        let _ = writeln!(
            svg,
            "<g font-family='sans-serif' font-size='9' fill='#7a7a7a' text-anchor='middle' dominant-baseline='central'>"
        );
        for f in 0..maze.tiling.faces.len() {
            let c = frame.screen(FaceGeometry::of(&maze.tiling, f).centre());
            let _ = writeln!(svg, "<text x='{:.2}' y='{:.2}'>{f}</text>", c.x, c.y);
        }
        svg.push_str("</g>\n");
    }
    svg.push_str("</svg>\n");
    svg
}

/// The centreline of the route from the entrance's cap to the exit's.
fn solution_curves(maze: &Maze) -> Vec<Curve> {
    maze.solution()
        .iter()
        .filter_map(|step| {
            let face = FaceGeometry::of(&maze.tiling, step.face);
            match (step.from, step.to) {
                (Some(a), Some(b)) => Some(face.centreline(a, b)),
                (None, Some(b)) => Some(face.centreline_to_cap(b, &maze.style).reversed()),
                (Some(a), None) => Some(face.centreline_to_cap(a, &maze.style)),
                (None, None) => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::MazeOptions;
    use crate::rng::Rng;
    use crate::tiling::source::source_by_name;
    use crate::tiling::Region;

    fn hex_maze(seed: u64) -> Maze {
        let source = source_by_name("hexagonal").expect("hexagonal exists");
        let tiling = source.build(Region::rectangle(12.0, 9.0), &mut Rng::new(seed));
        let options = MazeOptions {
            seed,
            ..MazeOptions::default()
        };
        Maze::generate(tiling, &options).expect("a maze")
    }

    #[test]
    fn the_same_maze_gives_the_same_svg() {
        let options = SvgOptions {
            solution: true,
            boundaries: true,
            numbers: true,
            ports: true,
        };
        let a = render(&hex_maze(4), &options);
        assert_eq!(a, render(&hex_maze(4), &options));
        assert!(a.starts_with("<svg") && a.trim_end().ends_with("</svg>"));
    }

    /// FNV-1a, so the pinned output below needs no hashing crate.
    fn fnv(text: &str) -> u64 {
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[test]
    fn plain_output_is_unchanged() {
        // Taken from the plain renderer before ink existed, so adding ink
        // cannot quietly alter `-P`.
        let options = SvgOptions {
            solution: true,
            boundaries: true,
            numbers: true,
            ports: true,
        };
        let svg = render(&hex_maze(4), &options);
        assert_eq!((svg.len(), fnv(&svg)), (17709, 0x1ff0_aff1_c4cc_6b7a));
    }

    #[test]
    fn the_solution_is_one_unbroken_line() {
        let maze = hex_maze(5);
        let route = solution_curves(&maze);
        assert!(!route.is_empty());
        for pair in route.windows(2) {
            assert!(pair[0].end().distance(pair[1].start()) < 1e-9);
        }
        assert!(route[0].start().distance(maze.cap_centre(maze.entrance)) < 1e-9);
    }
}
