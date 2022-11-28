//! CoAP/ACE PoC: Firmware
//! ======================
//!
//! For general introductions, see the project's README.md file. This text is focused at people who
//! want to not just use the crate, but build on it, understand its workings or alter it.
//!
//! Building and running
//! --------------------
//!
//! When developing with this application, it is recommended to use a debugger rather than flashing
//! a monolithic hex file.
//!
//! You'll need:
//!
//! * a nightly Rust compiler
//! * a copy of the [S132 softdevice] (eg. `s132_nrf52_7.3.0_softdevice.hex`)
//!
//!   Note that that software is limited in how it can be distributed; you will find the precise
//!   license terms along with the file.
//!
//! * `probe-run`, `probe-rs` and `nrf-recover` installed: `$ cargo install probe-run probe-rs-cli
//!   nrf-recover`
//!
//!   Other debuggers will do as well, but to display debug output, some tool that supports `defmt`
//!   is needed; `probe-run` does that.
//!
//!   <!-- TBD: probably one of the two probe- might do -->
//!
//! To build and install the firmware:
//!
//! * Flash the softdevice. This is only necessary if that version was not installed previously
//!   already (eg. if the device was just erased).
//!
//!   ```shell
//!   $ probe-rs-cli download --chip nrf52832_xxAA --format hex /tmp/s132_nrf52_7.3.0/s132_nrf52_7.3.0_softdevice.hex
//!   ```
//!
//!   * If you encounter errors in the style of "Error: AP ApAddress { dp: Default, ap: 0 } is not
//!     a memory AP", the target chip may be in a locked state; this depends on the previously
//!     flashed firmware and/or the debugger. Run `nrf-recover` to unlock it; this erases all data
//!     on the target device.
//!
//! * Run
//!
//!   ```shell
//!   $ cargo +nightly run
//!   ```
//!
//!   which downloads all relevant crates, builds them and flashes them, all using `probe-run`.
//!
//!   After a long horizontal line, the program will print any debug output the firmware produces.
//!   To increase verbosity, prefix the command with `DEFMT_LOG=info`.
//!
//! Once the firmware is flashed, it will start whenever the device is powered.
//!
//! [S132 softdevice]: https://www.nordicsemi.com/Products/Development-software/s132/
#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use defmt_rtt as _;
use embassy_nrf as _;
use panic_probe as _;

use cortex_m_rt::entry;
use defmt::{info, warn, error, unwrap};
use embassy_executor::executor::{Executor, Spawner};
use embassy_util::Forever;
use nrf_softdevice::ble::{gatt_server, peripheral};
use nrf_softdevice::{raw, Softdevice};

static EXECUTOR: Forever<Executor> = Forever::new();

// Careful: Must match the executor::task(pool_size) manually
const MAX_CONNECTIONS: u8 = 2;
// Number of active BLE connections. This only roughly corresponds to the number of blueworker
// tasks running (as the only time we can decrement that counter is before blueworker returns).
// It's important to keep that counter pessimistic w/rt the actually used softdevice connections,
// for the softdevice would be very unhappy if a connectable advertisement were to be requested
// while there are no free connections. (Trying to create a task will just abort the connection
// late, which is OK for being in a racy situation).
//
// This is used with SeqCst for laziness; a better solution would be
// https://github.com/embassy-rs/embassy/issues/1080 anyway.
static USED_CONNECTIONS: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Background task in which the Softdevice handless all its tasks.
///
/// Note that many softdevice tasks are handled in interrupts, which must not be disabled; see the
/// [nrf_softdevice] documentation for details.
#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

// None of our current users take these as actual UUIDs...
// let coap_gatt_us: Uuid = "8df804b7-3300-496d-9dfa-f8fb40a236bc".parse().unwrap();
// let coap_gatt_uc: Uuid = "2a58fc3f-3c62-4ecc-8167-d66d4d9410c2".parse().unwrap();

#[nrf_softdevice::gatt_service(uuid = "8df804b7-3300-496d-9dfa-f8fb40a236bc")]
struct CoAPGattService {
    #[characteristic(uuid = "2a58fc3f-3c62-4ecc-8167-d66d4d9410c2", read, write, indicate)]
    // 700 doesn't work, 400 gets at least past startup, but let's start with unproblematic values
    message: heapless::Vec<u8, 200>,
}

// The only GATT attribute we're offering is the CoAP endpoint.
#[nrf_softdevice::gatt_server]
struct Server {
    coap: CoAPGattService,
}

// Careful: must match MAX_CONNECTIONS
#[embassy_executor::task(pool_size=2)]
async fn blueworker(server: &'static Server, conn: nrf_softdevice::ble::Connection) {
    info!("Running new BLE connection");
    gatt_server::run(&conn, server, |e| match e {
        ServerEvent::Coap(e) => match e {
            CoAPGattServiceEvent::MessageWrite(m) => {
                info!("Message: {}, setting empty 2.05 response", &*m);

                unwrap!(server.coap.message_set(unwrap!(heapless::Vec::from_slice(&[0x45]))));
            },
            CoAPGattServiceEvent::MessageCccdWrite { indications: ind } => {
                // Indications are currently specified but not implemented
                info!("Indications: {}", ind);
            }
        },
    })
    .await
    .unwrap_or_else(|e| match e {
        gatt_server::RunError::Disconnected => info!("Peer disconnected"),
        gatt_server::RunError::Raw(e) => error!("Error from gat_server: {:?}", e),
    });

    USED_CONNECTIONS.fetch_sub(1, core::sync::atomic::Ordering::SeqCst);
}

