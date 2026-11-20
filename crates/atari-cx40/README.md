# Atari CX40

The Atari CX40 is the currently implemented joystick variant in this workspace. It uses an RP2040 USB HID report with eight digital buttons, an X/Y D-pad, and optional ADC input.

## Status

- USB HID implementation: available
- Digital joystick directions and fire button: available
- Optional GPIO/ADC analog input: available behind the `analog` feature
- Bluetooth HID: planned at the workspace level

## DB9 wiring

This implementation uses active-low inputs. Each joystick switch connects a DB9 signal pin to Pico GPIO, and DB9 pin 8 connects to a Pico ground pin.

```text
                    Front view of Atari DB9 plug
                    (pins facing toward you)

                          1  2  3  4  5
                        .- - - - - - - -.
                       /  o  o  o  o  o  \
                       \  o  o  o  o  o  /
                        `- - - - - - - -'
                          6  7  8  9

      1 - Up       -> GPIO10 (D-pad up)
      2 - Down     -> GPIO11 (D-pad down)
      3 - Left     -> GPIO8  (D-pad left)
      4 - Right    -> GPIO9  (D-pad right)
      5 - Pot A    -> not used
      6 - Fire     -> GPIO0
      7 - +5V      -> not connected
      8 - Ground   -> Pico GND
      9 - Pot B    -> not used
```

```text
Raspberry Pi Pico / RP2040

GPIO0  <- DB9 pin 6 / Fire
GPIO8  <- DB9 pin 3 / Left
GPIO9  <- DB9 pin 4 / Right
GPIO10 <- DB9 pin 1 / Up
GPIO11 <- DB9 pin 2 / Down
GPIO13 -> Status LED output

GPIO1 through GPIO7 -> Optional extra buttons
GPIO26 -> Optional ADC0 analog input
GND               -> DB9 pin 8
```

Do not connect DB9 pin 7 to the Pico. A standard digital CX40 joystick does not require the +5 V supply for this circuit.

## Reference schematic

```text
               Atari CX40 switch matrix

    DB9 pin 1 / Up       -----o / o-----+ 
    DB9 pin 2 / Down     -----o / o-----+
    DB9 pin 3 / Left     -----o / o-----+---- Pico GPIO
    DB9 pin 4 / Right    -----o / o-----+
    DB9 pin 6 / Fire     -----o / o-----+
                                           |
                                           +---- DB9 pin 8 / GND / Pico GND

    Pico GPIO10 <- Up
    Pico GPIO11 <- Down
    Pico GPIO8  <- Left
    Pico GPIO9  <- Right
    Pico GPIO0  <- Fire
```

The firmware enables the Pico internal pull-ups. Each DB9 signal reads high when open and low when the switch closes to ground.

## Building and flashing

```shell
cargo build --release -p atari-cx40
cargo run --release -p atari-cx40
```

The build script generates `target/thumbv6m-none-eabi/debug/generated_sdl_mapping.txt` or `target/thumbv6m-none-eabi/release/generated_sdl_mapping.txt` for SDL and Bevy integration.
