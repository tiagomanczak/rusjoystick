# Atari CX30

The Atari CX30 is a pair of rotary paddle controllers. This crate implements paddle decoding and exposes the pair as a USB HID joystick.

## Status

- Hardware reference: documented
- Potentiometer ADC support: available
- Paddle fire switches: available
- USB HID implementation: available
- Bluetooth HID implementation: planned at the workspace level

## CX30 paddle hardware

One CX30 controller pair connects through a shared DE-9 connector. The pair contains two rotary potentiometers and two fire switches.

The normal Atari paddle convention uses:

| DE-9 pin | Paddle signal | Suggested Pico connection | Electrical behavior |
| --- | --- | --- | --- |
| 3 | Left paddle fire | GPIO digital input | Active-low switch to ground |
| 4 | Right paddle fire | GPIO digital input | Active-low switch to ground |
| 5 | Right paddle position | ADC input | Potentiometer voltage |
| 7 | Paddle supply | Paddle 5 V supply | 5 V only through the original wiring; do not feed the RP2040 pin directly |
| 8 | Ground | Pico GND | Common ground |
| 9 | Left paddle position | ADC input | Potentiometer voltage |

Pin assignments can vary by adapter or console generation. Verify an unknown controller with a multimeter and its original schematic before energizing a reproduction circuit.

## Front DB9 view

```text
                         DE-9 paddle connector

                          1  2  3  4  5
                        .- - - - - - - -.
                       /  o  o  o  o  o  \
                       \  o  o  o  o  o  /
                        `- - - - - - - -'
                          6  7  8  9

      1 - Console-specific / unused
      2 - Console-specific / unused
      3 - Left paddle fire
      4 - Right paddle fire
      5 - Right paddle position
      6 - Console-specific / unused
      7 - +5 V supply
      8 - Ground
      9 - Left paddle position
```

## Reference schematic

```text
                         Atari CX30 paddle pair

                         5 V (DB9 pin 7)
                              |
                      +-------+-------+
                      |               |
                   10 kohm          10 kohm
                 potentiometer    potentiometer
                      |               |
                  center tap       center tap
                      |               |
       Left paddle <- +---------------+ -> GPIO/ADC
       Right paddle <-+---------------+ -> GPIO/ADC
                      |
                      +------------- DB9 pin 8 / GND / Pico GND

Left paddle fire  -> DB9 pin 3 -> active-low Pico GPIO
Right paddle fire -> DB9 pin 4 -> active-low Pico GPIO
Left paddle axis  -> DB9 pin 9 -> RP2040 ADC input
Right paddle axis -> DB9 pin 5 -> RP2040 ADC input
```

The exact potentiometer resistance and supply arrangement can differ between paddle hardware and adapter cables. Use resistor or voltage-divider protection where needed; RP2040 ADC pins must not be driven outside their valid voltage range.

## Implementation

The firmware reads both paddle potentiometers through RP2040 ADC channels and both fire switches as active-low GPIO inputs. ADC values are normalized to the HID joystick axis range and sent in a report every 10 ms.

```text
Raspberry Pi Pico / RP2040

GPIO26 / ADC0 <- DB9 pin 5 / Right paddle position
GPIO27 / ADC1 <- DB9 pin 9 / Left paddle position
GPIO3         <- DB9 pin 3 / Left paddle fire
GPIO4         <- DB9 pin 4 / Right paddle fire
GPIO13        -> Status LED
GND           <- DB9 pin 8
```

The HID report uses button 0 for the left fire and button 1 for the right fire. It uses `leftx` for the left paddle and `lefty` for the right paddle.

## Building

```shell
cargo build --release -p atari-cx30
cargo run --release -p atari-cx30
```
