#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::gpio::{Input, Pull};
use embassy_time::Timer;
use embassy_usb::class::hid::{
    Config as HidConfig, HidBootProtocol, HidSubclass, HidWriter, State,
};
use embassy_usb::{Builder, Config as UsbConfig};
use panic_probe as _;
use static_cell::StaticCell;

#[cfg(feature = "analog")]
use embassy_rp::adc::{Adc, Channel, Config as AdcConfig};
#[cfg(feature = "analog")]
use embassy_rp::gpio::Pull as AdcPull;

use embassy_rp::{bind_interrupts, peripherals, usb};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<peripherals::USB>;
    #[cfg(feature = "analog")]
    ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
});

const JOYSTICK_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x04, // Usage (Joystick)
    0xa1, 0x01, // Collection (Application)
    0x09, 0x01, //   Usage (Pointer)
    0xa1, 0x00, //   Collection (Physical)
    0x09, 0x30, //     Usage (X)
    0x09, 0x31, //     Usage (Y)
    0x15, 0x81, //     Logical Minimum (-127)
    0x25, 0x7f, //     Logical Maximum (127)
    0x75, 0x08, //     Report Size (8)
    0x95, 0x02, //     Report Count (2)
    0x81, 0x02, //     Input (Data, Variable, Absolute)
    0xc0, //   End Collection
    0x05, 0x09, // Usage Page (Button)
    0x19, 0x01, //   Usage Minimum (1)
    0x29, 0x08, //   Usage Maximum (8)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Variable, Absolute)
    0xc0, // End Collection
];

static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUFFER: StaticCell<[u8; 64]> = StaticCell::new();
static HID_STATE: StaticCell<State<'static>> = StaticCell::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Embassy initializes the RP2040 peripherals and the async timer driver here.
    let p = embassy_rp::init(Default::default());
    defmt::info!("Starting");
    defmt::info!("Atari CX40 Embassy firmware started");

    let driver = embassy_rp::usb::Driver::new(p.USB, Irqs);
    let mut usb_config = UsbConfig::new(0x1209, 0x0001);
    usb_config.manufacturer = Some("Rusjoystick");
    usb_config.product = Some("Atari CX40");
    usb_config.serial_number = Some("CX40-EMBASSY");

    // These buffers and HID state must outlive the USB device and class.
    let mut builder = Builder::new(
        driver,
        usb_config,
        CONFIG_DESCRIPTOR.init([0; 256]),
        BOS_DESCRIPTOR.init([0; 256]),
        MSOS_DESCRIPTOR.init([0; 256]),
        CONTROL_BUFFER.init([0; 64]),
    );
    let hid_config = HidConfig {
        report_descriptor: JOYSTICK_REPORT_DESCRIPTOR,
        request_handler: None,
        poll_ms: 10,
        max_packet_size: 8,
        hid_subclass: HidSubclass::No,
        hid_boot_protocol: HidBootProtocol::None,
    };
    let mut hid_writer =
        HidWriter::<_, 3>::new(&mut builder, HID_STATE.init(State::new()), hid_config);
    let mut usb = builder.build();

    // CX40 switches connect these pins to ground, so enable pull-ups.
    let buttons = [
        Input::new(p.PIN_0, Pull::Up),
        Input::new(p.PIN_1, Pull::Up),
        Input::new(p.PIN_2, Pull::Up),
        Input::new(p.PIN_3, Pull::Up),
        Input::new(p.PIN_4, Pull::Up),
        Input::new(p.PIN_5, Pull::Up),
        Input::new(p.PIN_6, Pull::Up),
        Input::new(p.PIN_7, Pull::Up),
        Input::new(p.PIN_8, Pull::Up),
        Input::new(p.PIN_9, Pull::Up),
        Input::new(p.PIN_10, Pull::Up),
        Input::new(p.PIN_11, Pull::Up),
    ];

    #[cfg(feature = "analog")]
    let mut adc = Adc::new(p.ADC, Irqs, AdcConfig::default());
    #[cfg(feature = "analog")]
    let mut adc_pin = Channel::new_pin(p.PIN_26, AdcPull::None);

    // USB must be polled continuously. `join` lets the USB stack and joystick
    // sampling loop make progress together on Embassy's single-thread executor.
    let _never_returns = join(usb.run(), async {
        loop {
            #[cfg(feature = "analog")]
            let mut report = get_report(&buttons);
            #[cfg(not(feature = "analog"))]
            let report = get_report(&buttons);

            #[cfg(feature = "analog")]
            {
                let adc_value = adc.read(&mut adc_pin).await.expect("ADC conversion failed");
                defmt::info!("ADC: {}", adc_value);
                report[0] = normalize_adc(adc_value) as u8;
            }

            if let Err(error) = hid_writer.write(&report).await {
                defmt::warn!("Could not send joystick report: {:?}", error);
            }

            // Awaiting a timer yields to the USB future instead of busy-waiting.
            Timer::after_millis(10).await;
        }
    })
    .await;
}

fn get_report(pins: &[Input<'_>; 12]) -> [u8; 3] {
    let mut buttons = 0u8;
    for (index, pin) in pins[..8].iter().enumerate() {
        if pin.is_low() {
            buttons |= 1 << index;
        }
    }

    let pin2_low = pins[2].is_low();
    let pin8_low = pins[8].is_low();
    let pin9_low = pins[9].is_low();
    let pin10_low = pins[10].is_low();
    let pin11_low = pins[11].is_low();
    defmt::info!(
        "pin2={}, pin8={}, pin9={}, pin10={}, pin11={}",
        pin2_low,
        pin8_low,
        pin9_low,
        pin10_low,
        pin11_low
    );

    let x = if pins[8].is_low() {
        defmt::info!("left");
        -127
    } else if pins[9].is_low() {
        defmt::info!("right");
        127
    } else {
        0
    };
    let y = if pins[10].is_low() {
        defmt::info!("up");
        -127
    } else if pins[11].is_low() {
        defmt::info!("down");
        127
    } else {
        0
    };

    [x as u8, y as u8, buttons]
}

#[cfg(feature = "analog")]
fn normalize_adc(value: u16) -> i8 {
    let normalized = ((u32::from(value.min(4095)) * 254) / 4095) as i32 - 127;
    normalized.clamp(-127, 127) as i8
}
