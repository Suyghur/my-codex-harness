use crate::{
    commands::install::domain::check_links,
    shared::{
        assets::{AssetSet, Bundle},
        paths::Locations,
    },
};
use anyhow::{Context, Result};
use clap::{Args, builder::TypedValueParser};
use serde::Serialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Args)]
pub(crate) struct Check {
    #[arg(long)]
    json: bool,
    /// 校验场景结构和角色路由，不运行模型
    #[arg(long)]
    scenarios: bool,
    #[arg(long, value_parser = clap::builder::OsStringValueParser::new().try_map(crate::cli::nonempty_path))]
    codex_home: Option<PathBuf>,
}

impl Check {
    pub fn run(self, root: Option<&Path>, out: &mut dyn Write, err: &mut dyn Write) -> Result<u8> {
        let report = inspect(root, self.codex_home.as_deref(), self.scenarios);
        if self.json {
            serde_json::to_writer_pretty(&mut *out, &report).context("写入 JSON")?;
            writeln!(out)?;
        } else if report.errors.is_empty() {
            if let Some(bundle) = &report.bundle {
                writeln!(
                    out,
                    "资产检查通过；指纹（{}）：{}",
                    bundle.fingerprint_algorithm, bundle.fingerprint
                )?;
            }
            writeln!(err, "此结果不代表模型可用、权限已隔离或角色行为评估通过。")?;
        } else {
            for message in &report.errors {
                writeln!(err, "{message}")?;
            }
        }
        Ok(u8::from(!report.errors.is_empty()))
    }
}

#[derive(Serialize)]
struct CheckReport {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle: Option<Bundle>,
    errors: Vec<String>,
    behavior_evaluation: &'static str,
}

fn inspect(root: Option<&Path>, home: Option<&Path>, scenarios: bool) -> CheckReport {
    let loaded = (|| -> Result<_> {
        let locations = Locations::capture()?;
        let root = locations.root(root)?;
        let assets = AssetSet::load(&root)?;
        Ok((locations, root, assets))
    })();
    let (bundle, errors) = match loaded {
        Err(e) => (None, vec![format!("{e:#}")]),
        Ok((locations, root, assets)) => {
            let mut errors = match home.map(|path| locations.expand(path)).transpose() {
                Ok(Some(home)) => check_links(&home, &assets.sources),
                Ok(None) => Vec::new(),
                Err(e) => vec![format!("{e:#}")],
            };
            if scenarios && let Err(e) = assets.validate_scenarios(&root) {
                errors.push(format!("{e:#}"));
            }
            (Some(assets.bundle), errors)
        }
    };
    CheckReport {
        status: if errors.is_empty() {
            "passed"
        } else {
            "failed"
        },
        bundle,
        errors,
        behavior_evaluation: "not_run",
    }
}
