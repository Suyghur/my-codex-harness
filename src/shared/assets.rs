use crate::shared::fingerprint;
use crate::shared::role_name::validate_role_name;
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
    pub agents_boundary: PathBuf,
    pub manifest_source: PathBuf,
}
struct Manifest {
    schema_version: u32,
    name: String,
    version: String,
    roles: BTreeMap<String, Route>,
}
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
impl Manifest {
    // toml::Value only offers an owned Deserializer. Borrow the parsed tree
    // here so unknown metadata need not be cloned or parsed a second time.
    fn from_value(value: &toml::Value) -> Result<Self> {
        fn string(table: &toml::Table, key: &str) -> Result<String> {
            table
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .with_context(|| format!("{key} 必须为字符串"))
        }
        let table = value.as_table().context("清单必须为表")?;
        let schema_version = table
            .get("schema_version")
            .and_then(toml::Value::as_integer)
            .and_then(|value| u32::try_from(value).ok())
            .context("schema_version 必须为非负 u32 整数")?;
        let entries = table
            .get("roles")
            .and_then(toml::Value::as_table)
            .context("roles 必须为表")?;
        let mut roles = BTreeMap::new();
        for (name, value) in entries {
            let route = (|| -> Result<Route> {
                let table = value.as_table().context("角色路由必须为表")?;
                let modes = table
                    .get("modes")
                    .and_then(toml::Value::as_array)
                    .context("modes 必须为数组")?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .context("mode 必须为字符串")
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Route {
                    file: string(table, "file")?,
                    modes,
                    access: string(table, "access")?,
                })
            })()
            .with_context(|| format!("角色 {name} 路由字段无效"))?;
            roles.insert(name.clone(), route);
        }
        Ok(Self {
            schema_version,
            name: string(table, "name")?,
            version: string(table, "version")?,
            roles,
        })
    }
}

fn ensure_regular_file(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| format!("读取 {label} 文件类型失败"))?;
    ensure!(metadata.is_file(), "{label} 必须为普通文件");
    Ok(())
}

