#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
};
use esp_hal::timer::timg::TimerGroup;
use esp_radio::ble::controller::BleConnector;
use gobblegobble::sh1106::{self, Sh1106};


#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32 -o esp32-wroom-32e -o unstable-hal -o alloc -o wifi -o ble-bleps -o zed


    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
                    // for use, but check the datasheet of the module for more information on them.
                    // - GPIO0
// - GPIO2
// - GPIO5
// - GPIO12
// - GPIO15
// These GPIO pins are in use by some feature of the module and should not be used.
                        let _ = peripherals.GPIO6;
    let _ = peripherals.GPIO7;
    let _ = peripherals.GPIO8;
    let _ = peripherals.GPIO9;
    let _ = peripherals.GPIO10;
    let _ = peripherals.GPIO11;
    let _ = peripherals.GPIO16;
    let _ = peripherals.GPIO20;


    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);
    // COEX needs more RAM - so we've added some more
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    let (mut _wifi_controller, _interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");
    let _connector = BleConnector::new(peripherals.BT, Default::default());

    // SH1106 OLED wiring, same as the old MicroPython project:
    // SCK=GPIO18, MOSI=GPIO23, CS=GPIO5, D/C=GPIO4, RST=GPIO2
    let delay = Delay::new();
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(4))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO23);
    let dc = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO2, Level::High, OutputConfig::default());
    let mut display = Sh1106::new(spi, dc, cs, rst, &delay);

    const TEXT: &str = "gobble gobble";
    const SCALE: i32 = 2;
    let text_width = TEXT.len() as i32 * 8 * SCALE;
    let text_height = 8 * SCALE;
    // The 2x text is wider than the screen, so x pans between showing the
    // left edge (0) and the right edge (negative offset).
    let min_x = sh1106::WIDTH - text_width;
    let max_y = sh1106::HEIGHT - text_height;

    let (mut x, mut y) = (0, max_y / 2);
    let (mut dx, mut dy) = (-2, 1);

    loop {
        display.clear();
        display.draw_text(TEXT, x, y, SCALE);
        display.flush();

        x += dx;
        if x <= min_x || x >= 0 {
            x = x.clamp(min_x, 0);
            dx = -dx;
        }
        y += dy;
        if y <= 0 || y >= max_y {
            y = y.clamp(0, max_y);
            dy = -dy;
        }

        delay.delay_millis(20);
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}
