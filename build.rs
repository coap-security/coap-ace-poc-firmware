use std::io::Write;
use std::path::Path;

#[derive(Debug, serde::Deserialize)]
struct Config<'a> {
    issuer: &'a str,
    audience: &'a str,
    as_uri: &'a str,
    key: &'a str,
}

fn main() {
    println!("cargo:rerun-if-env-changed=RS_AS_ASSOCIATION");
    let config_file =
        std::env::var("RS_AS_ASSOCIATION").unwrap_or("demo_as_association.yaml".to_string());
    println!("cargo:rerun-if-changed={}", config_file);
    let yaml = std::fs::read(config_file).expect("Configuration file missing");
    let yaml = String::from_utf8(yaml).expect("Config file is not UTF-8");
    let config: Config =
        serde_yaml::from_str(&yaml).expect("Config file needs to match config structure");
    let key = hex::decode(config.key).expect("Config key should be hex");
    let config_outfile = Path::new(&std::env::var("OUT_DIR").unwrap()).join("rs_as_association.rs");
    let mut config_outfile =
        std::fs::File::create(config_outfile).expect("Config outfile needs to be writable");
    write!(
        config_outfile,
        "ace_oscore_helpers::resourceserver::RsAsSharedData {{ issuer: Some({:?}), audience: {:?}, as_uri: {:?}, key: aead::generic_array::GenericArray::clone_from_slice(&{:?}) }}",
        config.issuer, config.audience, config.as_uri, key,
    )
    .unwrap();

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
}
