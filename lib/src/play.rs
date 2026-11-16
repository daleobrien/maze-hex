//! Playing a maze: the player's trail from the entrance, and moving along it.
//!
//! This is the logic of Lubra's `Play` in the maze-website repository
//! (`curvy/src/play.rs`) and of its wasm `Game`, ported from Lubra's nodes and
//! nets to maze-hex's tiles, sides and blocks, so the website can drive both
//! engines through one interface. Nothing here knows about the browser, so it
//! is tested natively.
//!
//! The player always stands where a decision is needed: at the entrance, where
//! a track opens into a junction (a block of three or more sides), or at a
//! dead end (a block of one side). A move picks one of the ways on, the other
//! sides of the player's block, and follows it through any plain stretch of
//! track (blocks of exactly two sides, which includes both tracks of a bridge)
//! to the next such place. Picking the way just come by goes back to the last
//! junction instead. A way can also be chosen first (`cycle`, clockwise through
//! the ways on) and then taken (`choose`); the overlay shows the track it
//! would follow.
//!
//! Points and directions given to and returned from a [`Game`] are in the
//! drawing units of the SVG it draws (see [`Drawing`]), with `y` up, so a page
//! can pass pointer positions straight in.

use std::f64::consts::TAU;

use crate::maze::solution::Step;
use crate::maze::{Cap, Maze, MazeOptions};
use crate::render::ink::PENCIL;
use crate::render::svg::{render, Drawing, SvgOptions};
use crate::rng::Rng;
use crate::tiling::source::source_by_name;
use crate::tiling::Region;
use crate::track::face::FaceGeometry;

/// The colour of the player's own trail.
const TRAIL: &str = "#1d5fa6";

/// The colour of the way chosen with `cycle`, before it is taken.
const PREVIEW: &str = "#8db8e8";

/// The radius of the dot that marks where the player stands, in drawing units
/// (a track is about 16 wide, so the dot sits well inside it).
const PLAYER_RADIUS: f64 = 5.5;

/// How far outside the player's opening the way back counts as being, in
/// drawing units (an edge is 40 long).
const BACK_REACH: f64 = 10.0;

/// A point or direction in drawing units (y up).
type Point = (f64, f64);

/// The arrow keys' directions (y up): up, down, left and right.
const ARROWS: [Point; 4] = [(0.0, 1.0), (0.0, -1.0), (-1.0, 0.0), (1.0, 0.0)];

/// A way on from where the player stands: the side to leave by, or `None` for
/// the way back.
type Way = Option<usize>;

/// The tiling used when a name is not known.
const DEFAULT_TILING: &str = "hexagonal";

/// The seed of the maze with id `id` (any text): its 32-bit FNV-1a hash, as
/// the website's other engine takes it, so one id means one seed everywhere.
fn seed_of(id: &str) -> u64 {
    let hash = id.chars().fold(0x811c_9dc5_u32, |hash, c| {
        (hash ^ u32::from(c)).wrapping_mul(0x0100_0193)
    });
    u64::from(hash)
}

/// How far round `direction` is, clockwise from straight up, in radians.
fn clockwise((x, y): Point) -> f64 {
    x.atan2(y).rem_euclid(TAU)
}

/// How well `way` matches `direction`: the cosine of the angle between them.
fn alignment(way: Point, direction: Point) -> f64 {
    (way.0 * direction.0 + way.1 * direction.1)
        / (way.0.hypot(way.1) * direction.0.hypot(direction.1)).max(1e-9)
}

/// Gives each way its own arrow key where it can: as many ways as possible get
/// a key, ways on before the way back (at a junction of four ways on there is
/// no key left for it), and among those choices, the keys match the ways'
/// directions best. Returns the way each arrow (as in `ARROWS`) takes, if any.
fn arrow_keys(ways: &[(Way, Point)]) -> [Option<Way>; ARROWS.len()] {
    /// How many ways on and ways in all were given a key, then the total
    /// alignment.
    type Score = (usize, usize, f64);
    /// The best choice so far, and its keys.
    type Best = (Score, [Option<Way>; ARROWS.len()]);

    fn better(a: Score, b: Score) -> bool {
        (a.0, a.1) > (b.0, b.1) || ((a.0, a.1) == (b.0, b.1) && a.2 > b.2)
    }

    fn search(
        ways: &[(Way, Point)],
        keys: &mut [Option<Way>; ARROWS.len()],
        score: Score,
        best: &mut Best,
    ) {
        let Some((&(way, direction), rest)) = ways.split_first() else {
            if better(score, best.0) {
                *best = (score, *keys);
            }
            return;
        };
        // This way without a key, then with each free key in turn.
        search(rest, keys, score, best);
        let on = usize::from(way.is_some());
        for arrow in 0..ARROWS.len() {
            if keys[arrow].is_none() {
                keys[arrow] = Some(way);
                let fit = alignment(direction, ARROWS[arrow]);
                search(rest, keys, (score.0 + on, score.1 + 1, score.2 + fit), best);
                keys[arrow] = None;
            }
        }
    }

    let mut best: Best = ((0, 0, f64::NEG_INFINITY), [None; ARROWS.len()]);
    search(ways, &mut [None; ARROWS.len()], (0, 0, 0.0), &mut best);
    best.1
}