fn nonblank(value: &str, field: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{field} 不能为空白");
    Ok(())
}
impl AssetSet {
    pub fn load(root: &Path) -> Result<Self> {
        let manifest_source = root
            .join("harness.toml")
            .canonicalize()
            .context("解析 harness.toml 路径失败")?;
        ensure_regular_file(&manifest_source, "harness.toml")?;
        let text = fs::read_to_string(&manifest_source).context("读取 harness.toml 失败")?;
        let full: toml::Value = toml::from_str(&text).context("解析 harness.toml 失败")?;
        let manifest = Manifest::from_value(&full).context("清单字段无效")?;
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
            validate_role_name(&name)?;
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
            registered_files.insert((name.clone(), source.clone()));
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
                let actual = path
                    .canonicalize()
                    .with_context(|| format!("解析直属角色文件 {} 失败", path.display()))?;
                if actual.is_dir() {
                    continue;
                }
                let name = path.file_stem().and_then(|v| v.to_str());
                ensure!(
                    name.is_some_and(|name| registered_files.contains(&(name.to_owned(), actual))),
                    "未登记角色文件 {}",
                    path.display()
                );
            }
        }
        let fingerprint = fingerprint::bundle_digest(&full, &roles)?;
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
            agents_boundary: boundary,
            manifest_source,
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
        let path = root.join("evals/scenarios.json");
        ensure_regular_file(&path, "scenarios.json")?;
        let catalog: Catalog =
            serde_json::from_slice(&fs::read(&path).context("读取场景目录失败")?)
                .context("解析场景目录失败")?;
        ensure!(
            catalog.schema_version == 1 && catalog.status == "seed_not_run",
            "场景 schema_version 或 status 无效"
        );
        ensure!(!catalog.scenarios.is_empty(), "场景目录不能为空");
        let role_modes: BTreeMap<&str, BTreeSet<&str>> = self
            .bundle
            .roles
            .iter()
            .map(|role| {
                (
                    role.name.as_str(),
                    role.modes.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        let mut ids = BTreeSet::new();
        for scenario in &catalog.scenarios {
            nonblank(&scenario.id, "场景 id")?;
            ensure!(
                ids.insert(scenario.id.as_str()),
                "场景 {} id 重复",
                scenario.id
            );
            let Some(modes) = role_modes.get(scenario.role.as_str()) else {
                bail!("场景 {} 角色无效", scenario.id)
            };
            ensure!(
                modes.contains(scenario.mode.as_str()),
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
            for criterion in &scenario.criteria {
                nonblank(criterion, &format!("场景 {} criteria", scenario.id))?;
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
    fn parsed_manifest_projection_preserves_field_type_validation() {
        for (before, after) in [
            ("schema_version=1", "schema_version=-1"),
            ("schema_version=1", "schema_version=4294967296"),
            ("schema_version=1", "schema_version='1'"),
            ("name='fixture'", "name=1"),
            ("version='1'", "version=true"),
            ("[roles.one]", "[roles]\none='invalid'\n[other]"),
            ("file='agents/one.toml'", "file=1"),
            ("modes=['custom']", "modes='custom'"),
            ("modes=['custom']", "modes=[1]"),
            ("access='task-scoped'", "access=false"),
            ("access='task-scoped'", ""),
        ] {
            let dir = fixture();
            let path = dir.path().join("harness.toml");
            let text = fs::read_to_string(&path).unwrap();
            fs::write(path, text.replace(before, after)).unwrap();
            let error = AssetSet::load(dir.path()).err().unwrap();
            assert!(
                error.to_string().contains("清单字段无效"),
                "accepted or misclassified {after:?}: {error:#}"
            );
        }
    }
    #[test]
    fn toml_directories_are_not_unregistered_role_files() {
        let dir = fixture();
        let group = dir.path().join("agents/group.toml");
        fs::create_dir(&group).unwrap();
        fs::rename(dir.path().join("agents/one.toml"), group.join("one.toml")).unwrap();
        let manifest = dir.path().join("harness.toml");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            manifest,
            text.replace("agents/one.toml", "agents/group.toml/one.toml"),
        )
        .unwrap();
        AssetSet::load(dir.path()).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("group.toml", dir.path().join("agents/alias.toml")).unwrap();
            AssetSet::load(dir.path()).unwrap();
        }
        let extra = dir.path().join("agents/extra.toml");
        fs::write(&extra, "").unwrap();
        assert!(
            AssetSet::load(dir.path())
                .err()
                .unwrap()
                .to_string()
                .contains("未登记角色文件")
        );
        #[cfg(unix)]
        {
            fs::remove_file(&extra).unwrap();
            std::os::unix::fs::symlink("missing.toml", &extra).unwrap();
            assert!(
                AssetSet::load(dir.path())
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("解析直属角色文件")
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn nonregular_manifest_and_scenario_inputs_are_rejected() {
        let dir = fixture();
        let asset = AssetSet::load(dir.path()).unwrap();
        fs::create_dir(dir.path().join("evals")).unwrap();
        for (relative, label) in [
            ("harness.toml", "harness.toml"),
            ("evals/scenarios.json", "scenarios.json"),
        ] {
            let path = dir.path().join(relative);
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            fs::create_dir(&path).unwrap();
            ensure_regular_file(&path, label).unwrap_err();
            fs::remove_dir(&path).unwrap();
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
            let error = if label == "harness.toml" {
                AssetSet::load(dir.path()).err().unwrap()
            } else {
                asset.validate_scenarios(dir.path()).unwrap_err()
            };
            assert!(error.to_string().contains("必须为普通文件"), "{error:#}");
        }
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
    #[test]
    fn asset_role_names_use_the_portable_ascii_contract() {
        for name in [".", "..", "one\\two", "角色", "réader"] {
            let dir = fixture();
            let manifest = format!(
                "schema_version=1\nname='fixture'\nversion='1'\n[roles.'{name}']\nfile='agents/{name}.toml'\nmodes=['custom']\naccess='task-scoped'\n"
            );
            fs::write(dir.path().join("harness.toml"), manifest).unwrap();
            let error = AssetSet::load(dir.path()).err().unwrap();
            assert!(error.to_string().contains("非法角色名称"), "{error:#}");
        }
        let dir = fixture();
        let name = "Reader_2-X";
        fs::rename(
            dir.path().join("agents/one.toml"),
            dir.path().join(format!("agents/{name}.toml")),
        )
        .unwrap();
        fs::write(
            dir.path().join(format!("agents/{name}.toml")),
            format!("name='{name}'\ndescription='valid'\ndeveloper_instructions='valid'\n"),
        )
        .unwrap();
        fs::write(
            dir.path().join("harness.toml"),
            format!("schema_version=1\nname='fixture'\nversion='1'\n[roles.{name}]\nfile='agents/{name}.toml'\nmodes=['custom']\naccess='task-scoped'\n"),
        )
        .unwrap();
        assert_eq!(
            AssetSet::load(dir.path()).unwrap().bundle.roles[0].name,
            name
        );
    }
    #[cfg(unix)]
    #[test]
    fn registered_alias_matches_the_direct_source_but_not_other_role_names() {
        let dir = fixture();
        fs::create_dir(dir.path().join("links")).unwrap();
        std::os::unix::fs::symlink("../agents/one.toml", dir.path().join("links/one.toml"))
            .unwrap();
        let manifest = dir.path().join("harness.toml");
        fs::write(
            &manifest,
            fs::read_to_string(&manifest)
                .unwrap()
                .replace("file='agents/one.toml'", "file='links/one.toml'"),
        )
        .unwrap();
        let assets = AssetSet::load(dir.path()).unwrap();
        assert_eq!(
            assets.agents_boundary,
            dir.path().join("agents").canonicalize().unwrap()
        );
        assert_eq!(assets.manifest_source, manifest.canonicalize().unwrap());
        std::os::unix::fs::symlink("one.toml", dir.path().join("agents/extra.toml")).unwrap();
        assert!(
            AssetSet::load(dir.path())
                .err()
                .unwrap()
                .to_string()
                .contains("未登记角色文件")
        );
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
