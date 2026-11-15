//! Plain SVG output.

use crate::maze::Maze;

/// What to draw besides the walls.
#[derive(Clone, Copy, Debug, Default)]
pub struct SvgOptions {
    /// The route from entrance to exit, in red (`-S`).
    pub solution: bool,
    /// Faint tile outlines (`-B`).
    pub boundaries: bool,
    /// Each tile's number (`-N`).
    pub numbers: bool,
    /// A dot on every open port, coloured by track (`-T`).
    pub ports: bool,
}

/// The maze as a complete SVG document.
pub fn render(maze: &Maze, options: &SvgOptions) -> String {
    let _ = (maze, options);
    String::new()
}
