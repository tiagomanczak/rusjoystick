# Atari CX40 (Embassy)

This is a separate Embassy-based firmware for the same CX40 wiring as [`atari-cx40`](../atari-cx40/README.md). It uses Embassy's async RP2040 HAL and USB HID stack; it does not share firmware code with the other workspace crates.

## What is Embassy?

Embassy is an embedded Rust framework built around `async` tasks:

- The **executor** runs async functions and schedules them when they can make progress.
- Embassy's **HAL** configures the RP2040 GPIO and USB peripherals.
- The USB device's `run()` future must keep being polled to handle enumeration and USB traffic.
- The input loop awaits a timer rather than busy-waiting. This gives the USB future time to run while the firmware waits between joystick reports.

The firmware sends the same three-byte joystick report shape as the original CX40 project: X axis, Y axis, then eight button bits. GPIO inputs use pull-ups because joystick switches connect the signal to ground (active low).

## Wiring

The digital wiring matches the original CX40 firmware:

```text
GPIO0  <- DB9 pin 6 / Fire
GPIO8  <- DB9 pin 3 / Left
GPIO9  <- DB9 pin 4 / Right
GPIO10 <- DB9 pin 1 / Up
GPIO11 <- DB9 pin 2 / Down
GPIO13 -> Status LED (not used by this Embassy firmware)
GND    <- DB9 pin 8
```

GPIO1 through GPIO7 are also read as optional active-low buttons. Do not connect DB9 pin 7 (+5 V) to the Pico. The optional `analog` feature reads GPIO26/ADC0; only use a signal that is within the RP2040 ADC's valid 0–3.3 V range.

## Build and flash

```shell
cargo check -p atari-cx40-embassy
cargo check -p atari-cx40-embassy --features analog
cargo build --release -p atari-cx40-embassy
cargo run --release -p atari-cx40-embassy
```

The default build uses the digital D-pad. Build with `--features analog` to replace the X-axis value with GPIO26/ADC0. As with the original project, `cargo run` uses the workspace runner configured in `.cargo/config.toml` and requires a compatible probe; the ELF can also be loaded with another RP2040 flashing method.

The build generates `generated_sdl_mapping.txt` in the Cargo target profile directory. This crate uses the same USB VID/PID and HID joystick mapping as the original CX40 firmware.
