# CX40 BLE HID report

The `atari-cx40-ble` firmware exposes the CX40 as a Bluetooth HID joystick.
The HID Report Map is the descriptor in `crates/atari-cx40-ble/src/main.rs`;
it describes a 3-byte input report with no explicit Report ID:

| Byte | Field | Encoding |
| --- | --- | --- |
| 0 | X axis | Signed 8-bit value, logical range -127 to +127 |
| 1 | Y axis | Signed 8-bit value, logical range -127 to +127 |
| 2 | Buttons 1-8 | Eight 1-bit values, least-significant bit first |

The current digital joystick inputs produce axis values of `-127`, `0`, or
`+127`. They are sent as two's-complement bytes: `-127` is `0x81`, center is
`0x00`, and `+127` is `0x7f`. In this firmware, left/up are negative and
right/down are positive. If both directions on one axis are pressed, the
first condition in `get_report` wins: left over right and up over down.

The button byte maps GPIO0 through GPIO7 to bits 0 through 7, respectively.
Inputs are active-low, so a pressed switch sets its bit to 1. For example, if
GPIO0 and GPIO2 are pressed, the button byte is `0x05`. GPIO8-11 are reserved
for directions and are not included in this button byte.

## How the descriptor defines those bytes

The HID Report Map characteristic (UUID `0x2a4b`) contains a USB-HID-format
descriptor, as required by HID over GATT. Its relevant items declare:

- Generic Desktop / Joystick as an Application collection.
- X and Y as two 8-bit, signed, absolute values with logical limits -127..127.
- Button usages 1..8 as eight 1-bit absolute values with logical limits 0..1.

The two axis fields consume 16 bits; the eight buttons consume the following
8 bits. Since the map has no Report ID item, the report has no extra ID byte.
The Report Reference descriptor (UUID `0x2908`) accordingly identifies report
ID 0 and type Input (`0x01`).

## GATT delivery

The Report characteristic (UUID `0x2a4d`) is readable and supports Notify.
After connecting, a central reads the Report Map and other HID attributes, then
enables notifications through the characteristic's Client Characteristic
Configuration Descriptor (CCCD). The firmware attempts to send the same
3-byte format every 10 ms (100 Hz), even if no input changed. The current
`report_task` ends when a notification attempt returns an error; it does not
retry that task.

## Related standards

- [HID over GATT Profile (HOGP), 1.0](https://www.bluetooth.com/specifications/specs/hid-over-gatt-profile-1-0/)
  defines how HID reports and HID characteristics are carried over BLE.
- [Human Interface Device Service (HIDS), 1.0](https://www.bluetooth.com/specifications/specs/human-interface-device-service-1-0/)
  defines the HID Service and its Report, Report Map, and related characteristics.
- [Bluetooth Core Specification](https://www.bluetooth.com/specifications/specs/core-specification/)
  defines the underlying ATT/GATT and GAP procedures used for characteristic
  access, notifications, advertising, and connections.
- [USB HID Usage Tables](https://usb.org/sites/default/files/hut1_5.pdf)
  defines the Generic Desktop and Button usages referenced by the Report Map.

The Report Map is HID descriptor syntax; HOGP specifies how that descriptor is
discovered and how its report values travel over BLE. It is not a custom
Bluetooth packet format.
