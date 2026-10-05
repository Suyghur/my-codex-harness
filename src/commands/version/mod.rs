use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use std::io::Write;

#[derive(Args, Default)]
pub(crate) struct Version {
    #[arg(long)]
    json: bool,
}

impl Version {
    pub fn run(self, out: &mut dyn Write) -> Result<u8> {
        if self.json {
            #[derive(Serialize)]
            struct BuildInfo {
                version: &'static str,
                os: &'static str,
                arch: &'static str,
                rust_version: &'static str,
            }
            let info = BuildInfo {
                version: env!("CARGO_PKG_VERSION"),
                os: std::env::consts::OS,
                arch: std::env::consts::ARCH,
                rust_version: env!("HARNESS_RUST_VERSION"),
            };
            serde_json::to_writer_pretty(&mut *out, &info).context("写入版本 JSON")?;
            writeln!(out)?;
        } else {
            writeln!(
                out,
                "my-codex-harness {} {}/{}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            )?;
        }
        Ok(0)
    }
}
