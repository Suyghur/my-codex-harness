use crate::shared::fingerprint;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct Bundle {
    pub name: String,
    pub version: String,
    pub schema_version: u32,
    pub fingerprint_algorithm: String,
    pub fingerprint: String,
    pub roles: Vec<Role>,
}
#[derive(Debug, Serialize)]
pub struct Role {
    pub name: String,
    pub file: String,
    pub modes: Vec<String>,
    pub access: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub sha256: String,
    pub prompt_sha256: String,
}
pub struct AssetSet {
    pub bundle: Bundle,
    pub sources: Vec<(String, PathBuf)>,
}
#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    name: String,
    version: String,
    roles: BTreeMap<String, Route>,
}
#[derive(Deserialize)]
struct Route {
    file: String,
    modes: Vec<String>,
    access: String,
}
#[derive(Deserialize)]
struct Definition {
    name: String,
    description: String,
    developer_instructions: String,
    model: Option<String>,
    model_reasoning_effort: Option<String>,
}
fn nonblank(value: &str, field: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{field} 不能为空白");
    Ok(())
}
impl AssetSet {
    pub fn load(root: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(root.join("harness.toml")).context("读取 harness.toml 失败")?;
        let full: toml::Value = toml::from_str(&text).context("解析 harness.toml 失败")?;
        let manifest: Manifest = toml::from_str(&text).context("清单字段无效")?;
        ensure!(manifest.schema_version == 1, "不支持的 schema_version");
        nonblank(&manifest.name, "清单 name")?;
        nonblank(&manifest.version, "清单 version")?;
        ensure!(!manifest.roles.is_empty(), "清单至少登记一个角色");
        let boundary = root
            .join("agents")
            .canonicalize()
            .context("解析 agents 目录失败")?;
        ensure!(boundary.is_dir(), "agents 必须是目录");
        let mut registered = BTreeSet::new();
        let mut registered_files = BTreeSet::new();
        let mut sources = Vec::new();
        let mut roles = Vec::new();
        for (name, route) in manifest.roles {
            nonblank(&name, "角色 name")?;
            let path = Path::new(&route.file);
            ensure!(
                !path.is_absolute()
                    && !path.components().any(|v| matches!(
                        v,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )),
                "角色 {name} 路径必须为无上级逃逸的相对路径"
            );
            ensure!(
                path.extension().is_some_and(|v| v == "toml")
                    && path.file_stem().is_some_and(|v| v == name.as_str()),
                "角色 {name} 文件名必须与角色一致且为 TOML"
            );
            let source = root
                .join(path)
                .canonicalize()
                .with_context(|| format!("角色 {name} 源文件不可访问"))?;
            ensure!(
                source.starts_with(&boundary) && source != boundary && source.is_file(),
                "角色 {name} 源文件必须位于物理 agents 目录内"
            );
            ensure!(
                source.extension().is_some_and(|v| v == "toml")
                    && source.file_stem().is_some_and(|v| v == name.as_str()),
                "角色 {name} 的真实源文件名必须一致且为 TOML"
            );
            ensure!(
                registered.insert(source.clone()),
                "角色 {name} 登记了重复源路径"
            );
            ensure!(!route.modes.is_empty(), "角色 {name} 至少需要一个 mode");
            let mut seen = BTreeSet::new();
            for mode in &route.modes {
                nonblank(mode, &format!("角色 {name} mode"))?;
                ensure!(seen.insert(mode), "角色 {name} mode 重复");
            }
            ensure!(
                [
                    "read-only",
                    "read-only-with-scratch",
                    "read-only-with-report",
                    "planning-artifacts",
                    "task-scoped"
                ]
                .contains(&route.access.as_str()),
                "角色 {name} access 无效"
            );
            let bytes = fs::read(&source).with_context(|| format!("读取角色 {name} 失败"))?;
            let definition: Definition = toml::from_str(
                std::str::from_utf8(&bytes).with_context(|| format!("角色 {name} 不是 UTF-8"))?,
            )
            .with_context(|| format!("角色 {name} TOML 字段无效"))?;
            ensure!(definition.name == name, "角色 {name} TOML name 不匹配");
            nonblank(&definition.description, &format!("角色 {name} description"))?;
            nonblank(
                &definition.developer_instructions,
                &format!("角色 {name} developer_instructions"),
            )?;
            for (field, value) in [
                ("model", &definition.model),
                ("model_reasoning_effort", &definition.model_reasoning_effort),
            ] {
                if let Some(value) = value {
                    nonblank(value, &format!("角色 {name} {field}"))?;
                }
            }
            registered_files.insert(root.join(path));
            roles.push(Role {
                name: name.clone(),
                file: route.file,
                modes: route.modes,
                access: route.access,
                model: definition.model,
                reasoning_effort: definition.model_reasoning_effort,
                sha256: fingerprint::digest(&bytes),
                prompt_sha256: fingerprint::digest(definition.developer_instructions.as_bytes()),
            });
            sources.push((name, source));
        }
        for entry in fs::read_dir(root.join("agents"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|v| v == "toml") {
                ensure!(
                    registered_files.contains(&path),
                    "未登记角色文件 {}",
                    path.display()
                );
            }
        }
        let fingerprint = fingerprint::digest(&fingerprint::encode(&full, &roles)?);
        Ok(Self {
            bundle: Bundle {
                name: manifest.name,
                version: manifest.version,
                schema_version: manifest.schema_version,
                fingerprint_algorithm: fingerprint::ALGORITHM.into(),
                fingerprint,
                roles,
            },
            sources,
        })
    }

