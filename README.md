# Renology Reader

An ESP32 battery monitor for a Renogy RBC2125DS-21W DC-DC charger. It connects
to the charger over Bluetooth LE, polls it once a second, and shows live data
on a small OLED:

- **Main screen (20 s)**: house battery voltage + state of charge, starter
  battery voltage, charging state (`STDBY`/`BOOST`/`FLOAT`/`DCDC`/...), charge
  current, and input power.
- **Extras screen (20 s)**: alternator and solar input volts/amps, input
  watts, controller/battery temperatures, today's min-max battery voltage,
  amp-hours charged today, and fault flags.

While not connected it shows scan/connect status, and it reconnects by itself
if the link drops.

The old MicroPython version of this project (ADC voltage dividers instead of
Bluetooth) is kept for reference in `esp32-micropython-backup/`. The directory
is still called `gobblegobble` after the project's silly first incarnation; the
crate itself is `renology-reader`.

## Hardware

- ESP32 (original, Xtensa — WROOM-32E module)
- SH1106 128x64 OLED on SPI:

  | OLED pin | ESP32 GPIO |
  |----------|-----------|
  | SCK      | 18        |
  | MOSI     | 23        |
  | CS       | 5         |
  | D/C      | 4         |
  | RST      | 2         |

- Renogy RBC2125DS-21W within BLE range, advertising as `BT-TH-XXXXXXXX`.

## How it talks to the charger

The charger speaks Modbus RTU tunnelled over BLE GATT (same protocol as the
Renogy BT-1/BT-2 dongles): requests are written to characteristic `0xFFD1`,
responses arrive as notifications on `0xFFF1` and are reassembled/CRC-checked
(`src/renogy.rs`). The main data block is registers `0x0100` (voltages,
currents, SOC, temperatures) plus `0x0120` (charging state and fault bits).

Note: this model reports the active input's power under the solar/MPPT
registers even when charging from the alternator, so the display labels that
figure as input power (`IN`).

## Setup

1. Install the Espressif Rust toolchain (installs the `esp` rustup toolchain
   and Xtensa GCC, and writes `~/export-esp.sh`):

   ```sh
   cargo install espup
   espup install
   ```

2. Install the flasher:

   ```sh
   cargo install espflash
   ```

3. Every shell you build in needs the Xtensa tools on PATH first:

   ```sh
   source ~/export-esp.sh
   ```

   Without this, the build fails at the link step with
   `linker 'xtensa-esp32-elf-gcc' not found`.

## Build, flash, monitor

Find your serial port with `ls /dev/cu.usbserial*` (this board shows up as
`/dev/cu.usbserial-140`).

```sh
source ~/export-esp.sh

# Build
cargo build --release

# Flash
espflash flash --chip esp32 --port /dev/cu.usbserial-140 \
    target/xtensa-esp32-none-elf/release/renology-reader

# Watch the serial log (scan results, connection state, readings)
espflash monitor --port /dev/cu.usbserial-140
```

Or do all three in one step — the cargo runner is configured to flash and
monitor automatically:

```sh
cargo run --release
```

## Code layout

| Path                | What it is                                          |
|---------------------|-----------------------------------------------------|
| `src/bin/main.rs`   | BLE scan/connect/poll loop, screen layouts, timing  |
| `src/renogy.rs`     | Modbus framing, CRC, register map, data decoding    |
| `src/sh1106.rs`     | SH1106 OLED driver (SPI, framebuffer, text)         |
| `src/font.rs`       | 8x8 ASCII font (wraps the `font8x8` crate)          |

BLE central runs on [`trouble-host`](https://crates.io/crates/trouble-host)
over esp-radio's HCI controller, with the embassy async executor provided by
`esp-rtos`.

## Troubleshooting

- **`linker 'xtensa-esp32-elf-gcc' not found`** — you forgot
  `source ~/export-esp.sh`.
- **rust-analyzer shows E0308 errors that `cargo build` doesn't** — the editor
  failed to index the `alloc` sysroot crate for the Xtensa toolchain; restart
  the language server. The compiler is the source of truth.
- **Stuck on "Scanning for Renogy..."** — the charger stops advertising for a
  short while after a connection drops (e.g. right after reflashing); give it
  up to a minute. The serial log prints every device it sees.
