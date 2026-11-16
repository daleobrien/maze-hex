//! Draws every tiling source: `cargo run --example tilings -- DIR`.
//! Periodic tilings are drawn first, then the free mixes with seed 1, then
//! the random mix with half its square-triangle pairs merged into pentagons.

use std::path::PathBuf;

use lib::debug_svg::tiling_svg;
use lib::rng::Rng;
use lib::tiling::catalogue;
use lib::tiling::pentagon::merge_pentagons;
use lib::tiling::source::{source_by_name, source_names};
use lib::tiling::Region;

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
    // Pentagons are not a source of their own: they merge into another tiling.
    let random = source_by_name("random").expect("random is a source");
    let base = random.build(region, &mut Rng::new(1));
    let tiling = merge_pentagons(&base, 0.5, &mut Rng::new(1));
    let path = dir.join("random-pentagons.svg");
    std::fs::write(&path, tiling_svg(&tiling))?;
    println!(
        "{:<22} {:<12} {:>4} tiles  {}",
        "random-pentagons",
        "free mix",
        tiling.faces.len(),
        path.display()
    );
    Ok(())
}
