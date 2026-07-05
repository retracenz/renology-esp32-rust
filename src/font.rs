//! 8x8 ASCII font backed by the public-domain `font8x8` glyph set.
//! Each glyph is 8 rows; in font8x8 the LSB is the leftmost pixel.

use font8x8::legacy::BASIC_LEGACY;

pub const GLYPH_WIDTH: i32 = 8;
pub const GLYPH_HEIGHT: i32 = 8;

pub fn glyph(c: char) -> [u8; 8] {
    let idx = c as usize;
    if idx < BASIC_LEGACY.len() {
        BASIC_LEGACY[idx]
    } else {
        [0; 8]
    }
}
