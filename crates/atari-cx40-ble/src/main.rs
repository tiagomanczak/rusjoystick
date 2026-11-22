#![no_std]
#![no_main]

use bt_hci::cmd::le::LeRand;
use bt_hci::cmd::SyncCmd;
use bt_hci::controller::{Controller as HciController, ControllerCmdSync};
#[cfg(not(feature = "skip-cyw43-firmware"))]
use cyw43::aligned_bytes;
use cyw43_pio::PioSpi;
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
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

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>;
});

// Bound host memory for one connected central and two simultaneous L2CAP
// channels; these capacities are compile-time resource limits.
const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;

// Advertising the HID UUID only announces the service. This GATT attribute
// table is what a host reads after connecting; see docs/ble-host.md.
#[gatt_server]
struct Cx40Server {
    hid_service: HidService,
}

#[gatt_service(uuid = "1812")]
struct HidService {
    // HID Report Map (2a4b): describes the structure and meaning of each input
    // report byte. Here it declares signed X/Y axes and eight packed buttons;
    // the byte-level layout is documented in docs/hid-report.md.
    #[characteristic(uuid = "2a4b", read, value = [
        0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x09, 0x01, 0xa1, 0x00,
        0x09, 0x30, 0x09, 0x31, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08,
        0x95, 0x02, 0x81, 0x02, 0xc0, 0x05, 0x09, 0x19, 0x01, 0x29,
        0x08, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81,
        0x02, 0xc0
    ])]
    report_map: [u8; 42],

    // HID Report characteristic (2a4d), with a Report Reference (0x2908) for
    // report ID 0 and Input type. The characteristic value is the 3-byte
    // report; Notify lets a subscribed host receive updates without polling.
    #[descriptor(uuid = "2908", read, value = [0x00, 0x01])]
    #[characteristic(uuid = "2a4d", read, notify, value = [0, 0, 0])]
    report: [u8; 3],

    // HID Information (2a4a): bcdHID 1.11, no localized country, flags 0x01.
    // Per HIDS, bit 0 advertises RemoteWake; see docs/ble-host.md for the
    // distinction between this flag and the current suspend/wake handling.
    #[characteristic(uuid = "2a4a", read, value = [0x11, 0x01, 0x00, 0x01])]
    hid_information: [u8; 4],

    // Protocol Mode (0x2a4e) and HID Control Point (0x2a4c) are exposed for
    // HOGP hosts. The application currently accepts writes but does not change
    // its report format or implement suspend/resume behavior.
    #[characteristic(uuid = "2a4e", write)]
    protocol_mode: u8,

    #[characteristic(uuid = "2a4c", write)]
    hid_control_point: u8,
}

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>>,
) -> ! {
    // Keep the CYW43 firmware and its HCI transport running independently of
    // the BLE host's connection/advertising state.
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

    // CYW43 needs vendor firmware, Bluetooth firmware, NVRAM, and regulatory
    // data before its radio can be used. The skip feature is only for builds
    // that provide these assets elsewhere.
    #[cfg(not(feature = "skip-cyw43-firmware"))]
    let (fw, clm, btfw, nvram) = {
        let fw = aligned_bytes!("../cyw43-firmware/43439A0.bin");
        let clm = aligned_bytes!("../cyw43-firmware/43439A0_clm.bin");
        let btfw = aligned_bytes!("../cyw43-firmware/43439A0_btfw.bin");
        let nvram = aligned_bytes!("../cyw43-firmware/nvram_rp2040.bin");
        (fw, clm, btfw, nvram)
    };

    // Connect the RP2040 to the Pico W's CYW43 over PIO SPI; the CYW43 chip
    // acts as the Bluetooth controller while Trouble supplies the BLE host.
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

    // CX40 switches are active-low. Pull-ups make an open/unpressed input read
    // high, so get_report treats a low GPIO level as pressed.
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

    let controller: ExternalController<_, 10> = ExternalController::new(bt_device);
    run_ble(controller, &buttons).await;
}