    pub fn validate_scenarios(&self, root: &Path) -> Result<()> {
        #[derive(Deserialize)]
        struct Catalog {
            schema_version: u32,
            status: String,
            scenarios: Vec<Scenario>,
        }
        #[derive(Deserialize)]
        struct Scenario {
            id: String,
            role: String,
            mode: String,
            setup: String,
            task: String,
            criteria: Vec<String>,
        }
        let catalog: Catalog = serde_json::from_slice(
            &fs::read(root.join("evals/scenarios.json")).context("读取场景目录失败")?,
        )
        .context("解析场景目录失败")?;
        ensure!(
            catalog.schema_version == 1 && catalog.status == "seed_not_run",
            "场景 schema_version 或 status 无效"
        );
        ensure!(!catalog.scenarios.is_empty(), "场景目录不能为空");
        let mut ids = BTreeSet::new();
        for scenario in catalog.scenarios {
            nonblank(&scenario.id, "场景 id")?;
            ensure!(
                ids.insert(scenario.id.clone()),
                "场景 {} id 重复",
                scenario.id
            );
            let Some(role) = self.bundle.roles.iter().find(|v| v.name == scenario.role) else {
                bail!("场景 {} 角色无效", scenario.id)
            };
            ensure!(
                role.modes.contains(&scenario.mode),
                "场景 {} 模式无效",
                scenario.id
            );
            nonblank(&scenario.setup, &format!("场景 {} setup", scenario.id))?;
            nonblank(&scenario.task, &format!("场景 {} task", scenario.id))?;
            ensure!(
                !scenario.criteria.is_empty(),
                "场景 {} criteria 不能为空",
                scenario.id
            );
            for criterion in scenario.criteria {
                nonblank(&criterion, &format!("场景 {} criteria", scenario.id))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("agents")).unwrap();
        fs::write(dir.path().join("harness.toml"), "schema_version=1\nname='fixture'\nversion='1'\n[roles.one]\nfile='agents/one.toml'\nmodes=['custom']\naccess='task-scoped'\n").unwrap();
        fs::write(
            dir.path().join("agents/one.toml"),
            "name='one'\ndescription='描述'\ndeveloper_instructions='提示<&>'\n",
        )
        .unwrap();
        dir
    }
    #[test]
    fn nullable_model_and_path_independent_digest() {
        let a = fixture();
        let b = fixture();
        let original = AssetSet::load(a.path()).unwrap();
        assert_eq!(
            original.bundle.fingerprint,
            AssetSet::load(b.path()).unwrap().bundle.fingerprint
        );
        let role = serde_json::to_value(&original.bundle.roles[0]).unwrap();
        assert_eq!(role["model"], serde_json::Value::Null);
        assert_eq!(role["reasoning_effort"], serde_json::Value::Null);
        fs::write(
            a.path().join("agents/one.toml"),
            "name='one'\ndescription='描述'\ndeveloper_instructions='提示<&>'\n\n",
        )
        .unwrap();
        let changed = AssetSet::load(a.path()).unwrap();
        assert_ne!(original.bundle.fingerprint, changed.bundle.fingerprint);
        assert_ne!(
            original.bundle.roles[0].sha256,
            changed.bundle.roles[0].sha256
        );
        assert_eq!(
            original.bundle.roles[0].prompt_sha256,
            changed.bundle.roles[0].prompt_sha256
        );
    }
    #[test]
    fn invalid_assets_are_rejected() {
        for change in [
            "schema_version=2",
            "modes=['custom','custom']",
            "access='write-all'",
        ] {
            let dir = fixture();
            let path = dir.path().join("harness.toml");
            let text = fs::read_to_string(&path).unwrap();
            let before = if change.starts_with("schema") {
                "schema_version=1"
            } else if change.starts_with("modes") {
                "modes=['custom']"
            } else {
                "access='task-scoped'"
            };
            fs::write(path, text.replace(before, change)).unwrap();
            assert!(AssetSet::load(dir.path()).is_err());
        }
        for suffix in ["model=42\n", "model=' '\n", "model_reasoning_effort=''\n"] {
            let dir = fixture();
            let path = dir.path().join("agents/one.toml");
            let text = fs::read_to_string(&path).unwrap();
            fs::write(path, text + suffix).unwrap();
            assert!(AssetSet::load(dir.path()).is_err());
        }
        let dir = fixture();
        fs::write(dir.path().join("agents/extra.toml"), "").unwrap();
        assert!(AssetSet::load(dir.path()).is_err());
    }
    #[test]
    fn missing_blank_and_mismatched_fields_are_rejected() {
        for (before, after) in [
            ("name='fixture'", "name=' '"),
            ("version='1'", "version=''"),
            ("modes=['custom']", "modes=[]"),
            ("modes=['custom']", "modes=[' ']"),
            ("file='agents/one.toml'", "file='../agents/one.toml'"),
            ("file='agents/one.toml'", "file='/agents/one.toml'"),
            ("file='agents/one.toml'", "file='agents/missing.toml'"),
            ("schema_version=1\n", ""),
        ] {
            let dir = fixture();
            let path = dir.path().join("harness.toml");
            fs::write(
                &path,
                fs::read_to_string(&path).unwrap().replace(before, after),
            )
            .unwrap();
            assert!(AssetSet::load(dir.path()).is_err(), "accepted {after:?}");
        }
        for (before, after) in [
            ("name='one'", "name='other'"),
            ("description='描述'", "description=' '"),
            (
                "developer_instructions='提示<&>'",
                "developer_instructions=''",
            ),
            ("description='描述'\n", ""),
        ] {
            let dir = fixture();
            let path = dir.path().join("agents/one.toml");
            fs::write(
                &path,
                fs::read_to_string(&path).unwrap().replace(before, after),
            )
            .unwrap();
            assert!(AssetSet::load(dir.path()).is_err(), "accepted {after:?}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn two_roles_cannot_alias_the_same_source() {
        let dir = fixture();
        let path = dir.path().join("harness.toml");
        let mut manifest = fs::read_to_string(&path).unwrap();
        manifest.push_str(
            "[roles.two]\nfile='agents/two.toml'\nmodes=['custom']\naccess='task-scoped'\n",
        );
        fs::write(path, manifest).unwrap();
        std::os::unix::fs::symlink("one.toml", dir.path().join("agents/two.toml")).unwrap();
        assert!(AssetSet::load(dir.path()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn source_symlink_escape_is_rejected() {
        let dir = fixture();
        let external = tempfile::NamedTempFile::new().unwrap();
        let path = dir.path().join("agents/one.toml");
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(external.path(), path).unwrap();
        assert!(AssetSet::load(dir.path()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn resolved_source_extension_and_name_are_checked() {
        let dir = fixture();
        let path = dir.path().join("agents/one.toml");
        let content = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let renamed = dir.path().join("agents/one.txt");
        fs::write(&renamed, content).unwrap();
        std::os::unix::fs::symlink(&renamed, &path).unwrap();
        assert!(AssetSet::load(dir.path()).is_err());
    }
    #[test]
    fn metadata_participates_and_table_order_does_not() {
        let dir = fixture();
        let path = dir.path().join("harness.toml");
        let base = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{base}[metadata]\nz=1\na='x'\n")).unwrap();
        let initial = AssetSet::load(dir.path()).unwrap().bundle.fingerprint;
        fs::write(&path, format!("{base}[metadata]\na='x'\nz=1\n")).unwrap();
        assert_eq!(
            initial,
            AssetSet::load(dir.path()).unwrap().bundle.fingerprint
        );
        fs::write(&path, format!("{base}[metadata]\na='y'\nz=1\n")).unwrap();
        assert_ne!(
            initial,
            AssetSet::load(dir.path()).unwrap().bundle.fingerprint
        );
    }
    #[test]
    fn scenarios_validate_structure_only() {
        let dir = fixture();
        fs::create_dir(dir.path().join("evals")).unwrap();
        let asset = AssetSet::load(dir.path()).unwrap();
        let valid = serde_json::json!({"schema_version":1,"status":"seed_not_run","scenarios":[{"id":"case","role":"one","mode":"custom","setup":"fixture","task":"do","criteria":["passes"]}]});
        let path = dir.path().join("evals/scenarios.json");
        fs::write(&path, valid.to_string()).unwrap();
        asset.validate_scenarios(dir.path()).unwrap();
        for (field, value) in [
            ("role", serde_json::json!("unknown")),
            ("mode", serde_json::json!("unknown")),
            ("criteria", serde_json::json!([" "])),
        ] {
            let mut invalid = valid.clone();
            invalid["scenarios"][0][field] = value;
            fs::write(&path, invalid.to_string()).unwrap();
            assert!(asset.validate_scenarios(dir.path()).is_err());
        }
        let mut duplicate = valid.clone();
        duplicate["scenarios"]
            .as_array_mut()
            .unwrap()
            .push(valid["scenarios"][0].clone());
        fs::write(path, duplicate.to_string()).unwrap();
        assert!(asset.validate_scenarios(dir.path()).is_err());
    }
}
