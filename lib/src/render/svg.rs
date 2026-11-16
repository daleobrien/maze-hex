//! SVG output: hand-drawn ink by default, or with `plain` even black walls. Either
//! way a red route and red end markers.
//!
//! Everything inside the flipped group is in maze coordinates scaled to
//! pixels, with `y` pointing up; only text is placed outside it, so it isn't
//! drawn upside down.

use std::fmt::Write;

use super::ink::{self, Pen, Point, PENCIL};
use super::walls::{bridges, wall_strokes, BridgeShape, Stroke};
use crate::maze::solution::Step;
use crate::maze::{Cap, Maze};
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
    /// Even black walls instead of hand-drawn ink (`-P`).
    pub plain: bool,
    /// An empty `<g id='overlay'></g>` as the last child of the flipped group,
    /// so a page can put markup in the maze's own drawing units (see
    /// [`Drawing`]) without redrawing the maze. Off by default.
    pub overlay_group: bool,
}

/// Pixels per edge length.
const SCALE: f64 = 40.0;
const PADDING: f64 = 20.0;
const WALL: &str = "#1d1d1d";
const RED: &str = PENCIL;

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

    /// A wall stroke sampled into pixel points for the ink pen. A closed
    /// stroke does not repeat its first point, so each point is visited once.
    fn polyline(&self, stroke: &Stroke) -> Vec<Point> {
        let mut points = self.sampled(&stroke.curves);
        if stroke.closed && points.len() > 1 {
            let (first, last) = (points[0], points[points.len() - 1]);
            if (first.0 - last.0).hypot(first.1 - last.1) < 1e-6 {
                points.pop();
            }
        }
        points
    }

    /// Curves joined end to end as pixel points no more than `ink::STEP` apart.
    fn sampled(&self, curves: &[Curve]) -> Vec<Point> {
        let mut points: Vec<Point> = Vec::new();
        for curve in curves {
            for p in curve.sample(ink::STEP / SCALE) {
                let q = self.at(p);
                // A curve starts where the last one ended; keep that point once.
                let same = points
                    .last()
                    .is_some_and(|&(x, y)| (x - q.x).hypot(y - q.y) < 1e-6);
                if !same {
                    points.push((q.x, q.y));
                }
            }
        }
        points
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

/// How a maze is laid out on the page, for drawing on top of it afterwards.
///
/// A page that puts markup in the `overlay_group` (see [`SvgOptions`]) draws in
/// the same units as the maze: pixels, 40 per edge length with padding, `y`
/// pointing up. `Drawing` converts maze coordinates to those units and pencils
/// routes with the maze's own pen, so a trail looks like the solution does.
pub struct Drawing {
    frame: Frame,
    pen: Pen,
}

impl Drawing {
    /// The layout and pen of `maze`, the same ones `render` uses.
    pub fn of(maze: &Maze) -> Drawing {
        Drawing {
            frame: Frame::of(maze),
            pen: Pen::new(maze.seed),
        }
    }

    /// A point of the maze (in edge lengths) in drawing units.
    pub fn point(&self, p: V2) -> (f64, f64) {
        let q = self.frame.at(p);
        (q.x, q.y)
    }

    /// The tiles `steps` pass through, pencilled in `colour` as the solution is:
    /// twice over, faint where the route passes under a bridge and drawn on top
    /// of a bridge where it goes over. Steps ending short of a dead end (the
    /// player standing at a junction) stop at their side; only the exit's step
    /// runs on into its ring. `id` names the mask that hides the track under
    /// a bridge, so it must differ between routes drawn in one document.
    pub fn pencil(&self, maze: &Maze, steps: &[Step], colour: &str, id: &str) -> String {
        let shapes = bridges(maze);
        let outlines: Vec<String> = shapes.iter().map(|b| self.outline(b, false)).collect();
        let route = Route::of(maze, steps);
        self.pencil_route(&shapes, &outlines, &route, colour, id, false)
    }

    /// The outline of a bridge's upper band as path data.
    fn outline(&self, bridge: &BridgeShape, plain: bool) -> String {
        if plain {
            self.frame.path(&bridge.outline, true)
        } else {
            let mut outline = self.frame.sampled(&bridge.outline);
            outline.pop(); // the outline ends where it started
            ink::outline_path(&outline, &self.pen)
        }
    }

    /// A route in pencil (or plain colour), with the bands of the bridges it
    /// passes under cut out and a faint trace across them instead.
    fn pencil_route(
        &self,
        shapes: &[BridgeShape],
        outlines: &[String],
        route: &Route,
        colour: &str,
        id: &str,
        plain: bool,
    ) -> String {
        let (frame, pen) = (&self.frame, &self.pen);
        let (w, h) = (frame.width, frame.height);
        let mut svg = String::new();
        if route.curves.is_empty() {
            return svg;
        }
        // The route is drawn once, with the bands of the bridges it passes
        // under cut out of it; drawing it twice would leave darker blotches
        // where the semi-transparent strokes overlap.
        let hidden: Vec<&str> = shapes
            .iter()
            .zip(outlines)
            .filter(|(bridge, _)| route.passes_under(bridge.face))
            .map(|(_, outline)| outline.as_str())
            .collect();
        if !hidden.is_empty() {
            let _ = writeln!(
                svg,
                "<mask id='{id}' maskUnits='userSpaceOnUse' x='0' y='0' width='{w:.1}' height='{h:.1}'><rect width='{w:.1}' height='{h:.1}' fill='white'/><path fill='black' d='{}'/></mask>",
                hidden.join("")
            );
            let _ = writeln!(svg, "<g mask='url(#{id})'>");
        }
        if plain {
            let _ = writeln!(
                svg,
                "<path fill='none' stroke='{colour}' stroke-opacity='0.85' stroke-width='{:.2}' stroke-linejoin='round' stroke-linecap='round' d='{}'/>",
                route.width * SCALE * 0.3,
                frame.path(&route.curves, false)
            );
        } else {
            svg.push_str(&ink::pencil_markup(
                &frame.sampled(&route.curves),
                pen,
                colour,
            ));
        }
        if !hidden.is_empty() {
            svg.push_str("</g>\n");
            // A route can cross one bridge both ways. Its way over was cut out
            // with the rest of the band, so it is put back on top.
            for (_, piece) in route.over.iter().filter(|(f, _)| route.passes_under(*f)) {
                if plain {
                    let _ = writeln!(
                        svg,
                        "<path fill='none' stroke='{colour}' stroke-opacity='0.85' stroke-width='{:.2}' stroke-linejoin='round' stroke-linecap='round' d='{}'/>",
                        route.width * SCALE * 0.3,
                        frame.path(std::slice::from_ref(piece), false)
                    );
                } else {
                    let points = frame.sampled(std::slice::from_ref(piece));
                    svg.push_str(&ink::pencil_markup(&points, pen, colour));
                }
            }
        }

        // Under a bridge the route is still there, just hidden: a faint
        // trace across the track on top, as in Lubra.
        if plain {
            for (_, piece) in &route.under {
                let _ = writeln!(
                    svg,
                    "<path fill='none' stroke='{colour}' stroke-opacity='0.25' stroke-width='{:.2}' stroke-linecap='round' d='{}'/>",
                    route.width * SCALE * 0.3,
                    frame.path(std::slice::from_ref(piece), false)
                );
            }
        } else {
            let lines: Vec<Vec<Point>> = route
                .under
                .iter()
                .map(|(_, piece)| frame.sampled(std::slice::from_ref(piece)))
                .collect();
            svg.push_str(&ink::faint_pencil_markup(&lines, pen, colour));
        }
        svg
    }
}

/// The maze as a complete SVG document.
pub fn render(maze: &Maze, options: &SvgOptions) -> String {
    let drawing = Drawing::of(maze);
    let frame = &drawing.frame;
    let (w, h) = (frame.width, frame.height);
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w:.1} {h:.1}' width='{w:.0}' height='{h:.0}'>"
    );
    // No background, as in Lubra and maze-rs: the page shows through, and the
    // tracks are filled white, so all three look alike on maze.do.id.au.
    let _ = writeln!(svg, "<g transform='translate(0,{h:.2}) scale(1,-1)'>");

    // Walls without the bridges' upper tracks, which go on last.
    let strokes = wall_strokes(maze, true);
    // The pen is seeded from the maze, so one seed repeats the whole drawing.
    let pen = &drawing.pen;
    // The fill needs whole loops, so it takes every wall.
    let loops: Vec<Vec<Point>> = wall_strokes(maze, false)
        .iter()
        .filter(|s| s.closed)
        .map(|s| frame.polyline(s))
        .collect();
    if !options.plain {
        // Each closed wall stroke is the outline of one tree of track, so its
        // inside is the track and is painted white before anything else.
        svg.push_str(&ink::fill_markup(&loops, pen));
    }

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

    if options.plain {
        let walls: Vec<String> = strokes
            .iter()
            .map(|s| frame.path(&s.curves, s.closed))
            .collect();
        let _ = writeln!(
            svg,
            "<path fill='none' stroke='{WALL}' stroke-width='2.5' stroke-linejoin='round' stroke-linecap='round' d='{}'/>",
            walls.join("")
        );
    } else {
        let lines: Vec<(Vec<Point>, bool)> = strokes
            .iter()
            .map(|s| (frame.polyline(s), s.closed))
            .collect();
        svg.push_str(&ink::wall_markup(&lines, pen));
    }

    // Each bridge's upper track: paper over the track beneath, then its
    // walls. The outlines are kept to cut the route out of them below.
    let shapes = bridges(maze);
    let outlines: Vec<String> = shapes
        .iter()
        .map(|bridge| drawing.outline(bridge, options.plain))
        .collect();
    let paper = if options.plain { "white" } else { ink::PAPER };
    for (bridge, outline) in shapes.iter().zip(&outlines) {
        let _ = writeln!(svg, "<path fill='{paper}' stroke='none' d='{outline}'/>");
        if options.plain {
            let walls: Vec<String> = bridge
                .walls
                .iter()
                .map(|s| frame.path(&s.curves, false))
                .collect();
            let _ = writeln!(
                svg,
                "<path fill='none' stroke='{WALL}' stroke-width='2.5' stroke-linejoin='round' stroke-linecap='round' d='{}'/>",
                walls.join("")
            );
        } else {
            let lines: Vec<(Vec<Point>, bool)> = bridge
                .walls
                .iter()
                .map(|s| (frame.sampled(&s.curves), false))
                .collect();
            svg.push_str(&ink::wall_markup(&lines, pen));
        }
    }

    if options.solution {
        let route = Route::of(maze, &maze.solution());
        svg.push_str(&drawing.pencil_route(
            &shapes,
            &outlines,
            &route,
            RED,
            "under",
            options.plain,
        ));
    }

    // The entrance and exit: a red dot in the middle of the ring, as in
    // Lubra, or a smaller one in the cap where no ring fits.
    for cap in [maze.entrance, maze.exit] {
        let c = frame.at(maze.cap_centre(cap));
        let radius = match maze.ring(cap) {
            Some(ring) => ring.radius * 0.5 * SCALE,
            None => maze.style.width * SCALE * 0.38,
        };
        if options.plain {
            let _ = writeln!(
                svg,
                "<circle cx='{:.2}' cy='{:.2}' r='{radius:.2}' fill='{}'/>",
                c.x,
                c.y,
                ink::DOT
            );
        } else {
            svg.push_str(&ink::dot_markup(&[(c.x, c.y)], radius, pen));
        }
    }
    if options.overlay_group {
        svg.push_str("<g id='overlay'></g>\n");
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

/// A route through the maze as curves, and which tiles it passes a bridge in.
struct Route {
    /// The centreline, piece by piece, in order.
    curves: Vec<Curve>,
    /// The pieces that pass under a bridge, with their tiles.
    under: Vec<(usize, Curve)>,
    /// The pieces that pass over a bridge, with their tiles.
    over: Vec<(usize, Curve)>,
    /// The track width, for the plain pencil.
    width: f64,
}

impl Route {
    fn of(maze: &Maze, steps: &[Step]) -> Route {
        let mut under = Vec::new();
        let mut over = Vec::new();
        for step in steps {
            let (Some(bridge), Some(from), Some(to)) =
                (maze.bridges[step.face], step.from, step.to)
            else {
                continue;
            };
            let on = |block: usize| {
                let block = &maze.blocks[step.face][block];
                block.contains(&from) && block.contains(&to)
            };
            let curve = FaceGeometry::of(&maze.tiling, step.face).centreline(from, to);
            if on(bridge.under) {
                under.push((step.face, curve));
            } else if on(bridge.over) {
                over.push((step.face, curve));
            }
        }
        Route {
            curves: step_curves(maze, steps),
            under,
            over,
            width: maze.style.width,
        }
    }

    fn passes_under(&self, face: usize) -> bool {
        self.under.iter().any(|(f, _)| *f == face)
    }
}

/// The centreline through each step's tile. A step with a side and no way
/// out runs on into its dead end only at the exit: elsewhere it is where a
/// player stands, at the side they came in by, and there is nothing to draw.
fn step_curves(maze: &Maze, steps: &[Step]) -> Vec<Curve> {
    steps
        .iter()
        .filter_map(|step| {
            let face = FaceGeometry::of(&maze.tiling, step.face);
            match (step.from, step.to) {
                (Some(a), Some(b)) => Some(face.centreline(a, b)),
                (None, Some(b)) => Some(
                    face.centreline_to_cap(b, &maze.style_for(step.face))
                        .reversed(),
                ),
                (Some(a), None)
                    if maze.exit
                        == (Cap {
                            face: step.face,
                            side: a,
                        }) =>
                {
                    Some(face.centreline_to_cap(a, &maze.style_for(step.face)))
                }
                _ => None,
            }
        })
        .collect()
}

/// The pieces of the route that pass under a bridge, with their tiles.
#[cfg(test)]
fn route_under_bridges(maze: &Maze) -> Vec<(usize, Curve)> {
    Route::of(maze, &maze.solution()).under
}

/// The centreline of the route from the entrance's cap to the exit's.
#[cfg(test)]
fn solution_curves(maze: &Maze) -> Vec<Curve> {
    step_curves(maze, &maze.solution())
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
            plain: false,
            overlay_group: false,
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
    fn ink_is_the_default_and_follows_the_seed() {
        assert!(!SvgOptions::default().plain);
        let options = SvgOptions {
            solution: true,
            ..SvgOptions::default()
        };
        let ink = render(&hex_maze(4), &options);
        assert!(ink.starts_with("<svg") && ink.trim_end().ends_with("</svg>"));
        assert_eq!(ink.matches("<svg").count(), 1);
        assert_eq!(ink.matches("</svg>").count(), 1);
        assert_eq!(ink, render(&hex_maze(4), &options));
        let plain = SvgOptions {
            plain: true,
            ..options
        };
        assert_ne!(ink, render(&hex_maze(4), &plain));
        // The same tiling with another seed, and the same maze with another
        // pen seed, both draw differently.
        assert_ne!(ink, render(&hex_maze(5), &options));
        let mut other_pen = hex_maze(4);
        other_pen.seed = 99;
        assert_ne!(ink, render(&other_pen, &options));
    }

    #[test]
    fn the_overlay_group_is_the_only_difference_it_makes() {
        for plain in [false, true] {
            let options = SvgOptions {
                solution: true,
                plain,
                ..SvgOptions::default()
            };
            let with = SvgOptions {
                overlay_group: true,
                ..options
            };
            let maze = hex_maze(4);
            let (a, b) = (render(&maze, &options), render(&maze, &with));
            assert_eq!(b.matches("id='overlay'").count(), 1);
            assert_eq!(b.replace("<g id='overlay'></g>\n", ""), a);
            // It is the last child of the flipped group.
            assert!(b.contains("<g id='overlay'></g>\n</g>\n"));
        }
    }

    #[test]
    fn ink_overlays_still_draw() {
        let options = SvgOptions {
            boundaries: true,
            numbers: true,
            ports: true,
            ..SvgOptions::default()
        };
        let svg = render(&hex_maze(3), &options);
        assert!(svg.contains("<polygon") && svg.contains("<text") && svg.contains("<circle"));
    }

    /// Crossings off, which repeats the mazes made before bridges existed.
    fn hex_maze_without_crossings(seed: u64) -> Maze {
        let source = source_by_name("hexagonal").expect("hexagonal exists");
        let tiling = source.build(Region::rectangle(12.0, 9.0), &mut Rng::new(seed));
        let mut options = MazeOptions {
            seed,
            ..MazeOptions::default()
        };
        options.generate.crossings = 0.0;
        options.generate.junctions = 1.0;
        Maze::generate(tiling, &options).expect("a maze")
    }

    #[test]
    fn bridges_draw_in_plain_and_ink() {
        let maze = hex_maze(4);
        assert!(
            maze.bridges.iter().any(Option::is_some),
            "no bridge to test"
        );
        for plain in [true, false] {
            let options = SvgOptions {
                solution: true,
                plain,
                ..SvgOptions::default()
            };
            let svg = render(&maze, &options);
            assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        }
    }

    #[test]
    fn the_route_shows_faintly_under_a_bridge() {
        // Search a few seeds for a route that passes under a bridge.
        let maze = (1..40)
            .map(hex_maze)
            .find(|m| !route_under_bridges(m).is_empty())
            .expect("some route passes under a bridge");
        for (plain, faint) in [
            (true, "stroke-opacity='0.25'"),
            (false, "stroke-opacity='0.18'"),
        ] {
            let options = SvgOptions {
                solution: true,
                plain,
                ..SvgOptions::default()
            };
            assert!(render(&maze, &options).contains(faint), "plain: {plain}");
        }
    }

    #[test]
    fn plain_output_is_unchanged() {
        // First taken from the plain renderer before ink existed, so adding
        // ink cannot quietly alter `-P`. Re-taken once, on purpose, when the
        // entrance and exit gained Lubra's rings, and again when the background
        // and dots took Lubra's colours.
        let options = SvgOptions {
            solution: true,
            boundaries: true,
            numbers: true,
            ports: true,
            plain: true,
            overlay_group: false,
        };
        let svg = render(&hex_maze_without_crossings(4), &options);
        assert_eq!((svg.len(), fnv(&svg)), (17637, 20928853795271304));
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
