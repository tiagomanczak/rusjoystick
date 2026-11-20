#![no_std]
#![no_main]

//! Atari CX30 paddle-pair USB HID implementation for the RP2040/Pico.
//!
//! Wiring:
//! - GPIO26 / ADC0: Right paddle position
//! - GPIO27 / ADC1: Left paddle position
//! - GPIO3: Left paddle fire, active low
//! - GPIO4: Right paddle fire, active low
//! - GPIO13: Status LED
//! - DB9 pin 8: Common ground
//! - DB9 pin 7: Paddle 5 V supply; never connect directly to an RP2040 pin

use bsp::entry;
use bsp::hal;
use defmt_rtt as _;
use embedded_hal::digital::*;
use embedded_hal_02::adc::OneShot;
use embedded_hal_02::timer::CountDown;
use fugit::ExtU32;
use hal::{
    adc::{Adc, AdcPin},
    gpio::{DynPinId, FunctionSioInput, Pin, PullUp},
    pac,
};
use panic_probe as _;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::device::joystick::JoystickReport;
use usbd_human_interface_device::prelude::*;

use rp_pico as bsp;

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

    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .strings(&[StringDescriptors::default()
            .manufacturer("Rusjoystick")
            .product("Atari CX30")
            .serial_number("TEST")])
        .unwrap()
        .build();

    let mut led_pin = pins.gpio13.into_push_pull_output();
    let mut left_fire = pins.gpio3.into_pull_up_input().into_dyn_pin();
    let mut right_fire = pins.gpio4.into_pull_up_input().into_dyn_pin();

    let mut adc = Adc::new(peripherals.ADC, &mut peripherals.RESETS);
    let mut right_paddle = AdcPin::new(pins.gpio26.into_floating_input()).unwrap();
    let mut left_paddle = AdcPin::new(pins.gpio27.into_floating_input()).unwrap();

    led_pin.set_low().ok();

    let mut report_timer = timer.count_down();
    report_timer.start(10.millis());

    loop {
        if report_timer.wait().is_ok() {
            let left_adc = adc.read(&mut left_paddle).unwrap_or(0);
            let right_adc = adc.read(&mut right_paddle).unwrap_or(0);
            let report = get_report(&mut left_fire, &mut right_fire, left_adc, right_adc);

            match joy.device().write_report(&report) {
                Err(UsbHidError::WouldBlock) => {}
                Ok(_) => {}
                Err(error) => core::panic!("Failed to write joystick report: {:?}", error),
            }
        }

        usb_dev.poll(&mut [&mut joy]);
    }
}

fn normalize_paddle(value: u16) -> i8 {
    let value = value.min(4095);
    let normalized = ((value as u32 * 254) / 4095) as i16 - 127;
    normalized.clamp(-127, 127) as i8
}

fn get_report(
    left_fire: &mut Pin<DynPinId, FunctionSioInput, PullUp>,
    right_fire: &mut Pin<DynPinId, FunctionSioInput, PullUp>,
    left_adc: u16,
    right_adc: u16,
) -> JoystickReport {
    let mut buttons = 0;
    if left_fire.is_low().unwrap_or(false) {
        buttons |= 1;
    }
    if right_fire.is_low().unwrap_or(false) {
        buttons |= 2;
    }

    JoystickReport {
        buttons,
        x: normalize_paddle(left_adc),
        y: normalize_paddle(right_adc),
    }
}
