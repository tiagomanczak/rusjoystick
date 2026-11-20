#![no_std]
#![no_main]

//! Joystick example for RP2040/Pico
//!
//! This example demonstrates a USB HID joystick implementation with digital button inputs
//! and optional analog stick support via ADC.
//!
//! Features:
//! - 8 digital buttons (GPIO 0-7)
//! - D-PAD simulation via 4 digital switches (GPIO 8-11)
//! - Optional analog X-axis from ADC0 (GPIO26) when "analog" feature is enabled
//!
//! Wiring:
//! - GPIO0-7: Buttons connected to GND (active low, pull-up enabled)
//! - GPIO8-11: D-PAD buttons connected to GND
//! - GPIO26: ADC input for analog stick (0-3.3V)
//! - GPIO13: LED output

use bsp::entry;
use bsp::hal;
use cortex_m::prelude::*;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::*;
use fugit::ExtU32;
use hal::{
    gpio::{DynPinId, FunctionSioInput, Pin, PullUp},
    pac,
};
#[cfg(feature = "analog")]
use hal::{
    adc::Adc,
    adc::AdcPin,
};


use panic_probe as _;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::device::joystick::JoystickReport;
use usbd_human_interface_device::prelude::*;

use rp_pico as bsp;

/// Main entry point for the joystick firmware
/// 
/// Initializes USB, GPIO, timer, and ADC (if enabled).
/// Continuously polls buttons and sends HID reports every 10ms.
#[entry]
fn main() -> ! {
    let mut peripherals = pac::Peripherals::take().unwrap();

    let mut watchdog = hal::Watchdog::new(peripherals.WATCHDOG);
    let clocks = hal::clocks::init_clocks_and_plls(
        bsp::XOSC_CRYSTAL_FREQ,
        peripherals.XOSC,
        peripherals.CLOCKS,
        peripherals.PLL_SYS,
        peripherals.PLL_USB,
        &mut peripherals.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let timer = hal::Timer::new(peripherals.TIMER, &mut peripherals.RESETS, &clocks);

    let sio = hal::Sio::new(peripherals.SIO);
    let pins = hal::gpio::Pins::new(
        peripherals.IO_BANK0,
        peripherals.PADS_BANK0,
        sio.gpio_bank0,
        &mut peripherals.RESETS,
    );

    info!("Starting");

    //USB
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        peripherals.USBCTRL_REGS,
        peripherals.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut peripherals.RESETS,
    ));

    let mut joy = UsbHidClassBuilder::new()
        .add_device(usbd_human_interface_device::device::joystick::JoystickConfig::default())
        .build(&usb_bus);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .strings(&[StringDescriptors::default()
            .manufacturer("usbd-human-interface-device")
            .product("Rusty joystick")
            .serial_number("TEST")])
        .unwrap()
        .build();

    //GPIO pins
    let mut led_pin = pins.gpio13.into_push_pull_output();

    // Enable adc (only when analog feature is enabled)
    #[cfg(feature = "analog")]
    let mut adc = Adc::new(peripherals.ADC, &mut peripherals.RESETS);
    #[cfg(feature = "analog")]
    let mut adc_pin_0 = AdcPin::new(pins.gpio26.into_floating_input()).unwrap();

    let mut input_pins: [Pin<DynPinId, FunctionSioInput, PullUp>; 12] = [
        pins.gpio0.into_pull_up_input().into_dyn_pin(),
        pins.gpio1.into_pull_up_input().into_dyn_pin(),
        pins.gpio2.into_pull_up_input().into_dyn_pin(),
        pins.gpio3.into_pull_up_input().into_dyn_pin(),
        pins.gpio4.into_pull_up_input().into_dyn_pin(),
        pins.gpio5.into_pull_up_input().into_dyn_pin(),
        pins.gpio6.into_pull_up_input().into_dyn_pin(),
        pins.gpio7.into_pull_up_input().into_dyn_pin(),
        pins.gpio8.into_pull_up_input().into_dyn_pin(),
        pins.gpio9.into_pull_up_input().into_dyn_pin(),
        pins.gpio10.into_pull_up_input().into_dyn_pin(),
        pins.gpio11.into_pull_up_input().into_dyn_pin(),
    ];
    led_pin.set_low().ok();

    let mut input_count_down = timer.count_down();
    input_count_down.start(10.millis());

    loop {
        // Poll every 10ms
        if input_count_down.wait().is_ok() {
            #[cfg(feature = "analog")]
            let mut report = get_report(&mut input_pins);
            #[cfg(not(feature = "analog"))]
            let report: JoystickReport = get_report(&mut input_pins);

            // Apply analog stick input if feature is enabled
            #[cfg(feature = "analog")]
            {
                // Read ADC value from GPIO26 (ADC0)
                let adc_value: u16 = adc.read(&mut adc_pin_0).unwrap();
                info!("ADC: {}", adc_value);
                
                // Normalize ADC reading (0-4095) to joystick range (-127 to 127)
                // Divide by 16 to fit 12-bit ADC into signed byte range
                let normalized = ((adc_value as i16) / 16 - 127).clamp(-128, 127) as i8;
                report.x = normalized;
            }


            match joy.device().write_report(&report) {
                Err(UsbHidError::WouldBlock) => {}
                Ok(_) => {}
                Err(e) => {
                    core::panic!("Failed to write joystick report: {:?}", e)
                }
            }
        }

        if usb_dev.poll(&mut [&mut joy]) {}
    }
}

/// Generates a joystick report from the current button states
/// 
/// Reads the state of all GPIO pins and constructs a HID joystick report:
/// - Buttons: Digital inputs on GPIO 0-7
/// - D-PAD: Mapped to X/Y axes using GPIO 8-11
///   - GPIO8: Left (X = -127)
///   - GPIO9: Right (X = 127)
///   - GPIO10: Up (Y = -127)
///   - GPIO11: Down (Y = 127)
/// 
/// Note: Analog X-axis value should be set by caller if analog feature is enabled.
fn get_report(
    pins: &mut [Pin<DynPinId, FunctionSioInput, PullUp>; 12]
) -> JoystickReport {

    // Read out 8 buttons first
    let mut buttons = 0;
    for (idx, pin) in pins[..8].iter_mut().enumerate() {
        if pin.is_low().unwrap() {
            buttons |= 1 << idx;        }
    }

    // We're using digital switches in a D-PAD style configuration
    //    10
    //  8    9
    //    11
    // These are mapped to the limits of an axis

    let pin2_low = pins[2].is_low().unwrap(); // Test a button pin
    let pin8_low = pins[8].is_low().unwrap();
    let pin9_low = pins[9].is_low().unwrap();
    let pin10_low = pins[10].is_low().unwrap();
    let pin11_low = pins[11].is_low().unwrap();
    info!(
        "pin2={}, pin8={}, pin9={}, pin10={}, pin11={}",
        pin2_low, pin8_low, pin9_low, pin10_low, pin11_low
    );

    let x = if pin8_low {
        info!("right");
        127 // right
    } else if pin9_low {
        info!("left");
        -127 // left
    } else {
        0 // center
    };

    let y = if pin10_low {
        info!("up");
        -127 // up
    } else if pin11_low {
        info!("down");
        127 // down
    } else {
        0 // center
    };

    JoystickReport { buttons, x, y }
}
