//! The area a tiling fills: a box from the origin, and the shape drawn in it.

/// The outline a tiling is clipped to, inside the width × height box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    /// The whole box.
    #[default]
    Rectangle,
    /// The largest regular hexagon that fits in the box, with a vertex at the
    /// middle of the left and right sides, centred in the box.
    Hexagon,
    /// The largest circle that fits in the box, centred in the box.
    Circle,
}

/// The area a tiling fills, in edge lengths, from the origin.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub width: f64,
    pub height: f64,
    pub shape: Shape,
    /// Whether to drop the spiky tiles clipping leaves on the edge. On by
    /// default; see `Tiling::smoothed`.
    pub smooth: bool,
}

/// How far outside a curved or slanted outline a tile centre may be and still
/// count as inside. Centres of symmetric tilings often sit exactly on the
/// outline, and float rounding must not decide which side they fall on.
const SLACK: f64 = 1e-9;

impl Region {
    /// A `width` × `height` rectangle, with smoothing on.
    pub const fn rectangle(width: f64, height: f64) -> Self {
        Region {
            width,
            height,
            shape: Shape::Rectangle,
            smooth: true,
        }
    }

    /// The same box with another outline.
    pub const fn with_shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    /// The same region with boundary smoothing turned off.
    pub const fn without_smoothing(mut self) -> Self {
        self.smooth = false;
        self
    }

    /// Whether the point `(x, y)` is inside the shape.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (dx, dy) = ((x - self.width / 2.0).abs(), (y - self.height / 2.0).abs());
        match self.shape {
            Shape::Rectangle => (0.0..=self.width).contains(&x) && (0.0..=self.height).contains(&y),
            Shape::Hexagon => {
                let r = (self.width / 2.0).min(self.height / 3f64.sqrt());
                let h = 3f64.sqrt();
                dy <= h / 2.0 * r + SLACK && h * dx + dy <= h * r + SLACK
            }
            Shape::Circle => {
                let r = self.width.min(self.height) / 2.0;
                dx * dx + dy * dy <= r * r + SLACK
            }
        }
    }
}
