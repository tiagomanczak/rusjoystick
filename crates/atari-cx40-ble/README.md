# Atari CX40 BLE HID (Pico W)

This crate implements the Atari CX40 as a Bluetooth Low Energy HID joystick using Embassy Trouble and the Raspberry Pi Pico W CYW43 Bluetooth controller.

It is separate from the USB implementations and is only for the CX40.

## Wiring

The wiring matches the other CX40 implementations:

- GPIO0-7: active-low fire and button inputs
- GPIO8: left
- GPIO9: right
- GPIO10: up
- GPIO11: down
- Ground: controller ground

## Build

The CYW43 firmware is downloaded automatically by `build.rs`:

```shell
cargo build --release -p atari-cx40-ble
```

The device advertises as `Rusjoystick CX40 BLE` with the standard HID service and a three-byte report:

- Byte 0: X axis
- Byte 1: Y axis
- Byte 2: eight button bits

## BLE details

- [BLE host configuration and GATT characteristics](../../docs/ble-host.md)
- [HID report layout and descriptor](../../docs/hid-report.md)
