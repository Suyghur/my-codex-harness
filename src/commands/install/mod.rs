pub(crate) mod domain;

use crate::shared::{assets::AssetSet, paths::Locations};
use anyhow::Result;
use clap::{Args, builder::TypedValueParser};
use domain::{InstallationEvent, InstallationPlan};
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
        let manifests = [
            assets.manifest_source.clone(),
            root.canonicalize()?.join("harness.toml"),
        ];
        let plan =
            InstallationPlan::prepare(&home, &assets.sources, &assets.agents_boundary, &manifests)?;
        let mut events = Vec::new();
        let execution = plan.execute(self.dry_run, &mut events);
        let output = render_events(&events, out);
        // A filesystem failure takes precedence, including when reporting its
        // recovery events also fails. Output never interrupts filesystem work.
        execution?;
        output?;
        Ok(0)
    }
}

fn render_events(events: &[InstallationEvent], out: &mut dyn Write) -> Result<()> {
    for event in events {
        match event {
            InstallationEvent::Installed(name) => writeln!(out, "已安装：{name}")?,
            InstallationEvent::Preview {
                destination,
                source,
            } => {
                writeln!(
                    out,
                    "预览：{} -> {}",
                    destination.display(),
                    source.display()
                )?;
            }
            InstallationEvent::BackupCreated(path) => writeln!(out, "备份：{}", path.display())?,
        }
    }
    Ok(())
}
