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
//! * Restore operation of the reset pin after the `nrf-recover` wipe:
//!
//!   ```shell
//!   $ cat uicr_reset_pin21.hex | grep -v '//' | probe-rs-cli download --chip nrf52832_xxAA --format hex /dev/stdin
//!   ```
//!
//!   (where the grep is a workaround for probe-rs not accepting comments in ihex files<!-- https://github.com/martinmroz/ihex/issues/16#issuecomment-1374406055 -->).
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
//!
//! ## Device identity
//!
//! By default, `configs/d00.yaml` is used to configure the AS to use, and contains a key
//! shared between the device and its corresponding AS. When using multiple devices, they should
//! all be provisioned with individual identities (i.e. different audience values and individual
//! keys). The file to be used for a particular build can be passed in through the
//! `RS_AS_ASSOCIATION` environment variable.
#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(alloc_error_handler)]

mod coap_gatt;
mod rs_configuration;

mod alloc;
mod blink;
mod coap;
mod devicetime;

use defmt_rtt as _;
use embassy_nrf as _;
use panic_probe as _;

use cortex_m_rt::entry;
use defmt::{error, info, unwrap, warn};
use embassy_executor::{Executor, Spawner};
use nrf_softdevice::ble::{gatt_server, peripheral};
use nrf_softdevice::{raw, Softdevice};

use ace_oscore_helpers::resourceserver::ResourceServer;

static EXECUTOR: static_cell::StaticCell<Executor> = static_cell::StaticCell::new();

/// Maximum number of concurrent BLE connections to manage
///
/// Careful: Must match the executor::task(pool_size) manually (see also [USED_CONNECTIONS])
const MAX_CONNECTIONS: u8 = 4;
/// Number of active BLE connections. This only roughly corresponds to the number of blueworker
/// tasks running (as the only time we can decrement that counter is before blueworker returns).
/// It's important to keep that counter pessimistic w/rt the actually used softdevice connections,
/// for the softdevice would be very unhappy if a connectable advertisement were to be requested
/// while there are no free connections. (Trying to create a task will just abort the connection
/// late, which is OK for being in a racy situation).
///
/// This is used with SeqCst for laziness; a better solution would be
/// <https://github.com/embassy-rs/embassy/issues/1080> anyway.
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

// 700 exceeds some internal limits, but 400 is plenty for our a-bit-over-200 byte tokens.
const MAX_MESSAGE_LEN: usize = 400;

#[nrf_softdevice::gatt_service(uuid = "8df804b7-3300-496d-9dfa-f8fb40a236bc")]
struct CoAPGattService {
    #[characteristic(uuid = "2a58fc3f-3c62-4ecc-8167-d66d4d9410c2", read, write, indicate)]
    message: heapless::Vec<u8, MAX_MESSAGE_LEN>,
}

// The only GATT attribute we're offering is the CoAP endpoint.
#[nrf_softdevice::gatt_server]
struct Server {
    coap: CoAPGattService,
}

type RandomClosure = impl FnMut(&mut [u8]);
type RsMutex = embassy_sync::mutex::Mutex<
    embassy_sync::blocking_mutex::raw::NoopRawMutex,
    ResourceServer<rs_configuration::ApplicationClaims, RandomClosure>,
>;
type Rs = embassy_sync::mutex::Mutex<
    embassy_sync::blocking_mutex::raw::NoopRawMutex,
    ResourceServer<crate::rs_configuration::ApplicationClaims, RandomClosure>,
>;
// runs into an ICE which I couldn't minify yet
// type CoapHandlerFactory = impl Fn(Option<crate::rs_configuration::ApplicationClaims>, &'static Rs) -> coap::CoapHandler + 'static;
pub struct CoapHandlerFactory {
    leds: &'static blink::Leds,
    sd: &'static Softdevice,
}

impl CoapHandlerFactory {
    pub fn build(
        &self,
        claims: Option<&crate::rs_configuration::ApplicationClaims>,
        rs: &'static Rs,
    ) -> coap::CoapHandler {
        coap::create_coap_handler(claims, self.sd, self.leds, rs)
    }
}

