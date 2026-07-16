// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

#![forbid(unsafe_code)]

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use spidola_test_headend::{Config, Headend};

#[derive(Debug)]
struct Arguments {
    bind: String,
    assets: PathBuf,
    public_base: Option<String>,
    stall_seconds: u64,
    drop_seconds: u64,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spidola-test-headend: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments = parse_arguments(env::args().skip(1))?;
    let config = Config {
        assets_dir: arguments.assets,
        public_base: arguments.public_base.unwrap_or_default(),
        stall_duration: Duration::from_secs(arguments.stall_seconds),
        drop_duration: Duration::from_secs(arguments.drop_seconds),
    };
    config
        .validate_assets()
        .map_err(|error| error.to_string())?;
    let headend = Headend::bind(&arguments.bind, config).map_err(|error| error.to_string())?;
    let address = headend.local_addr().map_err(|error| error.to_string())?;
    println!("Spidola test headend listening on {address}");
    let (_shutdown_tx, shutdown_rx) = mpsc::channel();
    headend
        .serve(&shutdown_rx)
        .map_err(|error| error.to_string())
}

fn parse_arguments(arguments: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut parsed = Arguments {
        bind: "127.0.0.1:8090".to_owned(),
        assets: PathBuf::from("target/test-headend-assets"),
        public_base: None,
        stall_seconds: 300,
        drop_seconds: 20,
    };
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--bind" => parsed.bind = next_value(&mut arguments, "--bind")?,
            "--assets" => parsed.assets = PathBuf::from(next_value(&mut arguments, "--assets")?),
            "--public-base" => {
                parsed.public_base = Some(next_value(&mut arguments, "--public-base")?);
            }
            "--stall-seconds" => {
                parsed.stall_seconds =
                    parse_seconds(&next_value(&mut arguments, "--stall-seconds")?)?;
            }
            "--drop-seconds" => {
                parsed.drop_seconds =
                    parse_seconds(&next_value(&mut arguments, "--drop-seconds")?)?;
            }
            "--help" | "-h" => return Err(usage().to_owned()),
            unknown => return Err(format!("unknown argument {unknown}\n{}", usage())),
        }
    }
    Ok(parsed)
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_seconds(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("invalid seconds value {value:?}: {error}"))
}

fn usage() -> &'static str {
    "Usage: spidola-test-headend [--bind HOST:PORT] [--assets DIRECTORY] \
     [--public-base URL] [--stall-seconds N] [--drop-seconds N]"
}
