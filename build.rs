// SPDX-FileCopyrightText: Copyright 2022 EDF (Électricité de France S.A.)
// SPDX-License-Identifier: BSD-3-Clause
// See README for all details on copyright, authorship and license.
use std::io::Write;
use std::path::Path;

#[derive(Debug, serde::Deserialize)]
struct Config<'a> {
    issuer: &'a str,
    audience: &'a str,
    as_uri: &'a str,
    key: &'a str,

    edhoc_x: &'a str,
    edhoc_y: &'a str,
    edhoc_q: &'a str,

    as_pub_x: Option<&'a str>,
    as_pub_y: Option<&'a str>,
}

fn main() {
    println!("cargo:rerun-if-env-changed=RS_AS_ASSOCIATION");
    let config_file = std::env::var("RS_AS_ASSOCIATION").unwrap_or("configs/d00.yaml".to_string());
    println!("cargo:rerun-if-changed={}", config_file);
    let yaml = std::fs::read(config_file).expect("Configuration file missing");
    let yaml = String::from_utf8(yaml).expect("Config file is not UTF-8");
    let config: Config =
        serde_yaml::from_str(&yaml).expect("Config file needs to match config structure");
    let key = hex::decode(config.key).expect("Config key should be hex");
    let config_outfile = Path::new(&std::env::var("OUT_DIR").unwrap()).join("rs_as_association.rs");
    let mut config_outfile =
        std::fs::File::create(config_outfile).expect("Config outfile needs to be writable");
    // Assigning them static / no_mangle / through a black_box merely serves to make them easier to
    // spot and maybe change them in firmware images.
    write!(
        config_outfile,
        "{{
            #[no_mangle]
            static ISSUER: &str = &{:?};
            #[no_mangle]
            static AUDIENCE: &str = &{:?};
            #[no_mangle]
            static AS_URI: &str = &{:?};
            #[no_mangle]
            static KEY: [u8; 32] = {:?};
            let rs_as = ace_oscore_helpers::resourceserver::RsAsSharedData {{
                issuer: Some(*core::hint::black_box(&ISSUER)),
                audience: *core::hint::black_box(&AUDIENCE),
                as_uri: *core::hint::black_box(&AS_URI),
                key: aead::generic_array::GenericArray::clone_from_slice(core::hint::black_box(&KEY)),
            }};

            let coapcore_config = AdhocCoapcoreConfig {{
                as_symmetric: Some(&KEY),
                edhoc_x: Some({:?}),
                edhoc_y: Some({:?}),
                edhoc_q: Some(&{:?}),
                as_pub: {:?},
            }};

            (rs_as, coapcore_config)
        }}",
        config.issuer, config.audience, config.as_uri, key,
        hex::decode(config.edhoc_x).expect("Config edhoc_x should be hex"),
        hex::decode(config.edhoc_y).expect("Config edhoc_y should be hex"),
        hex::decode(config.edhoc_q).expect("Config edhoc_q should be hex"),
        {
            let x = config.as_pub_x.map(hex::decode).transpose()
                .expect("Config as_pub_x should be hex");
            let y = config.as_pub_y.map(hex::decode).transpose()
                .expect("Config as_pub_y should be hex");
            match (x, y) {
                (Some(x), Some(y)) => Some((x, y)),
                (None, None) => None,
                _ => panic!("Configs as_pub_x and as_pub_y have to be given as a pair"),
            }
        },
    )
    .unwrap();

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
}
