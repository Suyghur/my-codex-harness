pub(crate) mod domain;

use crate::shared::{assets::AssetSet, paths::Locations};
use anyhow::Result;
use clap::{Args, builder::TypedValueParser};
use domain::InstallationPlan;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Args)]
pub(crate) struct Install {
    /// 参数 > CODEX_HOME > ~/.codex
    #[arg(long, value_parser = clap::builder::OsStringValueParser::new().try_map(crate::cli::nonempty_path))]
    codex_home: Option<PathBuf>,
    /// 只预览，不创建文件或目录
    #[arg(long)]
    dry_run: bool,
}

impl Install {
    pub fn run(self, root: Option<&Path>, out: &mut dyn Write) -> Result<u8> {
        let locations = Locations::capture()?;
        let root = locations.root(root)?;
        let assets = AssetSet::load(&root)?;
        let home = locations.destination(self.codex_home.as_deref())?;
        InstallationPlan::prepare(&home, &assets.sources)?.execute(self.dry_run, out)?;
        Ok(0)
    }
}