/// One maze in play.
pub struct Game {
    maze: Maze,
    drawing: Drawing,
    /// The route from the entrance to the exit, for showing it.
    solution: Vec<Step>,
    /// The tiles walked through, from the entrance. The last one has no way
    /// out yet: the player stands where they came into it.
    trail: Vec<Step>,
    /// The way chosen with `cycle`, as an index into `ways_clockwise`, until a
    /// move.
    chosen: Option<usize>,
}

impl Game {
    /// A maze from the site's maze id (any text; its FNV-1a hash is the seed)
    /// on the tiling called `tiling` (`hexagonal` if there is none by that
    /// name), `width` by `height` edge lengths (at least 4 by 4), with bridges
    /// unless `crossings` is false. The same id, tiling, size and crossings
    /// always give the same maze. If a maze will not fit, the size grows a
    /// little until one does, so this never fails.
    pub fn from_id(id: &str, tiling: &str, width: f64, height: f64, crossings: bool) -> Game {
        let seed = seed_of(id);
        let mut options = MazeOptions {
            seed,
            ..MazeOptions::default()
        };
        if !crossings {
            options.generate.crossings = 0.0;
        }
        let (width, height) = (width.max(4.0), height.max(4.0));
        // A tiling that keeps failing is given up on, so a name that cannot
        // hold a maze at any size still ends in one; the hexagonal tiling
        // always does once it is large enough.
        let mut attempt = 0_u32;
        loop {
            let name = if attempt < 8 { tiling } else { DEFAULT_TILING };
            let source = source_by_name(name).or_else(|| source_by_name(DEFAULT_TILING));
            let grow = f64::from(attempt);
            let region = Region::rectangle(width + grow, height + grow);
            let maze = source.and_then(|source| {
                let tiling = source.build(region, &mut Rng::new(seed));
                Maze::generate(tiling, &options)
            });
            if let Some(maze) = maze.filter(|maze| maze.solution().len() >= 2) {
                return Game::new(maze);
            }
            attempt += 1;
        }
    }

    /// A new game on `maze`, standing at the entrance.
    pub fn new(maze: Maze) -> Game {
        let mut game = Game {
            drawing: Drawing::of(&maze),
            solution: maze.solution(),
            maze,
            trail: Vec::new(),
            chosen: None,
        };
        game.restart();
        game
    }

    pub fn maze(&self) -> &Maze {
        &self.maze
    }

    /// The maze as ink SVG (no solution), with an empty `<g id='overlay'>` as
    /// the last child of the flipped group, so overlay markup shares the
    /// maze's drawing units.
    pub fn maze_svg(&self) -> String {
        let options = SvgOptions {
            overlay_group: true,
            tile_group: true,
            ..SvgOptions::default()
        };
        render(&self.maze, &options)
    }

    /// Markup for the overlay group: the solution when `show_solution`, the
    /// way chosen with `cycle`, the player's trail and a dot where the player
    /// stands. Each is pencilled like the maze's own solution: faint where it
    /// passes under a bridge, on top of the bridge where it goes over.
    pub fn overlay(&self, show_solution: bool) -> String {
        let (maze, drawing) = (&self.maze, &self.drawing);
        let mut out = String::new();
        if show_solution {
            out.push_str(&drawing.pencil(maze, &self.solution, PENCIL, "under-solution"));
        }
        let preview = self.preview();
        out.push_str(&drawing.pencil(maze, &preview, PREVIEW, "under-preview"));
        out.push_str(&drawing.pencil(maze, &self.trail, TRAIL, "under-trail"));
        let (x, y) = self.position();
        out.push_str(&format!(
            "<circle cx='{x:.1}' cy='{y:.1}' r='{PLAYER_RADIUS}' fill='{TRAIL}' fill-opacity='0.9'/>"
        ));
        out
    }

    /// Moves the way that best matches the direction `(dx, dy)` (y up).
    /// Returns whether the player moved.
    pub fn step(&mut self, dx: f64, dy: f64) -> bool {
        if self.solved() || dx.hypot(dy) < 1e-9 {
            return false;
        }
        let best = self.best_way((dx, dy));
        self.take(best)
    }