/// This alternates between sending advertisements (when connectable) and processing a single
/// connection
#[embassy_executor::task]
async fn bluetooth_task(sd: &'static Softdevice, server: &'static Server, spawner: Spawner) {
    #[rustfmt::skip]
    let adv_data = &[
        // length, type, value; types see Generic Access Profile
        //
        // We'd only send the minimal data here; once we get someone's attention they'll scan us
        // for the more information below.

        // AD structure 1: Flags (they can't be in the scan data, which is enforced by the
        // softdevice; and without these, blueman-manager won't show the device)
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        // AD structure 2: Appearance: generic thermometer
        0x03, 0x19, 0x00, 0x03,
    ];
    #[rustfmt::skip]
    let scan_data = &[
        // AD structure 3: Shortened local name
        0x09, 0x08, b'C', b'o', b'A', b'P', b'-', b'A', b'C', b'E',
        // AD structure: Incomplete list of 128-bit Service Class UUIDs -- beware the endianness
        // (we could also send a complete one, not-sure/not-care at this stage)
        // Data from coap_gatt_us (but we build this literally right now, so meh)
        0x11, 0x06, 0xbc, 0x36, 0xa2, 0x40, 0xfb, 0xf8, 0xfa, 0x9d, 0x6d, 0x49, 0x00, 0x33, 0xb7, 0x04, 0xf8, 0x8d,
    ];

    loop {
        while USED_CONNECTIONS.load(core::sync::atomic::Ordering::SeqCst) >= MAX_CONNECTIONS {
            info!("Connections full; advertising unconnectable");
            // FIXME: Does this need to contain different info?
            let adv = peripheral::NonconnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
            let nonconn = peripheral::advertise(sd, adv, &peripheral::Config {
                // We can't easily cancel a running advertisement, so if we're at the connection limit,
                // we just terminate occasionally to check if there's a free slot now.
                timeout: Some(500 /* x 10ms = 5s */),
                ..Default::default()
            }).await;
        }

        info!("Advertising as connectable until a connection is establsihed");
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
        USED_CONNECTIONS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        let conn = peripheral::advertise_connectable(sd, adv, &peripheral::Config::default()).await;

        let conn = match conn {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to advertise connectable due to {:?}, continuing", e);
                continue;
            }
        };

        if let Err(_) = spawner.spawn(blueworker(server, conn)) {
            // Counting should make sure this never happens, but it's a bit racy.
            warn!("Spawn failure, dropping conn right away");
            USED_CONNECTIONS.fetch_sub(1, core::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// Entry function
///
/// This assembles the configuration, starts up the softdevice, and lets both the softdevice and a
/// task for Bluetooth event handling run in parallel.
#[entry]
fn main() -> ! {
    info!("Device is starting up...");

    let config = nrf_softdevice::Config {
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t {
            // The minimum is not acceptable in amsuess-core-coap-over-gatt-02
            att_mtu: 256,
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: b"CoAP-ACE demo #9876" as *const u8 as _,
            current_len: 19,
            max_len: 19,
            write_perm: nrf_softdevice_s132::ble_gap_conn_sec_mode_t { _bitfield_1: raw::ble_gap_conn_sec_mode_t::new_bitfield_1(0, 0) },
            // No writes allowed or planned, so we can just take the const pointer.
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(raw::BLE_GATTS_VLOC_USER as u8),
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: MAX_CONNECTIONS,
            event_length: raw::BLE_GAP_EVENT_LENGTH_DEFAULT as _,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: MAX_CONNECTIONS,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: Default::default(),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);

    let executor = EXECUTOR.put(Executor::new());

    // FIXME Does nothing else in embassy or Softdevice use any of these?
    let peripherals = nrf52832_hal::pac::Peripherals::take().unwrap();

    let pins = nrf52832_hal::gpio::p0::Parts::new(peripherals.P0);

    // See https://infocenter.nordicsemi.com/topic/ug_nrf52832_dk/UG/nrf52_DK/hw_btns_leds.html
    let mut led1_pin = pins.p0_17
        .into_push_pull_output(nrf52832_hal::gpio::Level::High);
    let mut led2_pin = pins.p0_18
        .into_push_pull_output(nrf52832_hal::gpio::Level::High);
    let mut led3_pin = pins.p0_19
        .into_push_pull_output(nrf52832_hal::gpio::Level::High);
    let mut led4_pin = pins.p0_20
        .into_push_pull_output(nrf52832_hal::gpio::Level::High);

    use nrf52832_hal::prelude::OutputPin;
    led1_pin.set_low();
    led4_pin.set_low();

    static SERVER: static_cell::StaticCell<Server> = static_cell::StaticCell::new();
    let server = SERVER.init(unwrap!(Server::new(sd)));

    executor.run(move |spawner| {
        unwrap!(spawner.spawn(softdevice_task(sd)));
        unwrap!(spawner.spawn(bluetooth_task(sd, server, spawner)));
        info!("Tasks are active.");
    });
}
