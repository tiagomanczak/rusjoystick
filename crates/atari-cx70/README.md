# Atari CX70

The CX70 firmware is an RP2040 USB HID implementation that supports the Atari CX30 paddle pair and CX40 joystick on the same hardware. It detects the controller at startup by sampling both ADC inputs: two ADC values above the detection threshold select CX30 mode; otherwise it selects CX40 mode. The selection is latched until the firmware restarts.

## DB9 wiring

The shared GPIO wiring uses active-low inputs. Connect DB9 pin 8 to Pico ground. In CX30 mode, GPIO20 and GPIO21 are the left and right paddle fire switches; in CX40 mode, they are left and right. GPIO22 is the CX40 fire button. GPIO26/ADC0 and GPIO27/ADC1 read the right and left paddle positions.

```text
                    Front view of Atari DB9 plug
                    (pins facing toward you)

                          1  2  3  4  5
                        .- - - - - - - -.
                       /  o  o  o  o  o  \
                       \  o  o  o  o  o  /
                        `- - - - - - - -'
                          6  7  8  9
          CX40      CX30
      1 - Up       Console    -> GPIO18 (D-pad up)
      2 - Down     Console    -> GPIO19 (D-pad down)
      3 - Left     Left       -> GPIO20  (D-pad left)
      4 - Right    Right      -> GPIO21  (D-pad right)
      5 - Pot A    Right      -> ADC0
      6 - Fire     Console    -> GPIO22
      7 - +5V      +5         -> Controller supply (not Pico GPIO/VCC)
      8 - Ground   Ground     -> Pico GND
      9 - Pot B    Left       -> ADC1

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

The firmware enables pull-ups on the digital inputs. Each DB9 switch signal reads high when open and low when closed to ground. ADC inputs have pull-downs so disconnected paddle inputs tend toward zero during CX40 detection. CX30 is selected only when both averaged ADC readings are above 32; for reliable detection, keep both paddle axes away from their ground-side endpoints during startup. The mode cannot be switched without restarting the firmware.

Never connect a 5 V signal directly to an RP2040 GPIO or ADC pin. Ensure paddle voltages are limited to the RP2040's 0–3.3 V input range. Do not connect DB9 pin 7 directly to Pico VCC.

## Build and flash

```shell
cargo check -p atari-cx70
cargo build --release -p atari-cx70
cargo run --release -p atari-cx70
```

The USB HID report contains X, Y, and eight button bits. CX40 directions are mapped to the X/Y axes and GPIO22 maps to button 0. In CX30 mode, the left and right paddle positions map to X and Y, while GPIO20/GPIO21 map to buttons 0 and 1.

USB identification uses VID/PID `1209:0001` and the product name `Atari CX70`. The build script generates `generated_sdl_mapping.txt` in the Cargo target profile directory.

## Testing on Linux

After flashing and connecting the Pico over USB, check enumeration and input events:

```shell
lsusb -d 1209:0001
sudo evtest
```

Select the Atari CX70 event device and verify that axes and buttons change in the corresponding controller mode.