    /// Moves the way arrow key `arrow` stands for: 0 up, 1 down, 2 left, 3
    /// right. Each way on gets its own key where there are keys enough, so two
    /// ways in much the same direction can both be taken; a key no way has
    /// goes the way it best matches.
    pub fn arrow(&mut self, arrow: usize) -> bool {
        if self.solved() || arrow >= ARROWS.len() {
            return false;
        }
        let way = arrow_keys(&self.ways())[arrow].or_else(|| self.best_way(ARROWS[arrow]));
        self.take(way)
    }

    /// Chooses the next way on, clockwise from up, without moving (the first,
    /// the first time). Returns whether there is a way to choose.
    pub fn cycle(&mut self) -> bool {
        let count = self.ways().len();
        if self.solved() || count == 0 {
            return false;
        }
        self.chosen = Some(self.chosen.map_or(0, |chosen| (chosen + 1) % count));
        true
    }

    /// Takes the way chosen with `cycle`. Returns whether the player moved.
    pub fn choose(&mut self) -> bool {
        if self.solved() {
            return false;
        }
        let way = self.chosen_way();
        self.take(way)
    }

    /// The trail as the side left by at the entrance and at each junction, one
    /// digit each. The stretches between follow on by themselves, so this is
    /// the whole trail.
    pub fn moves(&self) -> String {
        self.trail
            .iter()
            .enumerate()
            .filter(|&(index, step)| index == 0 || self.block(step).len() != 2)
            .filter_map(|(_, step)| step.to)
            .filter_map(|side| char::from_digit(side as u32, 10))
            .collect()
    }

    /// Walks `moves` (as from `moves`) from the entrance. Stops at the first
    /// move that does not fit this maze, so a link made for another maze just
    /// goes as far as it can. Returns how many moves were made.
    pub fn replay(&mut self, moves: &str) -> usize {
        self.restart();
        let mut made = 0;
        for side in moves.chars().map_while(|c| c.to_digit(10)) {
            let side = side as usize;
            let here = self.here();
            let open = self.block(&here).contains(&side) && Some(side) != here.from;
            if self.solved() || !open || !self.go(side) {
                break;
            }
            made += 1;
        }
        made
    }

