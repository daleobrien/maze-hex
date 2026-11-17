//! Times making and drawing a maze at each of maze.do.id.au's four hexagon
//! levels (`make timing`). The website sizes a level as 2.6 × side² square
//! edge lengths, about side² hexagons; here they are 4:3.

use std::time::Instant;

use maze_hex::play::Game;

fn main() {
    for (level, side) in [
        ("easy", 17.0),
        ("medium", 25.0),
        ("hard", 35.0),
        ("crazy", 50.0),
    ] {
        let area: f64 = 2.6 * side * side;
        let (width, height) = ((area * 4.0 / 3.0).sqrt(), (area * 3.0 / 4.0).sqrt());
        let start = Instant::now();
        let game = Game::from_id("timing", "hexagonal", width, height, true);
        let made = start.elapsed();
        let start = Instant::now();
        let svg = game.maze_svg();
        let drawn = start.elapsed();
        println!(
            "{level:7} {:5} tiles  make {made:>9.2?}  draw {drawn:>9.2?}  ({} kB of SVG)",
            game.maze().tiling.faces.len(),
            svg.len() / 1000
        );
    }
}
