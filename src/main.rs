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
//!   <!-- TBD: probably one of them will do -->
//!
//! To build and install the firmware:
//!
//! * Flash the softdevice. This is only necessary if that version was not installed previously
//!   already.
//!
//!   ```shell
//!   $ probe-rs-cli download --chip nrf52832_xxAA --format hex /tmp/s132_nrf52_7.3.0/s132_nrf52_7.3.0_softdevice.hex
//!   ```
//!
//!   * If you encounter errors in the style of "Error: AP ApAddress { dp: Default, ap: 0 } is not
//!     a memory AP", the target chip may be in a locked state; this depends on the previously
//!     flashed firmware. Run `nrf-recover` to unlock it; this erases all data on the target
//!     device.
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
use defmt::{info, unwrap};
use embassy_executor::executor::Executor;
use embassy_util::Forever;
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, Softdevice};

static EXECUTOR: Forever<Executor> = Forever::new();

/// Background task in which the Softdevice handless all its tasks.
///
/// Note that many softdevice tasks are handled in interrupts, which must not be disabled; see the
/// [nrf_softdevice] documentation for details.
#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

/// We don't have any GATT attributes yet
#[nrf_softdevice::gatt_server]
struct Server {
}

/// This alternates between sending advertisements (when connectable) and ... not yet processing
/// any connections.
#[embassy_executor::task]
async fn bluetooth_task(sd: &'static Softdevice, server: Server) {
    #[rustfmt::skip]
    let adv_data = &[
        // length, type, value; types see Generic Access Profile
        // AD structure 1: Flags
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        // AD structure 2: Appearance: generic thermometer
        0x03, 0x19, 0x00, 0x03,
        // AD structure 3: Shortened local name
        0x09, 0x08, b'C', b'o', b'A', b'P', b'-', b'A', b'C', b'E',
    ];
    #[rustfmt::skip]
    let scan_data = &[
        // Sending appearance all the time
        0x03, 0x1a, 0x03, 0x40,
    ];

    loop {
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
        let conn = unwrap!(peripheral::advertise_connectable(sd, adv, &config).await);

        // We're advertising as connectable, but we don't handle any connections yet
        let _ = conn;
        let _ = &server;
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
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);

    let executor = EXECUTOR.put(Executor::new());

    executor.run(move |spawner| {
        let server = unwrap!(Server::new(sd));
        unwrap!(spawner.spawn(softdevice_task(sd)));
        unwrap!(spawner.spawn(bluetooth_task(sd, server)));
        info!("Tasks are active.");
    });
}
