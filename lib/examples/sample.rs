//! Draws one maze per tiling, for checking by eye:
//! `cargo run --example sample -- DIR [SEED]`.

use std::path::PathBuf;

use lib::maze::{Maze, MazeOptions};
use lib::render::svg::{render, SvgOptions};
use lib::rng::Rng;
use lib::tiling::source::{source_by_name, source_names};
use lib::tiling::Region;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    for name in source_names() {
        let Some(source) = source_by_name(name) else {
            continue;
        };
        let tiling = source.build(Region::rectangle(16.0, 11.0), &mut Rng::new(seed));
        let options = MazeOptions {
            seed,
            ..MazeOptions::default()
        };
        let Some(maze) = Maze::generate(tiling, &options) else {
            eprintln!("{name}: too small for a maze");
            continue;
        };
        let svg = render(
            &maze,
            &SvgOptions {
                solution: true,
                ..SvgOptions::default()
            },
        );
        let path = dir.join(format!("{name}.svg"));
        std::fs::write(&path, svg)?;
        println!(
            "{name:<22} {:>4} tiles {:>3} blank  {}",
            maze.tiling.faces.len(),
            maze.blank_tiles(),
            path.display()
        );
    }
    Ok(())
}
