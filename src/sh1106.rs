//! SH1106 128x64 OLED driver, 4-wire SPI, ported from the old MicroPython
//! sh1106.py driver (same init sequence, page-mode writes, VLSB framebuffer).

use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::gpio::Output;
use esp_hal::spi::master::Spi;

use crate::font;

pub const WIDTH: i32 = 128;
pub const HEIGHT: i32 = 64;
const PAGES: usize = (HEIGHT as usize) / 8;
const BUF_SIZE: usize = (WIDTH as usize) * PAGES;
// The SH1106 RAM is 132 columns wide; the 128-column panel is centred, so
// every page write starts at column 2.
const COLUMN_OFFSET: u8 = 2;

pub struct Sh1106<'d> {
    spi: Spi<'d, Blocking>,
    dc: Output<'d>,
    cs: Output<'d>,
    rst: Output<'d>,
    buf: [u8; BUF_SIZE],
}

impl<'d> Sh1106<'d> {
    pub fn new(
        spi: Spi<'d, Blocking>,
        dc: Output<'d>,
        cs: Output<'d>,
        rst: Output<'d>,
        delay: &Delay,
    ) -> Self {
        let mut display = Self {
            spi,
            dc,
            cs,
            rst,
            buf: [0; BUF_SIZE],
        };
        display.reset(delay);
        display.write_cmd(0xAE); // display off
        display.write_cmd(0xA0); // segment remap: normal
        display.write_cmd(0xC0); // scan direction: normal
        display.write_cmd(0xA6); // non-inverted
        display.flush(); // push the cleared framebuffer
        display.write_cmd(0xAF); // display on
        display
    }

    fn reset(&mut self, delay: &Delay) {
        self.rst.set_high();
        delay.delay_millis(1);
        self.rst.set_low();
        delay.delay_millis(20);
        self.rst.set_high();
        delay.delay_millis(20);
    }

    fn write_cmd(&mut self, cmd: u8) {
        self.dc.set_low();
        self.cs.set_low();
        self.spi.write(&[cmd]).unwrap();
        self.cs.set_high();
    }

    pub fn clear(&mut self) {
        self.buf.fill(0);
    }

    pub fn set_pixel(&mut self, x: i32, y: i32) {
        if (0..WIDTH).contains(&x) && (0..HEIGHT).contains(&y) {
            self.buf[(y as usize / 8) * (WIDTH as usize) + x as usize] |= 1 << (y as usize % 8);
        }
    }

    /// Draw `text` with its top-left corner at (x, y), each font pixel
    /// expanded to a `scale` x `scale` block. Off-screen pixels are clipped.
    pub fn draw_text(&mut self, text: &str, x: i32, y: i32, scale: i32) {
        for (i, c) in text.chars().enumerate() {
            let glyph = font::glyph(c);
            let char_x = x + (i as i32) * font::GLYPH_WIDTH * scale;
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..font::GLYPH_WIDTH {
                    if bits >> (7 - col) & 1 != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                self.set_pixel(
                                    char_x + col * scale + sx,
                                    y + (row as i32) * scale + sy,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Send the framebuffer to the display, page by page.
    pub fn flush(&mut self) {
        for page in 0..PAGES {
            self.write_cmd(0xB0 | page as u8); // page address
            self.write_cmd(COLUMN_OFFSET & 0x0F); // low column address
            self.write_cmd(0x10 | (COLUMN_OFFSET >> 4)); // high column address
            self.dc.set_high();
            self.cs.set_low();
            let start = page * (WIDTH as usize);
            self.spi.write(&self.buf[start..start + WIDTH as usize]).unwrap();
            self.cs.set_high();
        }
    }
}