    /// Moves towards the point `(x, y)` in drawing units: the way whose
    /// opening is nearest to it. The way back counts as a point a little
    /// outside the opening the player stands at.
    pub fn toward(&mut self, x: f64, y: f64) -> bool {
        if self.solved() {
            return false;
        }
        let at = self.position();
        let here = self.here();
        let geometry = FaceGeometry::of(&self.maze.tiling, here.face);
        let nearest = self
            .ways()
            .into_iter()
            .map(|(way, direction)| {
                let point = match way {
                    Some(side) => self.drawing.point(geometry.midpoint(side)),
                    None => {
                        let length = direction.0.hypot(direction.1).max(1e-9);
                        (
                            at.0 + direction.0 / length * BACK_REACH,
                            at.1 + direction.1 / length * BACK_REACH,
                        )
                    }
                };
                (way, (point.0 - x).hypot(point.1 - y))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        self.take(nearest.map(|(way, _)| way))
    }

    /// Back to the entrance.
    pub fn restart(&mut self) {
        self.chosen = None;
        self.trail = vec![self.entrance_step()];
    }

    /// Whether the player has reached the exit.
    pub fn solved(&self) -> bool {
        let here = self.here();
        let exit = self.maze.exit;
        here.face == exit.face && here.from == Some(exit.side)
    }

    /// How many tiles the trail has passed through.
    pub fn trail_length(&self) -> usize {
        self.trail.len()
    }

    /// Where the player stands, in drawing units: at the side they came in by,
    /// in the ring at the entrance, and in the ring at the exit once solved.
    pub fn position(&self) -> (f64, f64) {
        let here = self.here();
        let point = if self.solved() {
            self.maze.cap_centre(self.maze.exit)
        } else if let Some(side) = here.from {
            FaceGeometry::of(&self.maze.tiling, here.face).midpoint(side)
        } else {
            self.maze.cap_centre(self.maze.entrance)
        };
        self.drawing.point(point)
    }

    /// The step standing in the entrance's dead end.
    fn entrance_step(&self) -> Step {
        Step {
            face: self.maze.entrance.face,
            from: None,
            to: None,
        }
    }

    /// Where the player stands: the last step of the trail.
    fn here(&self) -> Step {
        self.trail
            .last()
            .copied()
            .unwrap_or_else(|| self.entrance_step())
    }

    /// The sides of the block `step` is in: the one it was entered by, or the
    /// entrance's dead end for the first step. A tile may hold other blocks
    /// (a second track, or the other track of a bridge), which are not part of
    /// this step.
    fn block(&self, step: &Step) -> &[usize] {
        let side = step.from.unwrap_or(self.maze.entrance.side);
        self.maze.blocks[step.face]
            .iter()
            .find(|block| block.contains(&side))
            .map_or(&[], Vec::as_slice)
    }

    /// The ways on from where the player stands, each with its direction in
    /// drawing units: every other side of the block, towards that side's
    /// midpoint, and the way back (when the player came in by a side), which
    /// leaves the tile by that side.
    fn ways(&self) -> Vec<(Way, Point)> {
        let here = self.here();
        let at = self.position();
        let geometry = FaceGeometry::of(&self.maze.tiling, here.face);
        let mut ways: Vec<(Way, Point)> = self
            .block(&here)
            .iter()
            .filter(|&&side| Some(side) != here.from)
            .map(|&side| {
                let to = self.drawing.point(geometry.midpoint(side));
                (Some(side), (to.0 - at.0, to.1 - at.1))
            })
            .collect();
        if let Some(side) = here.from {
            // Back is out of the tile, through the side the player came by.
            let out = geometry.inward(side).scale(-1.0);
            ways.push((None, (out.x, out.y)));
        }
        ways
    }

    /// The way on that best matches `direction`, if there is any way on.
    fn best_way(&self, direction: Point) -> Option<Way> {
        self.ways()
            .into_iter()
            .map(|(way, d)| (way, alignment(d, direction)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(way, _)| way)
    }

    /// The ways on, clockwise from straight up: the order `cycle` goes through
    /// them.
    fn ways_clockwise(&self) -> Vec<(Way, Point)> {
        let mut ways = self.ways();
        ways.sort_by(|a, b| clockwise(a.1).total_cmp(&clockwise(b.1)));
        ways
    }

    /// The way `cycle` has chosen, if any.
    fn chosen_way(&self) -> Option<Way> {
        let chosen = self.chosen?;
        self.ways_clockwise().get(chosen).map(|&(way, _)| way)
    }

    /// The track the chosen way would follow, as steps from where the player
    /// stands (empty when no way is chosen).
    fn preview(&self) -> Vec<Step> {
        match self.chosen_way() {
            Some(Some(side)) => self.walk(side).unwrap_or_default(),
            Some(None) => self.trail[self.back_to()..].to_vec(),
            None => Vec::new(),
        }
    }

    /// Takes `way`, if there is one. Returns whether the player moved.
    fn take(&mut self, way: Option<Way>) -> bool {
        self.chosen = None;
        match way {
            Some(Some(side)) => self.go(side),
            Some(None) => self.back(),
            None => false,
        }
    }

    /// Leaves by `side` and follows the track to the next junction or dead
    /// end.
    fn go(&mut self, side: usize) -> bool {
        let Some(steps) = self.walk(side) else {
            return false;
        };
        self.trail.pop();
        self.trail.extend(steps);
        true
    }

    /// The steps leaving by `side` would take: where the player stands (now
    /// with its way out), then each tile to the next junction, dead end or the
    /// exit. `None` when there is no track that way.
    fn walk(&self, mut side: usize) -> Option<Vec<Step>> {
        let tiling = &self.maze.tiling;
        let mut steps = vec![self.here()];
        while let Some(here) = steps.last_mut() {
            let Some(&edge) = tiling.faces[here.face].edges.get(side) else {
                break;
            };
            let across = tiling.edges[edge].across(here.face);
            let Some(next) = across.filter(|_| self.maze.open[edge]) else {
                break;
            };
            let Some(block) = self.maze.blocks[next.face]
                .iter()
                .find(|block| block.contains(&next.side))
            else {
                break;
            };
            here.to = Some(side);
            let at_exit = Cap {
                face: next.face,
                side: next.side,
            } == self.maze.exit;
            let other = block.iter().copied().find(|&s| s != next.side);
            let plain = block.len() == 2;
            steps.push(Step {
                face: next.face,
                from: Some(next.side),
                to: None,
            });
            // A plain stretch carries on out of its other side.
            match other {
                Some(other) if plain && !at_exit => side = other,
                _ => break,
            }
        }
        (steps.len() > 1).then_some(steps)
    }

    /// Where on the trail the way back goes to: the last junction (or the
    /// entrance).
    fn back_to(&self) -> usize {
        let mut index = self.trail.len().saturating_sub(2);
        while index > 0 && self.block(&self.trail[index]).len() == 2 {
            index -= 1;
        }
        index
    }

    /// Goes back along the trail to the last junction (or the entrance).
    fn back(&mut self) -> bool {
        if self.trail.len() < 2 {
            return false;
        }
        self.trail.truncate(self.back_to() + 1);
        if let Some(last) = self.trail.last_mut() {
            last.to = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tilings and crossing settings every maze test runs on.
    const SETUPS: [(&str, bool); 4] = [
        ("hexagonal", true),
        ("hexagonal", false),
        ("random", true),
        ("random", false),
    ];

    fn game_for(id: &str, tiling: &str, crossings: bool) -> Game {
        Game::from_id(id, tiling, 20.0, 14.0, crossings)
    }

    fn game(seed: u64) -> Game {
        game_for(&format!("maze {seed}"), "hexagonal", true)
    }

    /// The solution's step for the tile the player stands in.
    fn route_step(game: &Game) -> Step {
        let here = game.here();
        *game
            .solution
            .iter()
            .find(|s| (s.face, s.from) == (here.face, here.from))
            .expect("the player stays on the route")
    }

    /// Plays the solution by moving towards each junction's way on in turn.
    fn walk_solution(game: &mut Game) {
        let mut guard = 0;
        while !game.solved() {
            guard += 1;
            assert!(guard < 1000, "never reached the exit");
            let here = game.here();
            let side = route_step(game).to.expect("the route goes on");
            let geometry = FaceGeometry::of(&game.maze.tiling, here.face);
            let (x, y) = game.drawing.point(geometry.midpoint(side));
            assert!(game.toward(x, y), "could not move towards the route");
        }
    }

    #[test]
    fn test_following_the_solution_solves_the_maze() {
        for (tiling, crossings) in SETUPS {
            for id in ["a", "demo", "maze.do.id", "4", "seven"] {
                let mut game = game_for(id, tiling, crossings);
                let name = format!("{tiling} {crossings} {id}");
                assert!(!game.solved(), "{name}");
                walk_solution(&mut game);
                assert!(game.solved(), "{name}");
                // The trail is then the solution itself.
                assert_eq!(game.trail, game.maze.solution(), "{name}");
                assert_eq!(game.trail_length(), game.solution.len(), "{name}");
                // The player ends on the exit's ring.
                let exit = game.drawing.point(game.maze.cap_centre(game.maze.exit));
                assert_eq!(game.position(), exit, "{name}");
            }
        }
    }

    #[test]
    fn test_no_moves_once_solved() {
        let mut game = game(3);
        walk_solution(&mut game);
        let trail = game.trail.clone();
        assert!(!game.step(1.0, 0.0));
        assert!(!game.step(0.0, -1.0));
        assert!(!game.toward(0.0, 0.0));
        assert!(!game.cycle());
        assert!(!game.choose());
        for arrow in 0..4 {
            assert!(!game.arrow(arrow));
        }
        assert_eq!(game.trail, trail);
    }

    #[test]
    fn test_the_player_always_stands_at_a_decision() {
        for (tiling, crossings) in SETUPS {
            let mut game = game_for("decisions", tiling, crossings);
            let mut guard = 0;
            while !game.solved() {
                guard += 1;
                assert!(guard < 1000);
                let here = game.here();
                let sides = game.block(&here).len();
                assert_ne!(sides, 2, "stopped in a plain stretch: {tiling}");
                let ways = game.ways();
                // The block's other sides, plus the way back; at the entrance, its one side.
                assert_eq!(ways.len(), sides);
                assert!(ways.iter().all(|(_, d)| d.0.hypot(d.1) > 1e-6));
                let side = route_step(&game).to.expect("the route goes on");
                let at = game
                    .drawing
                    .point(FaceGeometry::of(&game.maze.tiling, here.face).midpoint(side));
                assert!(game.toward(at.0, at.1));
            }
        }
    }

    #[test]
    fn test_going_back_returns_to_the_last_junction() {
        for (tiling, crossings) in SETUPS {
            let mut game = game_for("back", tiling, crossings);
            // Take the first way on, then step back the way we came.
            let start = game.trail.len();
            let way = game.ways()[0].1;
            assert!(game.step(way.0, way.1));
            let after = game.trail.clone();
            let back = game
                .ways()
                .into_iter()
                .find(|(w, _)| w.is_none())
                .map(|(_, d)| d)
                .expect("a way back");
            assert!(game.step(back.0, back.1));
            assert!(game.trail.len() < after.len());
            assert!(game.trail.len() >= start);
            let last = game.trail[game.trail.len() - 1];
            assert!(last.to.is_none());
            assert!(game.trail.len() == 1 || game.block(&last).len() != 2);
        }
    }

    #[test]
    fn test_arrow_keys_give_ways_in_the_same_direction_their_own_keys() {
        // Two ways up-ish, one right, and the way back down.
        let up = (Some(0), (0.2, 1.0));
        let also_up = (Some(1), (-0.3, 1.0));
        let right = (Some(2), (1.0, 0.1));
        let back = (None, (0.0, -1.0));
        let keys = arrow_keys(&[up, also_up, right, back]);
        assert_eq!(
            keys,
            [Some(Some(0)), Some(None), Some(Some(1)), Some(Some(2))]
        );
        // With fewer ways than keys, every way still gets the key it matches
        // best.
        assert_eq!(
            arrow_keys(&[right, back]),
            [None, Some(None), None, Some(Some(2))]
        );
    }

    #[test]
    fn test_ways_on_get_the_keys_before_the_way_back() {
        // Four ways on and the way back: the way back is left without a key,
        // even though it points the way no other way does.
        let ways = [
            (Some(0), (0.0, 1.0)),
            (Some(1), (0.2, 1.0)),
            (Some(2), (1.0, 0.0)),
            (Some(3), (0.0, -1.0)),
            (None, (-1.0, 0.0)),
        ];
        let keys = arrow_keys(&ways);
        for (way, _) in &ways[..4] {
            assert!(keys.contains(&Some(*way)), "{way:?} has a key");
        }
        assert!(!keys.contains(&Some(None)));
        // With fewer ways on, the way back has its key.
        let keys = arrow_keys(&ways[2..]);
        assert_eq!(keys[2], Some(None), "left goes back");
    }

    #[test]
    fn test_every_way_on_can_be_reached_with_the_arrow_keys() {
        for (tiling, crossings) in SETUPS {
            for seed in 0..2 {
                let mut game = game_for(&format!("keys {seed}"), tiling, crossings);
                let mut guard = 0;
                while !game.solved() {
                    guard += 1;
                    assert!(guard < 1000, "never reached the exit");
                    let ways = game.ways();
                    let keys = arrow_keys(&ways);
                    let reached = ways
                        .iter()
                        .filter(|(way, _)| keys.contains(&Some(*way)))
                        .count();
                    assert_eq!(reached, ways.len().min(ARROWS.len()), "seed {seed}");
                    let on = route_step(&game).to;
                    let arrow = keys
                        .iter()
                        .position(|&key| key == Some(on))
                        .expect("the way on has a key");
                    assert!(game.arrow(arrow));
                }
            }
        }
    }

    #[test]
    fn test_cycling_and_choosing_follows_the_preview() {
        for (tiling, crossings) in SETUPS {
            let mut game = game_for("cycle", tiling, crossings);
            let mut guard = 0;
            while !game.solved() {
                guard += 1;
                assert!(guard < 1000, "never reached the exit");
                assert!(game.preview().is_empty(), "nothing chosen yet");
                let here = game.here();
                let on = route_step(&game).to;
                // Cycle round to the way on, looking at each way's preview.
                let count = game.ways().len();
                let mut tries = 0;
                loop {
                    assert!(game.cycle());
                    tries += 1;
                    assert!(tries <= count, "the way on never came round");
                    let preview = game.preview();
                    assert!(!preview.is_empty(), "a preview goes somewhere");
                    if preview[0] == (Step { to: on, ..here }) {
                        break;
                    }
                }
                let preview = game.preview();
                let before = game.trail.len();
                assert!(game.choose());
                // The move followed the preview exactly.
                assert_eq!(&game.trail[before - 1..], &preview[..]);
            }
        }
    }

    #[test]
    fn test_choosing_the_way_back_follows_its_preview() {
        let mut game = game(2);
        let way = game.ways()[0].1;
        assert!(game.step(way.0, way.1));
        let walked = game.trail.clone();
        // Cycle to the way back and check its preview is the stretch just
        // walked.
        let back = game
            .ways_clockwise()
            .iter()
            .position(|(way, _)| way.is_none())
            .expect("a way back");
        for _ in 0..=back {
            assert!(game.cycle());
        }
        let preview = game.preview();
        assert_eq!(preview.last(), walked.last());
        assert!(game.choose());
        assert_eq!(game.trail.len(), walked.len() - preview.len() + 1);
        assert!(game.preview().is_empty(), "moving clears the choice");
    }

    #[test]
    fn test_moves_replay_to_the_same_trail() {
        for (tiling, crossings) in SETUPS {
            for seed in 0..2 {
                let id = format!("replay {seed}");
                let mut game = game_for(&id, tiling, crossings);
                // Wander: a few ways on, a step back, then the solution.
                for _ in 0..4 {
                    let way = game.ways()[0].1;
                    game.step(way.0, way.1);
                }
                let back = game.ways().into_iter().find(|(w, _)| w.is_none());
                if let Some((_, d)) = back {
                    game.step(d.0, d.1);
                }
                let moves = game.moves();
                let trail = game.trail.clone();
                let mut again = game_for(&id, tiling, crossings);
                assert_eq!(again.replay(&moves), moves.len(), "{tiling} {seed}");
                assert_eq!(again.trail, trail, "{tiling} {seed}");

                game.restart();
                walk_solution(&mut game);
                let mut solved = game_for(&id, tiling, crossings);
                solved.replay(&game.moves());
                assert!(solved.solved(), "{tiling} {seed}");
            }
        }
    }

    #[test]
    fn test_replay_stops_at_a_move_that_does_not_fit() {
        let mut game = game(1);
        assert_eq!(game.replay(""), 0);
        assert_eq!(game.trail.len(), 1);
        // Nonsense: sides out of range, letters, a side that is not open.
        assert_eq!(game.replay("9x"), 0);
        assert_eq!(game.replay("x1"), 0);
        assert_eq!(game.replay("-1"), 0);
        // A solution's moves with junk after them go as far as the junk.
        let mut other = game_for("other", "hexagonal", true);
        walk_solution(&mut other);
        let moves = other.moves();
        let mut same = game_for("other", "hexagonal", true);
        assert_eq!(same.replay(&format!("{moves}9zz")), moves.len());
        assert!(same.solved());
        // Another maze's moves go as far as they fit and no further.
        let made = game.replay(&moves);
        assert!(made <= moves.len());
        assert_eq!(game.moves().len(), made);
    }

    #[test]
    fn test_restart_goes_back_to_the_entrance() {
        let mut game = game(3);
        let start = game.position();
        walk_solution(&mut game);
        game.restart();
        assert_eq!(game.trail_length(), 1);
        assert_eq!(game.position(), start);
        let entrance = game.drawing.point(game.maze.cap_centre(game.maze.entrance));
        assert_eq!(start, entrance);
        assert_eq!(game.moves(), "");
    }

    #[test]
    fn test_from_id_is_deterministic_and_ids_differ() {
        let blocks = |id: &str| game_for(id, "hexagonal", true).maze.blocks;
        assert_eq!(blocks("demo"), blocks("demo"));
        assert_ne!(blocks("demo"), blocks("demo2"));
        assert_ne!(blocks("a"), blocks("b"));
        let a = game_for("demo", "hexagonal", true);
        assert_eq!(a.maze_svg(), game_for("demo", "hexagonal", true).maze_svg());
        // Crossings, size and tiling change the maze too.
        assert_ne!(
            a.maze_svg(),
            game_for("demo", "hexagonal", false).maze_svg()
        );
        assert_ne!(
            a.maze_svg(),
            Game::from_id("demo", "hexagonal", 26.0, 14.0, true).maze_svg()
        );
        assert_ne!(a.maze_svg(), game_for("demo", "square", true).maze_svg());
        // An unknown tiling is the hexagonal one.
        assert_eq!(
            a.maze_svg(),
            game_for("demo", "no such tiling", true).maze_svg()
        );
        // The seed is Lubra's: FNV-1a over the characters.
        assert_eq!(seed_of(""), 0x811c_9dc5);
        assert_eq!(seed_of("a"), 0xe40c_292c);
        assert_eq!(
            game_for("demo", "hexagonal", true).maze.seed,
            seed_of("demo")
        );
    }

    #[test]
    fn test_crossings_off_makes_no_bridges() {
        for id in ["a", "b", "c"] {
            let off = game_for(id, "hexagonal", false);
            assert!(off.maze.bridges.iter().all(Option::is_none));
        }
        let any = ["a", "b", "c"].iter().any(|id| {
            game_for(id, "hexagonal", true)
                .maze
                .bridges
                .iter()
                .any(Option::is_some)
        });
        assert!(any, "crossings on gives some bridges");
    }

    #[test]
    fn test_maze_svg_has_one_overlay_group_inside_the_flipped_group() {
        for (tiling, crossings) in SETUPS {
            let svg = game_for("svg", tiling, crossings).maze_svg();
            assert_eq!(svg.matches("id='overlay'").count(), 1);
            let flipped = svg
                .find("<g transform='translate(0,")
                .expect("the flipped group");
            let overlay = svg
                .find("<g id='overlay'></g>")
                .expect("an empty overlay group");
            assert!(overlay > flipped);
            // It is the last thing in the flipped group, which ends the drawing.
            let rest = &svg[overlay + "<g id='overlay'></g>".len()..];
            assert!(rest.trim_start().starts_with("</g>"), "{tiling}");
            assert!(!svg.contains("stroke='#c0392b'"), "no solution in it");
        }
    }

    #[test]
    fn test_maze_svg_has_the_tile_borders_first_in_the_flipped_group() {
        for (tiling, crossings) in SETUPS {
            let game = game_for("svg", tiling, crossings);
            let svg = game.maze_svg();
            assert_eq!(svg.matches("<g id='tiles'>").count(), 1, "{tiling}");
            let flipped = svg
                .find("<g transform='translate(0,")
                .expect("the flipped group");
            let opened = flipped + svg[flipped..].find('>').expect("a closed tag") + 1;
            // Underneath everything: the first thing drawn in the group.
            assert!(
                svg[opened..].trim_start().starts_with("<g id='tiles'>"),
                "{tiling}"
            );
            // One closed border per tile.
            let tiles = &svg[opened..];
            let path = &tiles[..tiles.find("</g>").expect("the group ends")];
            assert_eq!(
                path.matches('Z').count(),
                game.maze().tiling.faces.len(),
                "{tiling}"
            );
        }
    }

    /// Whether every tag in `markup` is closed and every group and mask is
    /// balanced, as a stand-in for an XML parser.
    fn well_formed(markup: &str) -> bool {
        let mut depth = 0_i32;
        let mut rest = markup;
        while let Some(open) = rest.find('<') {
            let Some(close) = rest[open..].find('>') else {
                return false;
            };
            let tag = &rest[open + 1..open + close];
            if tag.starts_with('/') {
                depth -= 1;
            } else if !tag.ends_with('/') {
                depth += 1;
            }
            if depth < 0 || tag.contains('<') {
                return false;
            }
            rest = &rest[open + close + 1..];
        }
        depth == 0 && !rest.contains('>')
    }

    #[test]
    fn test_overlay_is_well_formed_and_marks_the_player() {
        for (tiling, crossings) in SETUPS {
            let mut game = game_for("overlay", tiling, crossings);
            for solution in [false, true] {
                let overlay = game.overlay(solution);
                assert!(well_formed(&overlay), "{tiling}");
                let (x, y) = game.position();
                let dot = format!("<circle cx='{x:.1}' cy='{y:.1}' r='{PLAYER_RADIUS}'");
                assert!(overlay.contains(&dot), "{overlay}");
                assert!(overlay.contains("fill='#1d5fa6' fill-opacity='0.9'"));
                assert_eq!(overlay.contains("#c0392b"), solution);
            }
            // After a move, with a way chosen, trail and preview join in.
            assert!(game.step(0.0, 1.0));
            assert!(game.cycle());
            let overlay = game.overlay(true);
            assert!(well_formed(&overlay), "{tiling}");
            assert!(overlay.contains("stroke='#1d5fa6'"));
            assert!(overlay.contains("stroke='#8db8e8'"));
            // Mask ids may not repeat, or one route would hide another's track.
            let ids: Vec<&str> = overlay.split("<mask id='").skip(1).collect();
            let mut names: Vec<&str> = ids.iter().filter_map(|s| s.split('\'').next()).collect();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), ids.len());
        }
    }

    #[test]
    fn test_a_route_shows_over_and_under_a_bridge() {
        // Find a game whose solution passes under a bridge and show it.
        let game = (0..40)
            .map(|k| game_for(&format!("bridge {k}"), "hexagonal", true))
            .find(|game| {
                game.solution.iter().any(|step| {
                    let Some(bridge) = game.maze.bridges[step.face] else {
                        return false;
                    };
                    let block = &game.maze.blocks[step.face][bridge.under];
                    step.from
                        .zip(step.to)
                        .is_some_and(|(a, b)| block.contains(&a) && block.contains(&b))
                })
            })
            .expect("some solution passes under a bridge");
        let overlay = game.overlay(true);
        assert!(well_formed(&overlay));
        assert!(overlay.contains("<mask id='under-solution'"));
        assert!(overlay.contains("stroke-opacity='0.18'"));
    }

    #[test]
    fn test_tiny_sizes_do_not_panic() {
        for (w, h) in [
            (1.0, 1.0),
            (0.0, 0.0),
            (-3.0, 2.0),
            (f64::NAN, 5.0),
            (4.0, 4.0),
        ] {
            for tiling in [
                "hexagonal",
                "square",
                "triangular",
                "random",
                "triangle-hexagon",
            ] {
                for crossings in [true, false] {
                    let mut game = Game::from_id("tiny", tiling, w, h, crossings);
                    assert!(game.maze_svg().starts_with("<svg"));
                    assert!(well_formed(&game.overlay(true)));
                    walk_solution(&mut game);
                    assert!(game.solved(), "{tiling} {w}x{h}");
                }
            }
        }
        assert_eq!(Game::from_id("", "", 1.0, 1.0, true).moves(), "");
    }
}
