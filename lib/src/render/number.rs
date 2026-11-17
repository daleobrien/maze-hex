//! Writing coordinates into SVG path data quickly.
//!
//! A large maze's drawing holds about a million numbers, and `format!` with a
//! precision spends most of the drawing time on them. [`push_fixed`] writes the
//! same text as `{:.N}` (N = 1 or 2) far faster.

use std::fmt::Write;

/// Appends `x` with `decimals` places (1 or 2), exactly as `{:.1}` or `{:.2}`
/// would write it.
pub fn push_fixed(out: &mut String, x: f64, decimals: u32) {
    let scale = 10_f64.powi(decimals as i32);
    let scaled = x * scale;
    // Near a halfway point the product's own rounding could tip the result
    // the other way from `format!`, which rounds the exact value; those (and
    // anything too large or not finite) go through `format!` itself.
    let fraction = scaled - scaled.floor();
    if !scaled.is_finite() || scaled.abs() > 1e15 || (fraction - 0.5).abs() < 1e-6 {
        let _ = write!(out, "{x:.prec$}", prec = decimals as usize);
        return;
    }
    let units = scaled.round() as i64;
    // `format!` keeps the sign of a negative number that rounds to zero.
    if units < 0 || (units == 0 && x.is_sign_negative()) {
        out.push('-');
    }
    // The digits, last first, with the point `decimals` places from the end
    // and at least one digit before it.
    let mut units = units.unsigned_abs();
    let mut digits = [0_u8; 24];
    let mut len = 0;
    while len <= decimals as usize + 1 || units > 0 {
        if len == decimals as usize {
            digits[len] = b'.';
            len += 1;
            continue;
        }
        digits[len] = b'0' + (units % 10) as u8;
        units /= 10;
        len += 1;
    }
    digits[..len].reverse();
    // Only ASCII digits, '.' and nothing else were written, so this is UTF-8.
    if let Ok(text) = std::str::from_utf8(&digits[..len]) {
        out.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn fixed(x: f64, decimals: u32) -> String {
        let mut s = String::new();
        push_fixed(&mut s, x, decimals);
        s
    }

    #[test]
    fn writes_what_format_writes() {
        let mut rng = Rng::new(7);
        let special = [
            0.0, -0.0, -0.04, -0.05, 0.05, 0.25, 0.35, 1.005, -1.25, 99.95, 1e16,
        ];
        let random = (0..200_000).map(|k| {
            let x = (rng.unit() - 0.5) * 4000.0;
            if k % 3 == 0 {
                (x * 10.0).round() / 10.0 + 0.05
            } else {
                x
            }
        });
        for x in special.into_iter().chain(random) {
            assert_eq!(fixed(x, 1), format!("{x:.1}"), "{x}");
            assert_eq!(fixed(x, 2), format!("{x:.2}"), "{x}");
        }
    }
}
