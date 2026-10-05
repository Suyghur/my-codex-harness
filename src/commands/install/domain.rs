//! Read-only planning followed by all backups, then individual atomic replacements.
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
#[derive(Debug)]
pub struct InstallationPlan {
    home: PathBuf,
    agents: PathBuf,
    targets: Vec<Target>,
}

// Follow existing directory links, but reject dangling links and non-directory ancestors.
fn inspect_directory(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                if !fs::metadata(ancestor)
                    .with_context(|| format!("无法解析目录 {}", ancestor.display()))?
                    .is_dir()
                {
                    bail!("目录路径不可用：{}", ancestor.display());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("无法检查目录 {}", ancestor.display()));
            }
        }
    }
    Ok(())
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
    pub fn prepare(home: &Path, sources: &[(String, PathBuf)]) -> Result<Self> {
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            bail!("安装仅支持 macOS/Linux");
        }
        let home = if home.is_absolute() {
            home.to_path_buf()
        } else {
            std::env::current_dir()?.join(home)
        };
        let agents = home.join("agents");
        inspect_directory(&agents)?;
        let mut targets = Vec::with_capacity(sources.len());
        let mut names = std::collections::HashSet::new();
        for (name, source) in sources {
            if name.is_empty()
                || Path::new(name).components().count() != 1
                || name == "."
                || name == ".."
                || name.contains('/')
                || name.contains('\\')
                || !names.insert(name)
            {
                bail!("非法或重复角色名称：{name}");
            }
            let source = fs::canonicalize(source)
                .with_context(|| format!("无法解析角色源文件 {}", source.display()))?;
            if !fs::metadata(&source)?.is_file() {
                bail!("角色源不是普通文件：{}", source.display());
            }
            let destination = agents.join(format!("{name}.toml"));
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
            inspect_directory(&home.join("backups/my-codex-harness"))?;
        }
        Ok(Self {
            home,
            agents,
            targets,
        })
    }

    pub fn execute(self, dry_run: bool, output: &mut dyn Write) -> Result<()> {
        self.execute_with(dry_run, output, |_, _| Ok(()))
    }

    fn execute_with(
        self,
        dry_run: bool,
        output: &mut dyn Write,
        mut before: impl FnMut(Stage, usize) -> Result<()>,
    ) -> Result<()> {
        if dry_run {
            for target in &self.targets {
                if matches!(target.action, Action::Skip) {
                    writeln!(output, "已安装：{}", target.name)?;
                } else {
                    writeln!(
                        output,
                        "预览：{} -> {}",
                        target.destination.display(),
                        target.source.display()
                    )?;
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
                writeln!(output, "已安装：{}", target.name)?;
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
            writeln!(output, "备份：{}", backup.display())?;
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
            writeln!(output, "已安装：{}", target.name)?;
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
    sources
        .iter()
        .filter_map(|(name, source)| {
            let target = home.join("agents").join(format!("{name}.toml"));
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
        InstallationPlan::prepare(&home, &sources)
            .unwrap()
            .execute(true, &mut Vec::new())
            .unwrap();
        assert!(!home.exists());
        conflicts(&home);
        InstallationPlan::prepare(&home, &sources)
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
        assert!(InstallationPlan::prepare(&home, &sources).is_err());
        assert!(!home.join("agents/alpha.toml").exists());
        fs::remove_dir(home.join("agents/beta.toml")).unwrap();
        fs::write(home.join("agents/alpha.toml"), "old").unwrap();
        fs::write(home.join("backups"), "blocked").unwrap();
        assert!(InstallationPlan::prepare(&home, &sources).is_err());
        assert_eq!(
            fs::read_to_string(home.join("agents/alpha.toml")).unwrap(),
            "old"
        );
        fs::remove_file(home.join("backups")).unwrap();
        symlink("missing", home.join("backups")).unwrap();
        assert!(InstallationPlan::prepare(&home, &sources).is_err());
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
        InstallationPlan::prepare(&home, &sources)
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
        InstallationPlan::prepare(&home, &sources)
            .unwrap()
            .execute(false, &mut Vec::new())
            .unwrap();
        assert_eq!(backups(&home).len(), 1);
    }
    #[test]
    fn interrupted_backup_never_replaces_and_preserves_completed_backup() {
        let (_dir, home, sources) = fixture();
        conflicts(&home);
        let result = InstallationPlan::prepare(&home, &sources)
            .unwrap()
            .execute_with(false, &mut Vec::new(), |stage, index| {
                if stage == Stage::Backup && index == 1 {
                    bail!("injected backup failure");
                }
                Ok(())
            });
        assert!(result.is_err());
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
        let result = InstallationPlan::prepare(&home, &sources)
            .unwrap()
            .execute_with(false, &mut Vec::new(), |stage, index| {
                if stage == Stage::Replace && index == 1 {
                    bail!("injected rename failure");
                }
                Ok(())
            });
        assert!(result.is_err());
        assert_eq!(check_links(&home, &sources).len(), 1);
        assert_eq!(
            fs::read_link(home.join("agents/beta.toml")).unwrap(),
            Path::new("../missing.toml")
        );
        assert!(backups(&home)[0].join("alpha.toml").exists());
        InstallationPlan::prepare(&home, &sources)
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
        InstallationPlan::prepare(&home, &sources)
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
        InstallationPlan::prepare(&home, &sources)
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
        assert!(InstallationPlan::prepare(&blocked.join("home"), &sources).is_err());
        assert_eq!(fs::read_to_string(&blocked).unwrap(), "not a directory");
        let dangling = dir.path().join("dangling");
        symlink("missing", &dangling).unwrap();
        assert!(InstallationPlan::prepare(&dangling.join("home"), &sources).is_err());
        assert!(!dir.path().join("missing").exists());
    }
}
