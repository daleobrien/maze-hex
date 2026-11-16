//! The `maze-hex` command: draws one maze as an SVG file.

mod args;

use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use args::{Mode, Options};
use maze_hex::maze::{Maze, MazeOptions};
use maze_hex::render::svg::{render, SvgOptions};
use maze_hex::rng::Rng;
use maze_hex::tiling::grow::RandomMix;
use maze_hex::tiling::source::{source_by_name, source_names};
use maze_hex::tiling::{Region, TilingSource};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // Bad input exits with 2, the usual status for a usage error, so scripts
    // can tell it apart from a maze that could not be drawn (1).
    let options = args::parse(&arguments).unwrap_or_else(|message| {
        eprintln!("error: {message}");
        eprintln!("hint: run `maze-hex --help` to see every flag");
        process::exit(2)
    });

    match options.mode {
        Mode::Help => print!("{}", args::usage()),
        Mode::ListTilings => {
            for name in source_names() {
                println!("{name}");
            }
        }
        Mode::Generate => {
            if let Err(message) = run(&options) {
                eprintln!("error: {message}");
                process::exit(1);
            }
        }
    }
}

fn run(options: &Options) -> Result<(), String> {
    let seed = match options.seed {
        Some(seed) => seed,
        None => {
            // Printed before the work starts, so the seed is still known if
            // the maze then fails to generate.
            let seed = clock_seed();
            eprintln!("seed {seed} (from the clock; pass --seed {seed} to draw this maze again)");
            seed
        }
    };
    // `--mix` is only accepted with `--tiling random`, so it replaces the default source.
    let source: Box<dyn TilingSource> = match options.mix {
        Some(weights) => Box::new(RandomMix { weights }),
        None => source_by_name(&options.tiling)
            .ok_or_else(|| format!("unknown tiling '{}'", options.tiling))?,
    };
    let region = Region::rectangle(options.width, options.height).with_shape(options.shape);

    // The tiling and the maze each draw from their own stream, but both start
    // from this one seed, so a single number repeats the whole picture.
    let tiling = source.build(region, &mut Rng::new(seed));
    let maze_options = MazeOptions {
        seed,
        style: options.style,
        generate: options.generate,
    };
    let maze = Maze::generate(tiling, &maze_options).ok_or_else(|| {
        "could not fit a maze in this region; try a larger --width or --height".to_string()
    })?;

    let svg = render(
        &maze,
        &SvgOptions {
            solution: options.solution,
            boundaries: options.boundaries,
            numbers: options.numbers,
            ports: options.ports,
            plain: options.plain,
            overlay_group: false,
            tile_group: false,
        },
    );
    let path = args::output_path(&options.output);
    std::fs::write(&path, svg).map_err(|e| format!("could not write {path}: {e}"))?;
    eprintln!(
        "wrote {path}: {} tiles, {} blank, seed {seed}",
        maze.tiling.faces.len(),
        maze.blank_tiles()
    );
    Ok(())
}

/// A seed from the clock. Nanoseconds since 1970 differ on every run, which is
/// enough to make runs differ; the printed value is what makes one repeatable.
fn clock_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}
