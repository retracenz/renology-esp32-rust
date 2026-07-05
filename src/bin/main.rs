#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use core::cell::RefCell;

use bt_hci::controller::ExternalController;
use critical_section::Mutex as CsMutex;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::ble::controller::BleConnector;
use gobblegobble::renogy::{self, ChargerData};
use gobblegobble::sh1106::Sh1106;
use trouble_host::prelude::*;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

/// Collects the address of the first advertising Renogy device seen by the
/// BLE host runner. Written from the runner's event handler, taken by main.
struct AdvWatcher {
    found: CsMutex<RefCell<Option<(AddrKind, BdAddr)>>>,
}

impl AdvWatcher {
    const fn new() -> Self {
        Self {
            found: CsMutex::new(RefCell::new(None)),
        }
    }

    fn take(&self) -> Option<(AddrKind, BdAddr)> {
        critical_section::with(|cs| self.found.borrow_ref_mut(cs).take())
    }
}

impl EventHandler for AdvWatcher {
    fn on_adv_reports(&self, mut reports: LeAdvReportsIter<'_>) {
        while let Some(Ok(report)) = reports.next() {
            let mut name: Option<&[u8]> = None;
            for ad in AdStructure::decode(report.data).flatten() {
                match ad {
                    AdStructure::CompleteLocalName(n) | AdStructure::ShortenedLocalName(n) => {
                        name = Some(n);
                    }
                    _ => {}
                }
            }
            let Some(name) = name.and_then(|n| core::str::from_utf8(n).ok()) else {
                continue;
            };
            println!("adv: {:?} {:?} rssi {}", report.addr, name, report.rssi);
            if name.starts_with(renogy::NAME_PREFIX) {
                critical_section::with(|cs| {
                    *self.found.borrow_ref_mut(cs) = Some((report.addr_kind, report.addr));
                });
            }
        }
    }
}

fn show_status(display: &mut Sh1106<'_>, line1: &str, line2: &str) {
    display.clear();
    display.draw_text("gobble gobble", 12, 0, 1);
    display.draw_text(line1, 0, 28, 1);
    display.draw_text(line2, 0, 40, 1);
    display.flush();
}

fn fmt_volts(x10: u16) -> String {
    format!("{}.{}V", x10 / 10, x10 % 10)
}

