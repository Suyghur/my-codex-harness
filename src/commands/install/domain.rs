//! Read-only planning followed by all backups, then individual atomic replacements.
use crate::shared::{
    paths::physical_directory,
    role_name::{portable_name_key, validate_role_name},
};
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug)]
enum Action {
    Skip,
    Link,
    BackupFile,
    BackupLink(PathBuf),
}
#[derive(Debug)]
struct Target {
    name: String,
    source: PathBuf,
    destination: PathBuf,
    action: Action,
}

/// Completed actions and preview entries, formatted only by the command layer.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallationEvent {
    Installed(String),
    Preview {
        destination: PathBuf,
        source: PathBuf,
    },
    BackupCreated(PathBuf),
}
#[derive(Debug)]
pub struct InstallationPlan {
    home: PathBuf,
    agents: PathBuf,
    targets: Vec<Target>,
}

fn link(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, destination)
            .with_context(|| format!("无法创建链接 {}", destination.display()))
    }
    #[cfg(not(unix))]
    {
        let _ = (source, destination);
        bail!("安装仅支持 macOS/Linux")
    }
}

impl InstallationPlan {
    pub fn prepare(
        home: &Path,
        sources: &[(String, PathBuf)],
        source_directory: &Path,
        manifests: &[PathBuf],
    ) -> Result<Self> {
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            bail!("安装仅支持 macOS/Linux");
        }
        let home = if home.is_absolute() {
            home.to_path_buf()
        } else {
            std::env::current_dir()?.join(home)
        };
        let home = physical_directory(&home)?;
        let agents = physical_directory(&home.join("agents"))?;
        let source_directory = fs::canonicalize(source_directory)?;
        let mut targets = Vec::with_capacity(sources.len());
        let mut names = std::collections::HashSet::new();
        for (name, source) in sources {
            validate_role_name(name)?;
            if !names.insert(portable_name_key(name)) {
                bail!("角色目标名称发生大小写碰撞：{name}");
            }
            let source = fs::canonicalize(source)
                .with_context(|| format!("无法解析角色源文件 {}", source.display()))?;
            if !fs::metadata(&source)?.is_file() {
                bail!("角色源不是普通文件：{}", source.display());
            }
            let destination = agents.join(format!("{name}.toml"));
            // Protect the manifest entry as well as its resolved source. A
            // target symlink elsewhere can still be backed up and replaced.
            let entry = fs::symlink_metadata(&destination)
                .ok()
                .filter(|m| !m.file_type().is_symlink())
                .and_then(|_| fs::canonicalize(&destination).ok())
                .unwrap_or_else(|| destination.clone());
            let action = match fs::symlink_metadata(&destination) {
                Ok(metadata) => {
                    if !metadata.is_file() && !metadata.file_type().is_symlink() {
                        bail!("角色目标不是普通文件或符号链接：{}", destination.display());
                    }
                    if fs::canonicalize(&destination).is_ok_and(|resolved| resolved == source) {
                        Action::Skip
                    } else if metadata.file_type().is_symlink() {
                        Action::BackupLink(fs::read_link(&destination)?)
                    } else {
                        Action::BackupFile
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => Action::Link,
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("无法检查角色目标 {}", destination.display()));
                }
            };
            if !matches!(action, Action::Skip)
                && manifests
                    .iter()
                    .any(|manifest| *manifest == destination || *manifest == entry)
            {
                bail!("安装目标与资产清单重叠：{}", destination.display());
            }
            targets.push(Target {
                name: name.clone(),
                source,
                destination,
                action,
            });
        }
        if targets
            .iter()
            .any(|t| matches!(t.action, Action::BackupFile | Action::BackupLink(_)))
        {
            let backup_root = home.join("backups/my-codex-harness");
            if physical_directory(&backup_root)?.starts_with(&source_directory) {
                bail!("备份目录位于源资产目录内");
            }
        }
        if targets.iter().any(|t| !matches!(t.action, Action::Skip))
            && agents.starts_with(&source_directory)
        {
            bail!("安装写入目录位于源资产目录内：{}", agents.display());
        }
        Ok(Self {
            home,
            agents,
            targets,
        })
    }

    pub fn execute(self, dry_run: bool, events: &mut Vec<InstallationEvent>) -> Result<()> {
        self.execute_with(dry_run, events, |_, _| Ok(()))
    }

    fn execute_with(
        self,
        dry_run: bool,
        events: &mut Vec<InstallationEvent>,
        mut before: impl FnMut(Stage, usize) -> Result<()>,
    ) -> Result<()> {
        if dry_run {
            for target in &self.targets {
                if matches!(target.action, Action::Skip) {
                    events.push(InstallationEvent::Installed(target.name.clone()));
                } else {
                    events.push(InstallationEvent::Preview {
                        destination: target.destination.clone(),
                        source: target.source.clone(),
                    });
                }
            }
            return Ok(());
        }
        if self
            .targets
            .iter()
            .all(|t| matches!(t.action, Action::Skip))
        {
            for target in &self.targets {
                events.push(InstallationEvent::Installed(target.name.clone()));
            }
            return Ok(());
        }
        fs::create_dir_all(&self.agents).context("无法创建角色目录")?;
        let conflicts: Vec<_> = self
            .targets
            .iter()
            .filter(|t| matches!(t.action, Action::BackupFile | Action::BackupLink(_)))
            .collect();
        if !conflicts.is_empty() {
            let backup_root = self.home.join("backups/my-codex-harness");
            fs::create_dir_all(&backup_root).context("无法创建备份目录")?;
            // Durable recovery data deliberately outlives this execution, including errors.
            let backup = tempfile::Builder::new()
                .prefix("agents-")
                .tempdir_in(&backup_root)?
                .keep();
            events.push(InstallationEvent::BackupCreated(backup.clone()));
            for (index, target) in conflicts.iter().enumerate() {
                before(Stage::Backup, index)?;
                let destination = backup.join(format!("{}.toml", target.name));
                match &target.action {
                    Action::BackupLink(original) => link(original, &destination)?,
                    Action::BackupFile => {
                        let mut source = File::open(&target.destination)?;
                        let permissions = source.metadata()?.permissions();
                        let mut copy = OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&destination)?;
                        io::copy(&mut source, &mut copy).context("无法复制角色备份")?;
                        copy.set_permissions(permissions)?;
                        copy.flush().context("无法写入角色备份")?;
                        copy.sync_all().context("无法保存角色备份")?;
                    }
                    _ => unreachable!(),
                }
            }
        }
        let temporary = tempfile::Builder::new()
            .prefix(".harness-install-")
            .tempdir_in(&self.agents)?;
        for (index, target) in self
            .targets
            .iter()
            .filter(|t| !matches!(t.action, Action::Skip))
            .enumerate()
        {
            let staged = temporary.path().join(format!("{index}.toml"));
            link(&target.source, &staged)?;
            before(Stage::Replace, index)?;
            fs::rename(&staged, &target.destination)
                .with_context(|| format!("无法替换角色 {}", target.name))?;
            events.push(InstallationEvent::Installed(target.name.clone()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Backup,
    Replace,
}

pub fn check_links(home: &Path, sources: &[(String, PathBuf)]) -> Vec<String> {
    let agents = match physical_directory(&home.join("agents")) {
        Ok(agents) => agents,
        Err(error) => return vec![format!("{error:#}")],
    };
    sources
        .iter()
        .filter_map(|(name, source)| {
            let target = agents.join(format!("{name}.toml"));
            match (fs::canonicalize(source), fs::canonicalize(&target)) {
                (Ok(source), Ok(target)) if source == target => None,
                _ => Some(format!(
                    "角色 {name} 缺失或安装目标错误：{}",
                    target.display()
                )),
            }
        })
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    fn prepare(home: &Path, sources: &[(String, PathBuf)]) -> Result<InstallationPlan> {
        InstallationPlan::prepare(home, sources, sources[0].1.parent().unwrap(), &[])
    }
    fn fixture() -> (tempfile::TempDir, PathBuf, Vec<(String, PathBuf)>) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        fs::create_dir(&source).unwrap();
        let mut sources = Vec::new();
        for name in ["alpha", "beta"] {
            let path = source.join(format!("{name}.toml"));
            fs::write(&path, name).unwrap();
            sources.push((name.into(), path));
        }
        let home = dir.path().join("home");
        (dir, home, sources)
    }
    fn backups(home: &Path) -> Vec<PathBuf> {
        fs::read_dir(home.join("backups/my-codex-harness"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect()
    }
    fn conflicts(home: &Path) {
        fs::create_dir_all(home.join("agents")).unwrap();
        fs::write(home.join("agents/alpha.toml"), "original").unwrap();
        symlink("../missing.toml", home.join("agents/beta.toml")).unwrap();
    }
    #[test]
    fn previews_do_not_create_or_modify_anything() {
        let (_dir, home, sources) = fixture();
        prepare(&home, &sources)
            .unwrap()
            .execute(true, &mut Vec::new())
            .unwrap();
        assert!(!home.exists());
        conflicts(&home);
        prepare(&home, &sources)
            .unwrap()
            .execute(true, &mut Vec::new())
            .unwrap();
        assert_eq!(
            fs::read_to_string(home.join("agents/alpha.toml")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_link(home.join("agents/beta.toml")).unwrap(),
            Path::new("../missing.toml")
        );
        assert!(!home.join("backups").exists());
    }
    #[test]
    fn full_preflight_rejects_later_directory_and_bad_backup_ancestors() {
        let (_dir, home, sources) = fixture();
        fs::create_dir_all(home.join("agents/beta.toml")).unwrap();
        assert!(prepare(&home, &sources).is_err());
        assert!(!home.join("agents/alpha.toml").exists());
        fs::remove_dir(home.join("agents/beta.toml")).unwrap();
        fs::write(home.join("agents/alpha.toml"), "old").unwrap();
        fs::write(home.join("backups"), "blocked").unwrap();
        assert!(prepare(&home, &sources).is_err());
        assert_eq!(
            fs::read_to_string(home.join("agents/alpha.toml")).unwrap(),
            "old"
        );
        fs::remove_file(home.join("backups")).unwrap();
        symlink("missing", home.join("backups")).unwrap();
        assert!(prepare(&home, &sources).is_err());
    }
    #[test]
    fn backups_preserve_permissions_links_and_user_configuration() {
        let (_dir, home, sources) = fixture();
        conflicts(&home);
        fs::set_permissions(
            home.join("agents/alpha.toml"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        fs::write(home.join("config.toml"), "user config").unwrap();
        fs::write(home.join("agents/custom.toml"), "custom").unwrap();
        prepare(&home, &sources)
            .unwrap()
            .execute(false, &mut Vec::new())
            .unwrap();
        let backup = &backups(&home)[0];
        assert_eq!(
            fs::read_to_string(backup.join("alpha.toml")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::metadata(backup.join("alpha.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        assert_eq!(
            fs::read_link(backup.join("beta.toml")).unwrap(),
            Path::new("../missing.toml")
        );
        assert!(check_links(&home, &sources).is_empty());
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).unwrap(),
            "user config"
        );
        assert_eq!(
            fs::read_to_string(home.join("agents/custom.toml")).unwrap(),
            "custom"
        );
        prepare(&home, &sources)
            .unwrap()
            .execute(false, &mut Vec::new())
            .unwrap();
        assert_eq!(backups(&home).len(), 1);
    }
    #[test]
    fn interrupted_backup_never_replaces_and_preserves_completed_backup() {
        let (_dir, home, sources) = fixture();
        conflicts(&home);
        let mut events = Vec::new();
        let result =
            prepare(&home, &sources)
                .unwrap()
                .execute_with(false, &mut events, |stage, index| {
                    if stage == Stage::Backup && index == 1 {
                        bail!("injected backup failure");
                    }
                    Ok(())
                });
        assert!(result.is_err());
        assert_eq!(
            events,
            vec![InstallationEvent::BackupCreated(
                backups(&home)[0].canonicalize().unwrap()
            )]
        );
        assert_eq!(
            fs::read_to_string(home.join("agents/alpha.toml")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_link(home.join("agents/beta.toml")).unwrap(),
            Path::new("../missing.toml")
        );
        assert_eq!(
            fs::read_to_string(backups(&home)[0].join("alpha.toml")).unwrap(),
            "original"
        );
    }
    #[test]
    fn interrupted_replacement_keeps_targets_and_can_resume() {
        let (_dir, home, sources) = fixture();
        conflicts(&home);
        let mut events = Vec::new();
        let result =
            prepare(&home, &sources)
                .unwrap()
                .execute_with(false, &mut events, |stage, index| {
                    if stage == Stage::Replace && index == 1 {
                        bail!("injected rename failure");
                    }
                    Ok(())
                });
        assert!(result.is_err());
        assert_eq!(
            events,
            vec![
                InstallationEvent::BackupCreated(backups(&home)[0].canonicalize().unwrap()),
                InstallationEvent::Installed("alpha".into()),
            ]
        );
        assert_eq!(check_links(&home, &sources).len(), 1);
        assert_eq!(
            fs::read_link(home.join("agents/beta.toml")).unwrap(),
            Path::new("../missing.toml")
        );
        assert!(backups(&home)[0].join("alpha.toml").exists());
        prepare(&home, &sources)
            .unwrap()
            .execute(false, &mut Vec::new())
            .unwrap();
        assert!(check_links(&home, &sources).is_empty());
        assert_eq!(backups(&home).len(), 2);
        assert!(!fs::read_dir(home.join("agents")).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".harness-install-")
        }));
    }
    #[test]
    fn linked_agents_directory_does_not_replace_source_files() {
        let (_dir, home, sources) = fixture();
        fs::create_dir(&home).unwrap();
        symlink(sources[0].1.parent().unwrap(), home.join("agents")).unwrap();
        prepare(&home, &sources)
            .unwrap()
            .execute(false, &mut Vec::new())
            .unwrap();
        assert!(!home.join("backups").exists());
        for (_, source) in &sources {
            assert!(fs::symlink_metadata(source).unwrap().is_file());
        }
        assert!(check_links(&home, &sources).is_empty());
    }
    #[test]
    fn missing_and_wrong_targets_are_reported() {
        let (_dir, home, sources) = fixture();
        assert_eq!(check_links(&home, &sources).len(), 2);
        conflicts(&home);
        assert_eq!(check_links(&home, &sources).len(), 2);
    }
    #[test]
    fn absolute_link_conflicts_preserve_the_original_literal() {
        let (dir, home, sources) = fixture();
        fs::create_dir_all(home.join("agents")).unwrap();
        let original = dir.path().join("absent.toml");
        symlink(&original, home.join("agents/alpha.toml")).unwrap();
        prepare(&home, &sources)
            .unwrap()
            .execute(false, &mut Vec::new())
            .unwrap();
        assert_eq!(
            fs::read_link(backups(&home)[0].join("alpha.toml")).unwrap(),
            original
        );
        assert!(check_links(&home, &sources).is_empty());
    }
    #[test]
    fn unavailable_target_ancestors_fail_without_creating_directories() {
        let (dir, _home, sources) = fixture();
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, "not a directory").unwrap();
        assert!(prepare(&blocked.join("home"), &sources).is_err());
        assert_eq!(fs::read_to_string(&blocked).unwrap(), "not a directory");
        let dangling = dir.path().join("dangling");
        symlink("missing", &dangling).unwrap();
        assert!(prepare(&dangling.join("home"), &sources).is_err());
        assert!(!dir.path().join("missing").exists());
    }

    #[test]
    fn missing_parent_traversal_does_not_create_discarded_source_directories() {
        let (dir, _home, sources) = fixture();
        let source_directory = sources[0].1.parent().unwrap();
        let requested = source_directory.join("discarded/../../new-home");
        let actual = dir.path().canonicalize().unwrap().join("new-home");
        let plan = prepare(&requested, &sources).unwrap();
        plan.execute(false, &mut Vec::new()).unwrap();
        assert!(!source_directory.join("discarded").exists());
        assert!(check_links(&actual, &sources).is_empty());
        for (name, source) in sources {
            assert_eq!(fs::read_to_string(source).unwrap(), name);
        }
    }
}
