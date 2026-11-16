//! A plain drawing of a tiling, for checking the tilings by eye: each tile
//! shaded by its shape, with a dot at the middle of every edge a track may cross.

use std::fmt::Write;

use crate::tiling::{PolygonKind, Tiling};

/// Pixels per edge length.
const SCALE: f64 = 40.0;
const MARGIN: f64 = 10.0;

fn fill(kind: PolygonKind) -> &'static str {
    match kind {
        PolygonKind::Triangle => "#f4d58d",
        PolygonKind::Square => "#a8d5ba",
        PolygonKind::Hexagon => "#9cc3e4",
    }
}

/// The tiling as an SVG document, with `y` pointing up.
pub fn tiling_svg(tiling: &Tiling) -> String {
    let points: Vec<(f64, f64)> = tiling.points.iter().map(|p| p.to_f64()).collect();
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    for &(x, y) in &points {
        (min_x, min_y) = (min_x.min(x), min_y.min(y));
        (max_x, max_y) = (max_x.max(x), max_y.max(y));
    }
    let width = (max_x - min_x) * SCALE + 2.0 * MARGIN;
    let height = (max_y - min_y) * SCALE + 2.0 * MARGIN;
    let at = |(x, y): (f64, f64)| {
        (
            (x - min_x) * SCALE + MARGIN,
            height - ((y - min_y) * SCALE + MARGIN),
        )
    };

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {width:.1} {height:.1}' width='{width:.0}' height='{height:.0}'>"
    );
    let _ = writeln!(svg, "<rect width='100%' height='100%' fill='white'/>");
    for face in &tiling.faces {
        let d: Vec<String> = face
            .vertices
            .iter()
            .map(|&v| {
                let (x, y) = at(points[v]);
                format!("{x:.2},{y:.2}")
            })
            .collect();
        let _ = writeln!(
            svg,
            "<polygon points='{}' fill='{}' stroke='#2b2118' stroke-width='1'/>",
            d.join(" "),
            fill(face.kind)
        );
    }
    for edge in tiling.edges.iter().filter(|e| e.is_interior()) {
        let [a, b] = edge.vertices;
        let mid = (
            (points[a].0 + points[b].0) / 2.0,
            (points[a].1 + points[b].1) / 2.0,
        );
        let (x, y) = at(mid);
        let _ = writeln!(
            svg,
            "<circle cx='{x:.2}' cy='{y:.2}' r='2' fill='#c0392b'/>"
        );
    }
    svg.push_str("</svg>\n");
    svg
}