/// Single Bluetooth connection handler
///
/// This is spawned from [bluetooth_task] once a connection arrives, and terminates at
/// disconnection.
// Careful: pool_size must match MAX_CONNECTIONS
#[embassy_executor::task(pool_size = 4)]
async fn blueworker(
    server: &'static Server,
    conn: nrf_softdevice::ble::Connection,
    chf: &'static CoapHandlerFactory,
    rs: &'static Rs,
) {
    let mut cg = coap_gatt::Connection::new(chf, rs);

    info!("Running new BLE connection");
    gatt_server::run(&conn, server, |e| match e {
        ServerEvent::Coap(e) => match e {
            CoAPGattServiceEvent::MessageWrite(mut m) => {
                let response = cg.write(&mut *m);

                info!("Setting response {:?}", response);

                unwrap!(server.coap.message_set(&response));
            }
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

/// Main Bluetooth task
///
/// This task is active throughout the device's lifetime, and manages the creation of
/// per-connection tasks.
///
/// It alternates between sending connectable advertisements (when connectable) and unconnectable
/// advertisements (while the pool of connections is exhausted).
#[embassy_executor::task]
async fn bluetooth_task(
    sd: &'static Softdevice,
    server: &'static Server,
    scan_data: &'static [u8],
    spawner: Spawner,
    chf: &'static CoapHandlerFactory,
    rs: &'static Rs,
) {
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

    loop {
        while USED_CONNECTIONS.load(core::sync::atomic::Ordering::SeqCst) >= MAX_CONNECTIONS {
            info!("Connections full; advertising unconnectable");
            // FIXME: Does this need to contain different info?
            let adv = peripheral::NonconnectableAdvertisement::ScannableUndirected {
                adv_data,
                scan_data,
            };
            let nonconn = peripheral::advertise(
                sd,
                adv,
                &peripheral::Config {
                    // We can't easily cancel a running advertisement, so if we're at the connection limit,
                    // we just terminate occasionally to check if there's a free slot now.
                    timeout: Some(500 /* x 10ms = 5s */),
                    ..Default::default()
                },
            )
            .await;
            if let Err(err) = nonconn {
                error!("Failed to advertise: {:?}", err);
            }
        }

        info!("Advertising as connectable until a connection is establsihed");
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data,
            scan_data,
        };
        USED_CONNECTIONS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        let conn = peripheral::advertise_connectable(sd, adv, &peripheral::Config::default()).await;

        let conn = match conn {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to advertise connectable due to {:?}, continuing", e);
                continue;
            }
        };

        if let Err(_) = spawner.spawn(blueworker(server, conn, chf, rs)) {
            // Counting should make sure this never happens, but it's a bit racy.
            warn!("Spawn failure, dropping conn right away");
            USED_CONNECTIONS.fetch_sub(1, core::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// Parts of the peripherals that are needed by the application
struct ChipParts {
    leds: blink::LedPins,
}

/// Initialize chip peripherals, in particular clocks, interrupts and LEDs.
///
/// It returns all (possibly post-processed) peripherals that are needed later.
fn chip_startup() -> ChipParts {
    let mut config: embassy_nrf::config::Config = Default::default();
    // We have these on the board
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    config.lfclk_source = embassy_nrf::config::LfclkSource::ExternalXtal;
    // Differing from default, these stay out of softdevice's hair
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P7;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P6;

    let peripherals = embassy_nrf::init(config);

    use embassy_nrf::gpio::{Level, Output, OutputDrive};
    // See https://infocenter.nordicsemi.com/topic/ug_nrf52832_dk/UG/nrf52_DK/hw_btns_leds.html
    let led1_pin = Output::new(peripherals.P0_17, Level::Low, OutputDrive::Standard);
    let led2_pin = Output::new(peripherals.P0_18, Level::Low, OutputDrive::Standard);
    let led3_pin = Output::new(peripherals.P0_19, Level::Low, OutputDrive::Standard);
    let led4_pin = Output::new(peripherals.P0_20, Level::Low, OutputDrive::Standard);

    // Left in as a template for other interrupt driven components -- but the softdevice wants the
    // temperature interrupt for its own. See also complaints about how the softdevice handles this
    // around coap::Temperature.
    /*
    use embassy_nrf::interrupt::{self, InterruptExt};
    let temp_interrupt = interrupt::take!(TEMP);
    temp_interrupt.set_priority(embassy_nrf::interrupt::Priority::P5);
    let temperature = embassy_nrf::temp::Temp::new(
        peripherals.TEMP,
        temp_interrupt,
        );
    */

    ChipParts {
        leds: blink::LedPins {
            l1: led1_pin,
            l2: led2_pin,
            l3: led3_pin,
            l4: led4_pin,
        },
    }
}

/// Entry function
///
/// This assembles the configuration, starts up the softdevice, and lets both the softdevice and
/// other tasks (LED animations, Bluetooth handlers) run in parallel.
#[entry]
fn main() -> ! {
    info!("Device is starting up...");

    log_to_defmt::setup();

    use ace_oscore_helpers::aead;
    let rs_as_association = include!(concat!(env!("OUT_DIR"), "/rs_as_association.rs"));

    let mut full_name = heapless::String::<20>::new();
    full_name.push_str("CoAP-ACE demo #").unwrap();
    full_name.push_str(&rs_as_association.audience).unwrap();
    let full_name = full_name.into_bytes();
    let full_name_len: u16 = full_name.len().try_into().unwrap();

    #[rustfmt::skip]
    static SCAN_DATA: static_cell::StaticCell<heapless::Vec::<u8, 28>> = static_cell::StaticCell::new();
    let scan_data = SCAN_DATA.init({
        let mut scan_data = heapless::Vec::<u8, 28>::new();
        scan_data
            .push(
                (1 + 5 + rs_as_association.audience.len())
                    .try_into()
                    .unwrap(),
            )
            .unwrap();
        scan_data.push(0x08).unwrap();
        scan_data.extend_from_slice(b"CoAP ").unwrap();
        scan_data
            .extend_from_slice(rs_as_association.audience.as_bytes())
            .unwrap();
        scan_data
            .extend_from_slice(&[
                // AD structure: Incomplete list of 128-bit Service Class UUIDs -- beware the endianness
                // (we could also send a complete one, not-sure/not-care at this stage)
                // Data from coap_gatt_us (but we build this literally right now, so meh)
                0x11, 0x06, 0xbc, 0x36, 0xa2, 0x40, 0xfb, 0xf8, 0xfa, 0x9d, 0x6d, 0x49, 0x00, 0x33,
                0xb7, 0x04, 0xf8, 0x8d,
            ])
            .unwrap();
        scan_data
    });

    let config = nrf_softdevice::Config {
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t {
            // The minimum is not acceptable in amsuess-core-coap-over-gatt-02
            // (and the tokens we post are already in the order of 100 bytes long).
            att_mtu: 256,
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            // It needs a mut ptr, but we don't allow writing in the permissions
            p_value: full_name.as_ptr() as *mut u8,
            current_len: full_name_len,
            max_len: full_name_len,
            write_perm: nrf_softdevice_s132::ble_gap_conn_sec_mode_t {
                _bitfield_1: raw::ble_gap_conn_sec_mode_t::new_bitfield_1(0, 0),
            },
            // No writes allowed or planned, so we can just take the const pointer.
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_USER as u8,
            ),
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

    let ChipParts { leds } = chip_startup();

    let sd = Softdevice::enable(&config);

    let executor = EXECUTOR.init(Executor::new());

    static SERVER: static_cell::StaticCell<Server> = static_cell::StaticCell::new();
    let server = SERVER.init(unwrap!(Server::new(sd)));

    let sd: &'static Softdevice = sd;

    static LEDS: static_cell::StaticCell<blink::Leds> = static_cell::StaticCell::new();
    static RS: static_cell::StaticCell<RsMutex> = static_cell::StaticCell::new();
    static COAP_HANDLER_FACTORY: static_cell::StaticCell<CoapHandlerFactory> =
        static_cell::StaticCell::new();

    let rs = RS.init(embassy_sync::mutex::Mutex::new(
        ResourceServer::new_with_association_and_randomness(rs_as_association, move |buf| {
            // We can afford unwrapping here because the BLE exchanges to get here produce a nice
            // amount of entropy already
            unwrap!(nrf_softdevice::random_bytes(&sd, buf))
        }),
    ));

    executor.run(move |spawner| {
        let leds: &'static blink::Leds = LEDS.init(blink::Leds::new(spawner, leds));
        leds.set_idle(2);

        let coap_handler_factory = COAP_HANDLER_FACTORY.init(CoapHandlerFactory { sd, leds });

        unwrap!(spawner.spawn(softdevice_task(sd)));
        unwrap!(spawner.spawn(bluetooth_task(
            sd,
            server,
            scan_data,
            spawner,
            coap_handler_factory,
            rs
        )));
        info!("Device is ready.");

        // Initializing this only late to ensure that nothing of the "regular" things depends on
        // having a heap; this is only for dcaf / coset as they work with ciborium
        unsafe { alloc::init() };

        // Of course they go *after* alloc init: they're based on heap CoAP messages :-)
        unwrap!(do_oscore_test());
        info!("OSCORE tests passed");
    });
}

/// Run a piece of the libOSCORE plug test suite.
pub fn do_oscore_test() -> Result<(), &'static str> {
    use core::mem::MaybeUninit;

    use coap_message::{MessageOption, MinimalWritableMessage, ReadableMessage};

    use liboscore::raw;

    // From OSCORE plug test, security context A
    let immutables = liboscore::PrimitiveImmutables::derive(
        liboscore::HkdfAlg::from_number(5).unwrap(),
        b"\x9e\x7c\xa9\x22\x23\x78\x63\x40",
        b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10",
        None,
        liboscore::AeadAlg::from_number(24).unwrap(),
        b"\x01",
        b"",
    )
    .unwrap();

    let mut primitive = liboscore::PrimitiveContext::new_from_fresh_material(immutables);

    let mut msg = coap_message::heapmessage::HeapMessage::new();
    let oscopt = b"\x09\x00";
    msg.add_option(9, oscopt);
    msg.set_payload(b"\x5c\x94\xc1\x29\x80\xfd\x93\x68\x4f\x37\x1e\xb2\xf5\x25\xa2\x69\x3b\x47\x4d\x5e\x37\x16\x45\x67\x63\x74\xe6\x8d\x4c\x20\x4a\xdb");

    liboscore_msgbackend::with_heapmessage_as_msg_native(msg, |msg| {
        unsafe {
            let header = liboscore::OscoreOption::parse(oscopt).unwrap();

            let mut unprotected = MaybeUninit::uninit();
            let mut request_id = MaybeUninit::uninit();
            let ret = raw::oscore_unprotect_request(
                msg,
                unprotected.as_mut_ptr(),
                &mut header.into_inner(),
                primitive.as_mut(),
                request_id.as_mut_ptr(),
            );
            assert!(ret == raw::oscore_unprotect_request_result_OSCORE_UNPROTECT_REQUEST_OK);
            let unprotected = unprotected.assume_init();

            let unprotected = liboscore::ProtectedMessage::new(unprotected);
            assert!(unprotected.code() == 1);

            let mut message_options = unprotected.options().fuse();
            let mut ref_options = [(11, "oscore"), (11, "hello"), (11, "1")]
                .into_iter()
                .fuse();
            for (msg_o, ref_o) in (&mut message_options).zip(&mut ref_options) {
                assert!(msg_o.number() == ref_o.0);
                assert!(msg_o.value() == ref_o.1.as_bytes());
            }
            assert!(
                message_options.next().is_none(),
                "Message contained extra options"
            );
            assert!(
                ref_options.next().is_none(),
                "Message didn't contain the reference options"
            );
            assert!(unprotected.payload() == b"");
        };
    });

    // We've taken a *mut of it, let's make sure it lives to the end
    drop(primitive);

    Ok(())
}

#[no_mangle]
unsafe extern "C" fn abort() {
    defmt::panic!("C abort called");
}

#[no_mangle]
unsafe extern "C" fn __assert_func() {
    defmt::panic!("C assert called");
}