fn show_data(display: &mut Sh1106<'_>, d: &ChargerData) {
    display.clear();
    display.draw_text("HOUSE", 0, 0, 1);
    let soc = format!("{}%", d.soc);
    display.draw_text(&soc, 128 - 8 * soc.len() as i32, 0, 1);
    display.draw_text(&fmt_volts(d.house_volts_x10), 0, 9, 2);

    display.draw_text("START", 0, 28, 1);
    display.draw_text(&fmt_volts(d.start_volts_x10), 0, 37, 2);

    let amps = d.charge_amps_x100;
    let bottom = format!("CHG {}.{}A SOL {}W", amps / 100, (amps % 100) / 10, d.solar_watts);
    display.draw_text(&bottom, 0, 56, 1);
    display.flush();
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

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
    show_status(&mut display, "BLE starting", "");

    let connector =
        BleConnector::new(peripherals.BT, Default::default()).expect("BLE init failed");
    let controller: ExternalController<_, 20> = ExternalController::new(connector);
    let mut resources: HostResources<DefaultPacketPool, 1, 2> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources);
    let Host {
        central,
        mut runner,
        ..
    } = stack.build();

    let watcher = AdvWatcher::new();
    let display = &mut display;

    let app = async {
        let mut central_slot = Some(central);
        loop {
            let central = central_slot.take().unwrap();

            // Scan until a Renogy device shows up.
            show_status(display, "Scanning for", "Renogy...");
            println!("scanning for {}*", renogy::NAME_PREFIX);
            let mut scanner = Scanner::new(central);
            let (addr_kind, addr) = loop {
                match scanner.scan(&ScanConfig::default()).await {
                    Ok(session) => {
                        let mut found = None;
                        for _ in 0..100 {
                            if let Some(t) = watcher.take() {
                                found = Some(t);
                                break;
                            }
                            Timer::after_millis(100).await;
                        }
                        drop(session);
                        if let Some(t) = found {
                            break t;
                        }
                    }
                    Err(e) => {
                        println!("scan error: {:?}", e);
                        Timer::after_secs(1).await;
                    }
                }
            };
            let mut central = scanner.into_inner();

            println!("connecting to {:?}", addr);
            show_status(display, "Connecting...", "");
            let conn_cfg = ConnectConfig {
                connect_params: Default::default(),
                scan_config: ScanConfig {
                    filter_accept_list: &[(addr_kind, &addr)],
                    ..Default::default()
                },
            };
            let conn = match with_timeout(Duration::from_secs(15), central.connect(&conn_cfg)).await
            {
                Ok(Ok(conn)) => conn,
                Ok(Err(e)) => {
                    println!("connect failed: {:?}", e);
                    central_slot = Some(central);
                    continue;
                }
                Err(_) => {
                    println!("connect timed out");
                    central_slot = Some(central);
                    continue;
                }
            };

            println!("connected, starting GATT");
            let client = match GattClient::<_, DefaultPacketPool, 10>::new(&stack, &conn).await {
                Ok(c) => c,
                Err(e) => {
                    println!("gatt client failed: {:?}", e);
                    central_slot = Some(central);
                    continue;
                }
            };

            let poll = async {
                let services = client
                    .services_by_uuid(&Uuid::from(renogy::WRITE_SERVICE))
                    .await?;
                let write_service = services.first().ok_or(Error::NotFound)?.clone();
                let write_char: Characteristic<[u8]> = client
                    .characteristic_by_uuid(&write_service, &Uuid::from(renogy::WRITE_CHAR))
                    .await?;

                let services = client
                    .services_by_uuid(&Uuid::from(renogy::NOTIFY_SERVICE))
                    .await?;
                let notify_service = services.first().ok_or(Error::NotFound)?.clone();
                let notify_char: Characteristic<[u8]> = client
                    .characteristic_by_uuid(&notify_service, &Uuid::from(renogy::NOTIFY_CHAR))
                    .await?;

                let mut listener = client.subscribe(&notify_char, false).await?;
                println!("subscribed, polling charger");

                let mut response = renogy::ResponseBuffer::new();
                let mut misses = 0u32;
                loop {
                    let request =
                        renogy::read_request(renogy::DEVICE_ID, renogy::DYNAMIC_REG, renogy::DYNAMIC_WORDS);
                    client
                        .write_characteristic_without_response(&write_char, &request)
                        .await?;

                    response.reset();
                    let mut data = None;
                    while data.is_none() {
                        match with_timeout(Duration::from_secs(3), listener.next()).await {
                            Ok(notification) => {
                                if let Some(words) = response.feed(notification.as_ref()) {
                                    data = ChargerData::parse(words);
                                    break;
                                }
                            }
                            Err(_) => break, // response timeout
                        }
                    }

                    match data {
                        Some(d) => {
                            misses = 0;
                            println!(
                                "house {}.{}V soc {}% start {}.{}V chg {}A/100 solar {}W",
                                d.house_volts_x10 / 10,
                                d.house_volts_x10 % 10,
                                d.soc,
                                d.start_volts_x10 / 10,
                                d.start_volts_x10 % 10,
                                d.charge_amps_x100,
                                d.solar_watts
                            );
                            show_data(display, &d);
                        }
                        None => {
                            misses += 1;
                            println!("no response ({})", misses);
                            if misses >= 5 {
                                return Err(Error::Timeout.into());
                            }
                        }
                    }

                    Timer::after_secs(1).await;
                }
            };

            let result: Either<_, Result<(), BleHostError<_>>> =
                select(client.task(), poll).await;
            match result {
                Either::First(r) => println!("gatt task ended: {:?}", r),
                Either::Second(r) => println!("poll ended: {:?}", r),
            }
            show_status(display, "Disconnected", "retrying...");
            Timer::after_secs(2).await;
            central_slot = Some(central);
        }
    };

    join(runner.run_with_handler(&watcher), app).await;
}
