#![no_std]
#![no_main]

#[cfg(not(feature = "skip-cyw43-firmware"))]
use cyw43::aligned_bytes;
use cyw43_pio::PioSpi;
use defmt::{info, warn};
use embassy_executor::Spawner;
use bt_hci::cmd::SyncCmd;
use bt_hci::cmd::le::LeRand;
use bt_hci::controller::{Controller as HciController, ControllerCmdSync};
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::{bind_interrupts, dma};
use embassy_time::Timer;
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use static_cell::StaticCell;
use trouble_host::prelude::*;
use {defmt_rtt as _, panic_probe as _};

#[cfg(feature = "analog")]
use embassy_rp::adc::Async as AdcAsync;
#[cfg(feature = "analog")]
use embassy_rp::adc::{Adc, Channel, Config as AdcConfig};
#[cfg(feature = "analog")]
use embassy_rp::gpio::Pull as AdcPull;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>;
    #[cfg(feature = "analog")]
    ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
});

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;

#[gatt_server]
struct Cx40Server {
    hid_service: HidService,
}

#[gatt_service(uuid = "1812")]
struct HidService {
    #[characteristic(uuid = "2a4b", read, value = [
        0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x09, 0x01, 0xa1, 0x00,
        0x09, 0x30, 0x09, 0x31, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08,
        0x95, 0x02, 0x81, 0x02, 0xc0, 0x05, 0x09, 0x19, 0x01, 0x29,
        0x08, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81,
        0x02, 0xc0
    ])]
    report_map: [u8; 42],

    #[descriptor(uuid = "2908", read, value = [0x00, 0x01])]
    #[characteristic(uuid = "2a4d", read, notify, value = [0, 0, 0])]
    report: [u8; 3],

    #[characteristic(uuid = "2a4a", read, value = [0x11, 0x01, 0x00, 0x01])]
    hid_information: [u8; 4],

    #[characteristic(uuid = "2a4e", write)]
    protocol_mode: u8,

    #[characteristic(uuid = "2a4c", write)]
    hid_control_point: u8,
}

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Starting Atari CX40 BLE HID");
    let p = embassy_rp::init(Default::default());

    #[cfg(feature = "skip-cyw43-firmware")]
    let (fw, clm, btfw, nvram) = {
        static EMPTY: &cyw43::Aligned<cyw43::A4, [u8]> = &cyw43::Aligned([0u8; 0]);
        (EMPTY, &[] as &[u8], EMPTY, EMPTY)
    };

    #[cfg(not(feature = "skip-cyw43-firmware"))]
    let (fw, clm, btfw, nvram) = {
        let fw = aligned_bytes!("../cyw43-firmware/43439A0.bin");
        let clm = aligned_bytes!("../cyw43-firmware/43439A0_clm.bin");
        let btfw = aligned_bytes!("../cyw43-firmware/43439A0_btfw.bin");
        let nvram = aligned_bytes!("../cyw43-firmware/nvram_rp2040.bin");
        (fw, clm, btfw, nvram)
    };

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        cyw43_pio::DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        dma::Channel::new(p.DMA_CH0, Irqs),
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (_net_device, bt_device, mut control, runner) =
        cyw43::new_with_bluetooth(state, pwr, spi, fw, btfw, nvram).await;
    spawner.spawn(cyw43_task(runner).unwrap());
    control.init(clm).await;

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

    let controller: ExternalController<_, 10> = ExternalController::new(bt_device);
    #[cfg(feature = "analog")]
    run_ble(controller, &buttons, &mut adc, &mut adc_pin).await;
    #[cfg(not(feature = "analog"))]
    run_ble(controller, &buttons).await;
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
    info!(
        "pin2={}, pin8={}, pin9={}, pin10={}, pin11={}",
        pin2_low, pin8_low, pin9_low, pin10_low, pin11_low
    );

    let x = if pin8_low {
        info!("left");
        -127
    } else if pin9_low {
        info!("right");
        127
    } else {
        0
    };
    let y = if pin10_low {
        info!("up");
        -127
    } else if pin11_low {
        info!("down");
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

#[cfg(feature = "analog")]
async fn run_ble<C>(
    controller: C,
    pins: &[Input<'_>; 12],
    adc: &mut Adc<'_, AdcAsync>,
    adc_pin: &mut Channel<'_>,
) -> !
where
    C: Controller,
{
    let address = Address::random([0xff, 0x8f, 0x40, 0x05, 0xe4, 0xff]);
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let controller: ExternalController<_, 10> = ExternalController::new(controller);
    let mut rng = ChaCha20Rng::from_seed(hci_random_seed(&controller).await);
    let builder = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .set_random_generator_seed(&mut rng);
    builder.set_io_capabilities(IoCapabilities::NoInputNoOutput);
    let stack = builder.build();
    let runner = stack.runner;
    let mut peripheral = stack.peripheral;
    let server = Cx40Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "Rusjoystick CX40 BLE",
        appearance: &appearance::UNKNOWN,
    }))
    .unwrap();

    let _ = select(ble_task(runner), async {
        loop {
            match advertise(&mut peripheral, &server).await {
                Ok(conn) => {
                    let gatt_events = gatt_events_task(&server, &conn);
                    let reports = report_task(&server, &conn, pins, adc, adc_pin);
                    let _ = select(gatt_events, reports).await;
                }
                Err(error) => {
                    let error = defmt::Debug2Format(&error);
                    warn!("BLE advertising error: {:?}", error);
                }
            }
        }
    })
    .await;
    core::future::pending().await
}