fn get_report(pins: &[Input<'_>; 12]) -> [u8; 3] {
    // GPIO0-7 map directly to report byte 2, bits 0-7 (LSB first). Active-low
    // inputs become HID button value 1 when pressed.
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
    // These diagnostics run for every report, so they appear 100 times per second.
    info!(
        "pin2={}, pin8={}, pin9={}, pin10={}, pin11={}",
        pin2_low, pin8_low, pin9_low, pin10_low, pin11_low
    );

    // GPIO8/9 encode X and GPIO10/11 encode Y. If opposite switches are both
    // pressed, the first branch wins (left over right; up over down).
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

    // HID fields are signed, but the notification is a byte array: casting
    // preserves the i8 two's-complement encoding (for example -127 becomes 0x81).
    [x as u8, y as u8, buttons]
}

async fn run_ble<C>(controller: C, pins: &[Input<'_>; 12]) -> !
where
    C: Controller + ControllerCmdSync<LeRand>,
{
    // Use this fixed random address for the controller's LE random-address
    // configuration. The LE Rand HCI command supplies entropy for Trouble's
    // security RNG; address configuration and RNG seeding are separate.
    let address = Address::random([0xff, 0x8f, 0x40, 0x05, 0xe4, 0xff]);
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let mut rng = ChaCha20Rng::from_seed(hci_random_seed(&controller).await);
    let builder = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .set_random_generator_seed(&mut rng);
    // The device has no pairing UI. This describes its pairing I/O capability;
    // it does not by itself require pairing or guarantee an encrypted link.
    builder.set_io_capabilities(IoCapabilities::NoInputNoOutput);
    let stack = builder.build();
    let runner = stack.runner;
    let mut peripheral = stack.peripheral;
    // GAP peripheral name is also included in advertising. UNKNOWN avoids
    // claiming a more specific Bluetooth appearance than this code configures.
    let server = Cx40Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "Rusjoystick CX40 BLE",
        appearance: &appearance::UNKNOWN,
    }))
    .unwrap();

    // The runner processes controller/HCI events while the other future handles
    // GAP connections and GATT. Once either per-connection task ends, the
    // connection scope is dropped and the peripheral loop advertises again.
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
        // HCI LE Rand yields 8 bytes per command. Read controller events in
        // parallel so the command-complete event can be consumed; collect four
        // results to fill ChaCha20's 32-byte seed.
        let command = LeRand::new();
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
    // Legacy advertising data is limited to 31 bytes. The service UUID here is
    // only a discovery hint; the full HID service is served over GATT after
    // connection. Flags mark general discoverability and no BR/EDR support.
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
        // Associate this link with the HID GATT table so the central can read
        // characteristics/descriptors and subscribe to the report notifications.
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
            GattConnectionEvent::PairingComplete { security_level, .. } => {
                info!(
                    "BLE pairing complete: {:?}",
                    defmt::Debug2Format(&security_level)
                );
            }
            GattConnectionEvent::PairingFailed(error) => {
                warn!("BLE pairing failed: {:?}", defmt::Debug2Format(&error));
            }
            GattConnectionEvent::Gatt { event } => {
                let request = event.payload().incoming();
                info!("BLE ATT request: {:?}", defmt::Debug2Format(&request));
                // Accept supported ATT reads/writes using Trouble's generated
                // attribute table, then send the corresponding ATT response.
                let reply = match event {
                    GattEvent::Write(write) => write.accept(),
                    GattEvent::Read(read) => read.accept(),
                    _ => event.accept(),
                };
                match reply {
                    Ok(reply) => reply.send().await,
                    Err(error) => {
                        warn!(
                            "Failed to build GATT response: {:?}",
                            defmt::Debug2Format(&error)
                        )
                    }
                }
            }
            _ => {}
        }
    }
}

async fn report_task<P: PacketPool>(
    server: &Cx40Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    pins: &[Input<'_>; 12],
) -> Result<(), Error> {
    let report = server.hid_service.report;
    loop {
        let data = get_report(pins);
        // Attempt a notification every 10 ms (100 Hz), even if inputs did not
        // change. The peer must enable notifications on the Report characteristic.
        if report.notify(conn, &data).await.is_err() {
            return Ok(());
        }
        Timer::after_millis(10).await;
    }
}
