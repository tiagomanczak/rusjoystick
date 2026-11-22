#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::adc::{Adc, Async as AdcAsync, Channel, Config as AdcConfig};
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::{bind_interrupts, peripherals, usb};
use embassy_time::Timer;
use embassy_usb::class::hid::{
    Config as HidConfig, HidBootProtocol, HidSubclass, HidWriter, State,
};
use embassy_usb::{Builder, Config as UsbConfig};
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<peripherals::USB>;
    ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
});

const JOYSTICK_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x09, 0x01, 0xa1, 0x00, 0x09, 0x30, 0x09, 0x31, 0x15, 0x81,
    0x25, 0x7f, 0x75, 0x08, 0x95, 0x02, 0x81, 0x02, 0xc0, 0x05, 0x09, 0x19, 0x01, 0x29, 0x08, 0x15,
    0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0xc0,
];

// empiric measured values range from 160 to 4095 with the analog cx30
// with the cx40 the pin is open and it is around 60
const DETECTION_SAMPLES: usize = 8;
const CX30_DETECT_THRESHOLD: u16 = 100;

static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUFFER: StaticCell<[u8; 64]> = StaticCell::new();
static HID_STATE: StaticCell<State<'static>> = StaticCell::new();

#[derive(Clone, Copy, defmt::Format)]
enum ControllerMode {
    Cx30,
    Cx40,
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    defmt::info!("Starting Atari CX70 USB HID firmware");

    let driver = embassy_rp::usb::Driver::new(p.USB, Irqs);
    let mut usb_config = UsbConfig::new(0x1209, 0x0001);
    usb_config.manufacturer = Some("Rusjoystick");
    usb_config.product = Some("Atari CX70");
    usb_config.serial_number = Some("CX70-USB");

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

    let inputs = [
        Input::new(p.PIN_18, Pull::Up),
        Input::new(p.PIN_19, Pull::Up),
        Input::new(p.PIN_20, Pull::Up),
        Input::new(p.PIN_21, Pull::Up),
        Input::new(p.PIN_22, Pull::Up),
    ];

    let mut adc = Adc::new(p.ADC, Irqs, AdcConfig::default());
    let mut right_paddle = Channel::new_pin(p.PIN_26, Pull::Down);
    let mut left_paddle = Channel::new_pin(p.PIN_27, Pull::Down);
    let mode = detect_controller(&mut adc, &mut right_paddle, &mut left_paddle).await;
    defmt::info!("Detected controller mode: {}", mode);

    let _never_returns = join(usb.run(), async {
        loop {
            let report = match mode {
                ControllerMode::Cx30 => {
                    get_cx30_report(&inputs, &mut adc, &mut right_paddle, &mut left_paddle).await
                }
                ControllerMode::Cx40 => get_cx40_report(&inputs),
            };

            if let Err(error) = hid_writer.write(&report).await {
                defmt::warn!("Could not send joystick report: {:?}", error);
            }
            Timer::after_millis(10).await;
        }
    })
    .await;
}

async fn detect_controller(
    adc: &mut Adc<'_, AdcAsync>,
    right_paddle: &mut Channel<'_>,
    left_paddle: &mut Channel<'_>,
) -> ControllerMode {
    let mut right_total = 0u32;
    let mut left_total = 0u32;
    for _ in 0..DETECTION_SAMPLES {
        right_total += u32::from(
            adc.read(right_paddle)
                .await
                .expect("Could not read right paddle ADC during controller detection"),
        );
        left_total += u32::from(
            adc.read(left_paddle)
                .await
                .expect("Could not read left paddle ADC during controller detection"),
        );
    }

    let right_average = (right_total / DETECTION_SAMPLES as u32) as u16;
    let left_average = (left_total / DETECTION_SAMPLES as u32) as u16;
    defmt::info!(
        "Controller detection ADC: right={}, left={}",
        right_average,
        left_average
    );

    if right_average > CX30_DETECT_THRESHOLD && left_average > CX30_DETECT_THRESHOLD {
        ControllerMode::Cx30
    } else {
        ControllerMode::Cx40
    }
}

fn get_cx40_report(pins: &[Input<'_>; 5]) -> [u8; 3] {
    let mut buttons = 0;
    if pins[4].is_low() {
        buttons |= 1;
    }

    let x = if pins[2].is_low() {
        -127
    } else if pins[3].is_low() {
        127
    } else {
        0
    };
    let y = if pins[0].is_low() {
        -127
    } else if pins[1].is_low() {
        127
    } else {
        0
    };

    [x as u8, y as u8, buttons]
}

async fn get_cx30_report(
    pins: &[Input<'_>; 5],
    adc: &mut Adc<'_, AdcAsync>,
    right_paddle: &mut Channel<'_>,
    left_paddle: &mut Channel<'_>,
) -> [u8; 3] {
    let right = adc
        .read(right_paddle)
        .await
        .expect("Could not read right paddle ADC");
    let left = adc
        .read(left_paddle)
        .await
        .expect("Could not read left paddle ADC");
    let mut buttons = 0;
    if pins[2].is_low() {
        buttons |= 1;
    }
    if pins[3].is_low() {
        buttons |= 2;
    }

    [
        normalize_adc(left) as u8,
        normalize_adc(right) as u8,
        buttons,
    ]
}

fn normalize_adc(value: u16) -> i8 {
    let normalized = ((u32::from(value.min(4095)) * 254) / 4095) as i32 - 127;
    normalized.clamp(-127, 127) as i8
}
