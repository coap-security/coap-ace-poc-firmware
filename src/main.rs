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

/// Background task hand
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
        0x03, 0x1a, 0x03, 0x40,
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
