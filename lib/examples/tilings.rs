//! Draws every tiling in the catalogue: `cargo run --example tilings -- DIR`.

use std::path::PathBuf;

use lib::debug_svg::tiling_svg;
use lib::tiling::{catalogue, Region, Tiling};

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let region = Region {
        width: 14.0,
        height: 9.0,
    };
    for cell in catalogue::ALL {
        let tiling = Tiling::build(cell, region);
        let path = dir.join(format!("{}.svg", cell.name));
        std::fs::write(&path, tiling_svg(&tiling))?;
        println!(
            "{:<22} {:<12} {:>4} tiles  {}",
            cell.name,
            cell.notation,
            tiling.faces.len(),
            path.display()
        );
    }
    Ok(())
}
