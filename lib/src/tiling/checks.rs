//! Invariants every tiling must satisfy, shared by the tests of every source.

use super::Tiling;
use crate::geometry::Point;

/// Panics, naming `label`, unless `tiling` is a valid single piece: every edge
/// is an exact unit vector, edges and faces agree, every interior vertex has
/// exactly 360° round it, and every tile can be reached from every other.
pub fn assert_valid(tiling: &Tiling, label: &str) {
    assert!(!tiling.faces.is_empty(), "{label}: empty");
    for edge in &tiling.edges {
        let [a, b] = edge.vertices;
        let v: Point = tiling.points[b] - tiling.points[a];
        assert!(v.as_unit_direction().is_some(), "{label}: {v:?}");
    }
    for face in &tiling.faces {
        assert_eq!(face.edges.len(), face.kind.sides(), "{label}");
        assert_eq!(face.vertices.len(), face.kind.sides(), "{label}");
    }
    // Every corner turns as its kind says, which for a pentagon also means it
    // starts at its 60° apex and runs anticlockwise.
    for face in &tiling.faces {
        let n = face.vertices.len();
        let edge = |i: usize| {
            let v = tiling.points[face.vertices[(i + 1) % n]] - tiling.points[face.vertices[i % n]];
            v.as_unit_direction().expect("unit edge").steps()
        };
        for corner in 0..n {
            let turn = (edge(corner) + 12 - edge(corner + n - 1)) % 12;
            assert_eq!(
                turn,
                face.kind.turn_steps_at(corner),
                "{label}: {:?} corner {corner}",
                face.kind
            );
        }
    }
    for (e, edge) in tiling.edges.iter().enumerate() {
        for side in edge.faces.iter().flatten() {
            assert_eq!(tiling.faces[side.face].edges[side.side], e, "{label}");
        }
    }

    let mut angle = vec![0u32; tiling.points.len()];
    let mut on_boundary = vec![false; tiling.points.len()];
    for face in &tiling.faces {
        for (corner, &v) in face.vertices.iter().enumerate() {
            angle[v] += u32::from(face.kind.interior_steps_at(corner));
        }
    }
    for edge in tiling.edges.iter().filter(|e| !e.is_interior()) {
        for v in edge.vertices {
            on_boundary[v] = true;
        }
    }
    for v in 0..tiling.points.len() {
        if on_boundary[v] {
            assert!(angle[v] < 12, "{label}: boundary vertex {v} over 360°");
        } else {
            assert_eq!(angle[v], 12, "{label}: vertex {v}");
        }
    }

    let mut seen = vec![false; tiling.faces.len()];
    let mut stack = vec![0];
    seen[0] = true;
    while let Some(face) = stack.pop() {
        for next in tiling.neighbours(face) {
            if !seen[next.face] {
                seen[next.face] = true;
                stack.push(next.face);
            }
        }
    }
    assert!(seen.iter().all(|&s| s), "{label}: not one piece");
}