#[cfg(not(feature = "analog"))]
async fn run_ble<C>(controller: C, pins: &[Input<'_>; 12]) -> !
where
    C: Controller,
{
    let address = Address::random([0xff, 0x8f, 0x40, 0x05, 0xe4, 0xff]);
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let controller: ExternalController<_, 10> = ExternalController::new(controller);
    let mut rng = ChaCha20Rng::from_seed(hci_random_seed(&controller).await);
    let builder = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .set_random_generator_seed(&mut rng);
    builder.set_io_capabilities(IoCapabilities::NoInputNoOutput);
    let stack = builder.build();
    let runner = stack.runner;
    let mut peripheral = stack.peripheral;
    let server = Cx40Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "Rusjoystick CX40 BLE",
        appearance: &appearance::UNKNOWN,
    }))
    .unwrap();

    let _ = select(ble_task(runner), async {
        loop {
            match advertise(&mut peripheral, &server).await {
                Ok(conn) => {
                    let gatt_events = gatt_events_task(&server, &conn);
                    let reports = report_task(&server, &conn, pins);
                    let _ = select(gatt_events, reports).await;
                }
                Err(error) => {
                    let error = defmt::Debug2Format(&error);
                    warn!("BLE advertising error: {:?}", error);
                }
            }
        }
    })
    .await;
    core::future::pending().await
}

async fn hci_random_seed<C>(controller: &C) -> [u8; 32]
where
    C: HciController + ControllerCmdSync<LeRand>,
{
    let mut seed = [0; 32];
    for chunk in seed.chunks_exact_mut(8) {
        let command = LeRand {};
        let read_events = async {
            let mut buffer = [0; 64];
            loop {
                if controller.read(&mut buffer).await.is_err() {
                    panic!("HCI read failed while collecting the security seed");
                }
            }
        };
        match select(
            select(command.exec(controller), read_events),
            Timer::after_secs(5),
        )
        .await
        {
            Either::First(Either::First(Ok(random))) => chunk.copy_from_slice(&random),
            Either::First(Either::First(Err(_))) => {
                panic!("Bluetooth controller LE Rand command failed")
            }
            Either::First(Either::Second(())) => unreachable!(),
            Either::Second(()) => panic!("Bluetooth controller LE Rand command timed out"),
        }
    }
    seed
}

async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) -> ! {
    loop {
        if let Err(error) = runner.run().await {
            let error = defmt::Debug2Format(&error);
            warn!("BLE runner error: {:?}", error);
        }
    }
}

async fn advertise<'values, 'server, C: Controller>(
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Cx40Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let hid_handles = server.hid_service.handle_range();
    info!(
        "HID GATT handles: {}..{}",
        hid_handles.start, hid_handles.end
    );
    let mut advertiser_data = [0; 31];
    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::ServiceUuids16(&[[0x12, 0x18]]),
            AdStructure::CompleteLocalName(b"Rusjoystick CX40 BLE"),
        ],
        &mut advertiser_data[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &advertiser_data[..len],
                scan_data: &[],
            },
        )
        .await?;
    info!("BLE advertising");
    let connection = advertiser
        .accept()
        .await?
        .with_attribute_server(&server.server)?;
    info!("BLE connection established");
    Ok(connection)
}

async fn gatt_events_task<P: PacketPool>(
    _server: &Cx40Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { .. } => {
                info!("BLE disconnected");
                return Ok(());
            }
            GattConnectionEvent::Gatt { event } => {
                let request = event.payload().incoming();
                info!("BLE ATT request: {:?}", defmt::Debug2Format(&request));
                let reply = match event {
                    GattEvent::Write(write) => write.accept(),
                    GattEvent::Read(read) => read.accept(),
                    _ => event.accept(),
                };
                match reply {
                    Ok(reply) => reply.send().await,
                    Err(_) => warn!("Failed to build GATT response"),
                }
            }
            _ => {}
        }
    }
}

#[cfg(feature = "analog")]
async fn report_task<P: PacketPool>(
    server: &Cx40Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    pins: &[Input<'_>; 12],
    adc: &mut Adc<'_, AdcAsync>,
    adc_pin: &mut Channel<'_>,
) -> Result<(), Error> {
    let report = server.hid_service.report;
    loop {
        let mut data = get_report(pins);
        match adc.read(adc_pin).await {
            Ok(value) => {
                info!("ADC: {}", value);
                data[0] = normalize_adc(value) as u8;
            }
            Err(error) => warn!("ADC conversion failed: {:?}", error),
        }
        if report.notify(conn, &data).await.is_err() {
            return Ok(());
        }
        Timer::after_millis(10).await;
    }
}

#[cfg(not(feature = "analog"))]
async fn report_task<P: PacketPool>(
    server: &Cx40Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    pins: &[Input<'_>; 12],
) -> Result<(), Error> {
    let report = server.hid_service.report;
    loop {
        let data = get_report(pins);
        if report.notify(conn, &data).await.is_err() {
            return Ok(());
        }
        Timer::after_millis(10).await;
    }
}
