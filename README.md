# Rusjoystick

Originally copied from the examples of the crate usbd-human-interface-device


- joystick - two axis joystick with eight buttons



## Installation of development dependencies

```shell
rustup target install thumbv6m-none-eabi
cargo install flip-link
# This is the suggested default 'runner'
cargo install probe-rs-cli
# If you want to use elf2uf2-rs instead of probe-run, instead do...
cargo install elf2uf2-rs
```

## Running

```shell
cargo run --release
```

## Atari joystick wiring

This project uses active-low inputs: each joystick switch connects its DB9 signal pin to GPIO, and DB9 pin 8 connects to a Pico ground pin.

```text
                    Front view of Atari DB9 plug
                    (pins facing toward you)

                          1  2  3  4  5
                        .- - - - - - - -. 
                       /  o  o  o  o  o  \
                       \  o  o  o  o  o  /
                        `- - - - - - - -'
                          6  7  8  9

      1 - Up       -> GPIO10 (D-pad up)      6 - Fire -> GPIO0
      2 - Down     -> GPIO11 (D-pad down)    7 - +5V  -> not connected
      3 - Left     -> GPIO8  (D-pad left)     8 - GND  -> Pico GND
      4 - Right    -> GPIO9  (D-pad right)    9 - Pot B -> not connected
      5 - Pot A    -> not connected
```

```text
    Raspberry Pi Pico / RP2040

    GPIO00  <- Atari DB9 pin 6 / Fire
    GPIO01  -  Button 2  (optional)
    GPIO02  -  Button 3  (optional)
    GPIO03  -  Button 4  (optional)
    GPIO04  -  Button 5  (optional)
    GPIO05  -  Button 6  (optional)
    GPIO06  -  Button 7  (optional)
    GPIO07  -  Button 8  (optional)

    GPIO08  <- Atari DB9 pin 3 / Left
    GPIO09  <- Atari DB9 pin 4 / Right
    GPIO10  <- Atari DB9 pin 1 / Up
    GPIO11  <- Atari DB9 pin 2 / Down

    GPIO26  -  ADC0 analog X input (when "analog" feature is enabled)

    GND     -- Atari DB9 pin 8 / Ground
```

> Use DB9 pin 8 as the common ground. Do not connect DB9 pin 7 (+5 V) to the Pico; it is not required for a standard Atari joystick.

### Reference schematic

```text
                         Atari joystick
                      switches to common

                         DB9 pin 1  Up
                            |
                            [ ]
                            |\
                            | \  normally open switch
DB9 pin 8/GND ------------+  \
                            |   \ to DB9 pin 1
                            |    
                         DB9 pin 2  Down
                            |
                            [ ]
                            |\
                            | 
DB9 pin 8/GND ------------+  
                            |
                         DB9 pin 3  Left
                            |
                            [ ]
                            |\
                            | 
DB9 pin 8/GND ------------+  
                            |
                         DB9 pin 4  Right
                            |
                            [ ]
                            |\
                            | 
DB9 pin 8/GND ------------+  
                            |
                         DB9 pin 6  Fire
                            |
                            [ ]
                            |\
                            | 
DB9 pin 8/GND ------------+  

DB9 pin 8/GND ---------- Raspberry Pi Pico GND
Each direction/fire pin -> matching Pico GPIO with internal pull-up enabled
```

A cleaner electrical view is:

```text
        Atari joystick switch matrix

                 5 V (pin 7) - unused

        pin 1 (Up) ----o / o---- GPIO10
        pin 2 (Down) --o / o---- GPIO11
        pin 3 (Left) --o / o---- GPIO8
        pin 4 (Right)-o / o---- GPIO9
        pin 6 (Fire) --o / o---- GPIO0
                          |
                          +-------- DB9 pin 8 / GND / Pico GND
```

The firmware enables the Pico's internal pull-ups. Therefore, each DB9 signal reads high while open and low while that joystick switch closes to ground.

The remaining GPIO 1-7 inputs are available for extra buttons and are read as active-low digital inputs.

## SDL controller mapping

The build script now emits `generated_sdl_mapping.txt` alongside the compiled binary in the profile output directory:

- `target/debug/generated_sdl_mapping.txt`
- `target/release/generated_sdl_mapping.txt`

This file contains the SDL controller mapping string for the joystick and can be loaded into a Bevy game by setting `SDL_GAMECONTROLLERCONFIG` before SDL initializes.

For example, in a Bevy application:

```rust
fn main() {
    let mapping = std::fs::read_to_string("./generated_sdl_mapping.txt")
        .expect("Failed to read generated SDL controller mapping");
    std::env::set_var("SDL_GAMECONTROLLERCONFIG", mapping);
    // build your Bevy App here
}
```

When launching the built executable directly, SDL will pick up the mapping from the generated file located next to the binary.
