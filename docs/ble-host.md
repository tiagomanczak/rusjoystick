# CX40 BLE host and GATT configuration

This note explains the Bluetooth host setup in
`crates/atari-cx40-ble/src/main.rs`. The Pico W's CYW43 chip is the Bluetooth
controller (radio and HCI transport); Embassy runs the hardware tasks and
Trouble provides the BLE host, peripheral role, and GATT server.

## Startup and host resources

1. The firmware loads the CYW43 vendor firmware, Bluetooth firmware, NVRAM,
   and regulatory data, then connects the RP2040 to the chip over PIO SPI.
   `cyw43_task` keeps the controller running.
2. Before starting Trouble, `hci_random_seed` obtains four 8-byte values using
   the controller's LE Rand HCI command and uses the resulting 32 bytes to seed
   a ChaCha20 random generator for the host's security functions. This RNG seed
   is separate from the Bluetooth device address.
3. `HostResources<DefaultPacketPool, 1, 2>` bounds memory for one connection
   and two simultaneous L2CAP channels. The packet pool uses Trouble's
   `default-packet-pool-mtu-128` feature. These are compile-time resource
   capacities, not a promise that every possible BLE procedure is supported.
4. The builder is configured with a fixed six-byte random address and
   `IoCapabilities::NoInputNoOutput`, then built into a host runner and
   peripheral. The address bytes are constant in this firmware: they are not
   regenerated or rotated at each boot. Two devices using this same address
   at the same time can conflict. The I/O capability says there is no display
   or keyboard for pairing; it does not by itself require pairing, bonding, or
   link encryption.

The host runner processes HCI events concurrently with the peripheral loop.
The peripheral advertises, accepts one connection, attaches the HID GATT
attribute table, and runs GATT-event and report tasks for that connection.
When either per-connection task ends, the loop proceeds to advertising again.

## Advertising and HID service

The peripheral advertises as `Rusjoystick CX40 BLE` using connectable,
scannable, undirected legacy advertising. The advertising data contains:

- General Discoverable and BR/EDR Not Supported flags.
- The 16-bit HID service UUID `0x1812` (encoded little-endian in the payload).
- The complete local name.

Advertising only helps a central find the device. It does not carry the full
HID service; after connecting, the central discovers that service through
GATT. The GATT server exposes:

| Characteristic | UUID | Purpose in this firmware |
| --- | --- | --- |
| HID Information | `0x2a4a` | Reports HID version 1.11, country code 0, and flags `0x01`. |
| Report Map | `0x2a4b` | Readable descriptor defining the 3-byte joystick input. |
| HID Control Point | `0x2a4c` | Writable control characteristic; writes are accepted but not acted on. |
| Report | `0x2a4d` | Readable and notifiable 3-byte Input report. |
| Protocol Mode | `0x2a4e` | Writable characteristic; writes do not change the report format. |

The Report characteristic has a Report Reference descriptor (`0x2908`) with
report ID 0 and type Input. A central must enable its CCCD to receive
notifications. The report fields and notification behavior are detailed in
[CX40 BLE HID report](hid-report.md).

The HID Information flags byte `0x01` sets the HIDS RemoteWake bit. The current
firmware accepts HID Control Point writes but does not implement suspend or
remote-wake behavior, so the bit describes the advertised attribute value,
not a completed wake-up feature. Similarly, Protocol Mode is exposed as
write-only and the application does not switch between boot and report formats.
These are implementation limitations to consider when testing with strict
HOGP hosts.

## Pairing and ATT event handling

The GATT event task logs pairing success/failure and incoming ATT requests.
Reads and writes are accepted through Trouble's GATT event API and their ATT
responses are sent. The configured `NoInputNoOutput` capability is not a
security policy: review Trouble's security configuration and the central's
behavior separately if a product requires authenticated or encrypted access.

The report task calls `notify` every 10 ms, including when input is unchanged.
Trouble 0.6 updates the Report characteristic value and returns successfully
without sending a notification when the peer has not enabled notifications yet.
Once the peer subscribes, later calls send notifications. If `notify` returns
an actual error, this firmware ends the report task without retrying; the
connection's GATT event task may continue until disconnect.

## Recognizing and testing it on Linux

The service UUID in the advertisement (`0x1812`) lets a BLE central find the
device. Linux does not create a joystick event device just from that UUID:
BlueZ must connect, discover the HID Service and Report Map, and hand the HID
report to Linux's HID/input stack. Once that succeeds, the joystick appears as
a Linux input device, usually under `/dev/input/event*`. `evtest` reads those
kernel input events; it does not scan for BLE devices or connect to them.

On a Debian/Ubuntu-style system, install the tools if needed:

```shell
sudo apt install bluez evtest
```

With the Pico W powered and advertising, use `bluetoothctl` to find and connect
to it (replace the address with the one shown by scanning):

```text
bluetoothctl
power on
agent NoInputNoOutput
default-agent
scan on
```

After `Rusjoystick CX40 BLE` appears, stop scanning and pair/connect:

```text
scan off
pair XX:XX:XX:XX:XX:XX
trust XX:XX:XX:XX:XX:XX
connect XX:XX:XX:XX:XX:XX
```

Pairing or bonding can depend on the host's policy; if the host reports that
pairing is not needed, try `connect` directly. After connecting, check
`bluetoothctl info XX:XX:XX:XX:XX:XX` for the HID service UUID
(`00001812-0000-1000-8000-00805f9b34fb`), then list input devices:

```shell
sudo evtest
```

Select the Rusjoystick event device from the list. Move the stick and press
buttons while watching events. Expected values from this firmware are:

- Left/right: `EV_ABS` `ABS_X` values -127/+127; centered is 0.
- Up/down: `EV_ABS` `ABS_Y` values -127/+127; centered is 0.
- GPIO0-7: `EV_KEY` button press/release events, corresponding to HID Button
  usages 1-8. Linux's displayed button names depend on its HID-to-input mapping.

If BlueZ can see/connect to the peripheral but `evtest` shows no joystick, the
failure is before input-event delivery, not a bad axis/button value. Check the
Bluetooth service logs and the device's GATT/HID discovery. In particular,
this firmware exposes Protocol Mode as write-only and accepts its writes
without changing modes. This is a HOGP compatibility gap to investigate if a
host discovers the device but does not create an input device.
For report-field meanings and exact GPIO mapping, see
[CX40 BLE HID report](hid-report.md).

## Bluetooth references

- [HID over GATT Profile (HOGP), 1.0](https://www.bluetooth.com/specifications/specs/hid-over-gatt-profile-1-0/)
- [Human Interface Device Service (HIDS), 1.0](https://www.bluetooth.com/specifications/specs/human-interface-device-service-1-0/)
- [Bluetooth Core Specification](https://www.bluetooth.com/specifications/specs/core-specification/) — GAP, ATT, and GATT
- [Bluetooth Assigned Numbers](https://www.bluetooth.com/specifications/assigned-numbers/) — service, characteristic, and appearance values

See also the [report format and descriptor notes](hid-report.md).
