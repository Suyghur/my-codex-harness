use anyhow::{Context, Result, bail};
use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

/// Captured once for a command; tests supply their own paths without changing
/// process-global environment or working directory.
pub(crate) struct Locations {
    pub cwd: PathBuf,
    pub user_home: Option<PathBuf>,
    pub codex_home: Option<PathBuf>,
}

impl Locations {
    pub fn capture() -> Result<Self> {
        Ok(Self {
            cwd: env::current_dir().context("读取当前目录")?,
            user_home: nonempty_env("HOME"),
            codex_home: nonempty_env("CODEX_HOME"),
        })
    }

    pub fn expand(&self, path: &Path) -> Result<PathBuf> {
        let bytes = path.as_os_str().as_encoded_bytes();
        let expanded = if path == Path::new("~") {
            self.user_home.clone().context("无法获取 HOME")?
        } else if bytes.starts_with(b"~/") {
            self.user_home
                .as_ref()
                .context("无法获取 HOME")?
                .join(path.strip_prefix("~")?)
        } else if bytes.starts_with(b"~") {
            bail!("不支持用户别名路径：{}", path.display());
        } else {
            path.to_path_buf()
        };
        Ok(if expanded.is_absolute() {
            expanded
        } else {
            self.cwd.join(expanded)
        })
    }

    pub fn root(&self, explicit: Option<&Path>) -> Result<PathBuf> {
        if let Some(path) = explicit {
            return self.expand(path);
        }
        for ancestor in self.cwd.ancestors() {
            let manifest = ancestor.join("harness.toml");
            match fs::metadata(&manifest) {
                Ok(info) if info.is_file() => return Ok(ancestor.to_path_buf()),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).with_context(|| format!("读取 {}", manifest.display())),
            }
        }
        bail!("未找到 harness.toml；请用 --root 指定资产仓库")
    }

    pub fn destination(&self, explicit: Option<&Path>) -> Result<PathBuf> {
        self.expand(
            explicit
                .or(self.codex_home.as_deref())
                .unwrap_or(Path::new("~/.codex")),
        )
    }
}

// Existing links retain physical parent semantics. Missing components can be
// discarded by `..`, but every subsequently existing component is inspected.
// No directory is created during resolution.
pub(crate) fn physical_directory(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().context("读取当前目录")?.join(path)
    };
    match fs::canonicalize(&absolute) {
        Ok(resolved) => {
            if !fs::metadata(&resolved)?.is_dir() {
                bail!("目录路径不可用：{}", path.display());
            }
            return Ok(resolved);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("无法解析目标目录 {}", path.display()));
        }
    }
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                resolved.push(component.as_os_str());
            }
            std::path::Component::ParentDir => {
                resolved.pop();
            }
            std::path::Component::CurDir => {}
            std::path::Component::Normal(name) => {
                resolved.push(name);
                match fs::symlink_metadata(&resolved) {
                    Ok(_) => {
                        let actual = fs::canonicalize(&resolved)
                            .with_context(|| format!("无法解析目录 {}", resolved.display()))?;
                        if !fs::metadata(&actual)?.is_dir() {
                            bail!("目录路径不可用：{}", resolved.display());
                        }
                        resolved = actual;
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("无法检查目录 {}", resolved.display()));
                    }
                }
            }
        }
    }
    Ok(resolved)
}

fn nonempty_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|v: &OsString| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expands_without_requiring_existing_directories() {
        let locations = Locations {
            cwd: "/tmp/project".into(),
            user_home: Some("/tmp/user".into()),
            codex_home: Some("config".into()),
        };
        assert_eq!(
            locations.expand(Path::new("~")).unwrap(),
            PathBuf::from("/tmp/user")
        );
        assert_eq!(
            locations.expand(Path::new("~/")).unwrap(),
            PathBuf::from("/tmp/user")
        );
        assert_eq!(
            locations.destination(None).unwrap(),
            PathBuf::from("/tmp/project/config")
        );
        assert_eq!(
            locations.destination(Some(Path::new("~/new"))).unwrap(),
            PathBuf::from("/tmp/user/new")
        );
        assert!(locations.expand(Path::new("~other")).is_err());
        let missing = Locations {
            user_home: None,
            ..locations
        };
        assert!(missing.destination(Some(Path::new("~/new"))).is_err());
    }
}
