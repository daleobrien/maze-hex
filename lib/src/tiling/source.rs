//! Where tilings come from. The maze and drawing code only ever see a
//! [`Tiling`], so a periodic unit cell and a random generator are interchangeable
//! behind one trait and the command line can pick either by name.

use super::{
    catalogue,
    merge::TriangleHexagon,
    rows::{RowMix, RowMixHexagon},
    Region, Tiling, UnitCell,
};
use crate::rng::Rng;

/// Something that can fill a region with tiles.
pub trait TilingSource {
    /// The name used on the command line, e.g. `hexagonal`.
    fn name(&self) -> &str;

    /// Fills `region`. Periodic sources ignore `rng`; free mixes use it, so the
    /// same seed gives the same tiling.
    fn build(&self, region: Region, rng: &mut Rng) -> Tiling;
}

impl TilingSource for UnitCell {
    fn name(&self) -> &str {
        self.name
    }

    fn build(&self, region: Region, _rng: &mut Rng) -> Tiling {
        Tiling::build(self, region)
    }
}

/// Lets a catalogue entry (`&'static UnitCell`) be boxed as a source.
impl<T: TilingSource + ?Sized> TilingSource for &T {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn build(&self, region: Region, rng: &mut Rng) -> Tiling {
        (**self).build(region, rng)
    }
}

/// The names of the free-mix generators, after the periodic tilings.
const GENERATORS: [&str; 3] = ["triangle-hexagon", "row-mix", "row-mix-hexagon"];

/// Every source name: the periodic tilings, then the generators.
pub fn source_names() -> Vec<&'static str> {
    catalogue::ALL
        .iter()
        .map(|cell| cell.name)
        .chain(GENERATORS)
        .collect()
}

/// The source called `name`, with its default settings, if there is one.
pub fn source_by_name(name: &str) -> Option<Box<dyn TilingSource>> {
    if let Some(cell) = catalogue::by_name(name) {
        return Some(Box::new(cell));
    }
    match name {
        "triangle-hexagon" => Some(Box::new(TriangleHexagon::default())),
        "row-mix" => Some(Box::new(RowMix::default())),
        "row-mix-hexagon" => Some(Box::new(RowMixHexagon::default())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_finds_a_source_with_that_name() {
        let names = source_names();
        assert!(names.len() > catalogue::ALL.len());
        for name in names {
            let source = source_by_name(name).expect(name);
            assert_eq!(source.name(), name);
        }
        assert!(source_by_name("no-such-tiling").is_none());
    }

    #[test]
    fn every_source_builds_a_tiling() {
        for name in source_names() {
            let source = source_by_name(name).expect(name);
            let tiling = source.build(Region::rectangle(14.0, 9.0), &mut Rng::new(1));
            assert!(tiling.faces.len() > 20, "{name}");
        }
    }

    #[test]
    fn a_unit_cell_ignores_the_rng() {
        let source = source_by_name("hexagonal").expect("hexagonal exists");
        let region = Region::rectangle(14.0, 9.0);
        let a = source.build(region, &mut Rng::new(1));
        let b = source.build(region, &mut Rng::new(2));
        assert_eq!(a.points, b.points);
    }
}
