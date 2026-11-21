# Rusjoystick

Rusjoystick is a Rust workspace for converting classic retro controllers into modern USB and Bluetooth input devices using an RP2040, such as the Raspberry Pi Pico.

Originally based on examples from `usbd-human-interface-device`, the repository is being organized as independent controller implementations so each controller can have its own hardware assumptions, HID report, and transport support.

## Controller variants

| Variant | Controller | Implementation status | Documentation |
| --- | --- | --- | --- |
| `atari-cx40` | Atari CX40 joystick | USB implementation available | [CX40 README](crates/atari-cx40/README.md) |
| `atari-cx40-embassy` | Atari CX40 joystick | Independent Embassy USB implementation | [CX40 Embassy README](crates/atari-cx40-embassy/README.md) |
| `atari-cx40-ble` | Atari CX40 joystick | Embassy Trouble BLE HID implementation for Pico W | [CX40 BLE README](crates/atari-cx40-ble/README.md) |
| `atari-cx30` | Atari CX30 paddle pair | USB implementation available | [CX30 README](crates/atari-cx30/README.md) |
| `atari-cx70` | Atari CX30 paddle pair or CX40 joystick | Embassy USB HID implementation with startup ADC detection | [CX70 README](crates/atari-cx70/README.md) |

Additional retro controllers will be added as separate workspace members. Keep controller-specific wiring and protocol code inside each member rather than adding all variants to a shared firmware binary.

## Transport goals

The workspace is intended to support two transport paths:

- **USB HID:** direct connection to Windows, Linux, macOS, SDL, Bevy, emulators, and other HID-aware hosts.
- **Bluetooth HID:** planned support for RP2040-compatible Bluetooth modules or a future supported Bluetooth controller stack.

USB and Bluetooth support should remain separate from controller decoding. A CX40 or CX30 implementation should convert physical inputs into a common or controller-specific HID report, while transport crates decide whether that report is sent over USB or Bluetooth.

## Development dependencies

```shell
rustup target install thumbv6m-none-eabi
cargo install flip-link
cargo install probe-rs-cli
cargo install elf2uf2-rs
```

The repository `.cargo/config.toml` is configured for the RP2040 target and `probe-rs` as the default runner.

## Building

Build one controller variant:

```shell
cargo build -p atari-cx40
cargo build -p atari-cx40-embassy
cargo build -p atari-cx30
cargo build -p atari-cx70
```

Check every workspace member:

```shell
cargo check --workspace
```

Run or flash the CX40 implementation:

```shell
cargo run --release -p atari-cx40
cargo run --release -p atari-cx40-embassy
cargo run --release -p atari-cx70
```

The CX30 and CX70 firmware packages are available for their respective paddle and shared-controller hardware.

## Workspace layout

```text
rusjoystick/
├── crates/
│   ├── atari-cx40/  # USB joystick implementation
│   ├── atari-cx40-embassy/  # Independent Embassy USB joystick implementation
│   ├── atari-cx30/  # Paddle implementation
│   └── atari-cx70/  # CX30/CX40 shared-hardware USB implementation
├── memory.x         # RP2040 linker memory layout
└── Cargo.toml       # Workspace configuration
```

## SDL controller mapping

Each implemented build script may emit `generated_sdl_mapping.txt` into the Cargo profile output directory:

- `target/thumbv6m-none-eabi/debug/generated_sdl_mapping.txt`
- `target/thumbv6m-none-eabi/release/generated_sdl_mapping.txt`

Set `SDL_GAMECONTROLLERCONFIG` before initializing SDL or Bevy:

```rust
fn main() {
    let mapping = std::fs::read_to_string("./generated_sdl_mapping.txt")
        .expect("Failed to read generated SDL controller mapping");

    std::env::set_var("SDL_GAMECONTROLLERCONFIG", mapping);

    // Start Bevy or SDL after setting the environment variable.
}
```
