//! Walks part way along a maze's solution and draws the result, to check the
//! overlay by eye: `cargo run --example play -- [FILE]` (default `play.svg`).

use maze_hex::maze::solution::Step;
use maze_hex::maze::Maze;
use maze_hex::play::Game;
use maze_hex::render::svg::Drawing;
use maze_hex::track::face::FaceGeometry;

fn main() -> std::io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "play.svg".to_string());
    let mut game = Game::from_id("demo", "hexagonal", 20.0, 14.0, true);

    // Follow the solution for the first two thirds of its tiles, moving as a
    // page would, by pointing at the opening to leave by, then look at the
    // ways on without taking them. The first way clockwise from up is often
    // the way back, which the trail hides, so the second shows more.
    let route = game.maze().solution();
    while game.trail_length() < route.len() * 2 / 3 {
        if !follow(&mut game, &route) {
            break;
        }
    }
    game.cycle();
    game.cycle();

    let overlay = game.overlay(true);
    let svg = game.maze_svg().replace(
        "<g id='overlay'></g>",
        &format!("<g id='overlay'>{overlay}</g>"),
    );
    std::fs::write(&path, svg)?;
    println!(
        "{path}: {} of {} tiles walked, moves {:?}, solved {}",
        game.trail_length(),
        route.len(),
        game.moves(),
        game.solved()
    );
    Ok(())
}

/// Moves along `route` to the next decision. Returns whether the player moved.
fn follow(game: &mut Game, route: &[Step]) -> bool {
    let drawing = Drawing::of(game.maze());
    let at = game.position();
    // The player stands where the route's step for some tile is entered.
    let leaving = route.iter().find_map(|step| {
        let entered = opening(game.maze(), &drawing, step.face, step.from)?;
        let near = (entered.0 - at.0).hypot(entered.1 - at.1) < 1e-6;
        let to = step.to?;
        near.then(|| opening(game.maze(), &drawing, step.face, Some(to)))?
    });
    match leaving {
        Some((x, y)) => game.toward(x, y),
        None => false,
    }
}

/// The middle of side `side` of tile `face` in drawing units, or the entrance's
/// ring when there is no side.
fn opening(maze: &Maze, drawing: &Drawing, face: usize, side: Option<usize>) -> Option<(f64, f64)> {
    let point = match side {
        Some(side) => FaceGeometry::of(&maze.tiling, face).midpoint(side),
        None if face == maze.entrance.face => maze.cap_centre(maze.entrance),
        None => return None,
    };
    Some(drawing.point(point))
}
