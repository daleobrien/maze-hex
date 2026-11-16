//! Draws every tiling source: `cargo run --example tilings -- DIR`.
//! Periodic tilings are drawn first, then the free mixes with seed 1.

use std::path::PathBuf;

use maze_hex::debug_svg::tiling_svg;
use maze_hex::rng::Rng;
use maze_hex::tiling::catalogue;
use maze_hex::tiling::source::{source_by_name, source_names};
use maze_hex::tiling::Region;

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let region = Region::rectangle(14.0, 9.0);
    for name in source_names() {
        let source = source_by_name(name).expect("every listed name has a source");
        // Free mixes are random, so the pictures fix the seed.
        let tiling = source.build(region, &mut Rng::new(1));
        let path = dir.join(format!("{name}.svg"));
        std::fs::write(&path, tiling_svg(&tiling))?;
        let notation = catalogue::by_name(name).map_or("free mix", |cell| cell.notation);
        println!(
            "{:<22} {:<12} {:>4} tiles  {}",
            name,
            notation,
            tiling.faces.len(),
            path.display()
        );
    }
    Ok(())
}
