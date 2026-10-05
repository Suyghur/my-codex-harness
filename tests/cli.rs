//! End-to-end contracts use isolated child-process environment and cwd.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

struct Fixture {
    dir: TempDir,
    root: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let root = dir.path().join("repository");
        let home = dir.path().join("user");
        fs::create_dir_all(root.join("agents")).unwrap();
        fs::create_dir_all(root.join("evals")).unwrap();
        fs::create_dir(&home).unwrap();
        fs::write(root.join("harness.toml"), "schema_version=1\nname='fixture'\nversion='asset-9'\n[roles.worker]\nfile='agents/worker.toml'\nmodes=['custom']\naccess='task-scoped'\n[roles.reader]\nfile='agents/reader.toml'\nmodes=['explore']\naccess='read-only'\n").unwrap();
        for name in ["reader", "worker"] {
            fs::write(
                root.join("agents").join(format!("{name}.toml")),
                format!(
                    "name='{name}'\ndescription='test'\ndeveloper_instructions='测试 <&> prompt'\n"
                ),
            )
            .unwrap();
        }
        fs::write(root.join("evals/scenarios.json"), r#"{"schema_version":1,"status":"seed_not_run","scenarios":[{"id":"custom","role":"worker","mode":"custom","setup":"x","task":"x","criteria":["x"]}]}"#).unwrap();
        Self { dir, root, home }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_my-codex-harness"));
        command
            .current_dir(self.dir.path())
            .env("HOME", &self.home)
            .env_remove("CODEX_HOME");
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn asset_command(&self) -> Command {
        let mut c = self.command();
        c.arg("--root").arg(&self.root);
        c
    }
}
fn code(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn standalone_help_and_build_metadata() {
    let f = Fixture::new();
    for args in [
        vec![],
        vec!["--help"],
        vec!["check", "--help"],
        vec!["version"],
        vec!["--version"],
    ] {
        code(&f.run(&args), 0);
    }
    let output = f.run(&["version", "--json"]);
    code(&output, 0);
    assert!(output.stderr.is_empty());
    let value = json(&output);
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["os"], std::env::consts::OS);
    assert_eq!(value["arch"], std::env::consts::ARCH);
    assert!(
        value["rust_version"]
            .as_str()
            .unwrap()
            .starts_with("rustc ")
    );
    assert!(value.get("go_version").is_none());
    assert!(
        String::from_utf8(f.run(&["--version"]).stdout)
            .unwrap()
            .starts_with("my-codex-harness ")
    );
}

#[test]
fn invalid_command_arguments_are_exit_two() {
    let f = Fixture::new();
    for args in [
        vec!["unknown"],
        vec!["check", "--dry-run"],
        vec!["check", "--root="],
        vec!["install", "--codex-home="],
        vec!["version", "extra"],
        vec!["check", "extra"],
        vec!["--version", "check"],
        vec!["version", "--root="],
        vec!["--root=", "version"],
        vec!["--version", "--root="],
        vec!["--root="],
        vec!["install", "--json"],
    ] {
        let output = f.run(&args);
        code(&output, 2);
        assert!(!output.stderr.is_empty());
    }
    assert!(!f.home.join(".codex").exists());
}

#[test]
fn discovery_and_explicit_roots_from_outside_repository() {
    let f = Fixture::new();
    code(&f.run(&["check"]), 1);
    let output = f
        .asset_command()
        .args(["check", "--json"])
        .output()
        .unwrap();
    code(&output, 0);
    let other = f
        .command()
        .arg("check")
        .arg("--root")
        .arg(&f.root)
        .arg("--json")
        .output()
        .unwrap();
    code(&other, 0);
    assert_eq!(json(&output), json(&other));
    let nested = f
        .command()
        .current_dir(f.root.join("agents"))
        .args(["check", "--json"])
        .output()
        .unwrap();
    code(&nested, 0);
    assert_eq!(json(&nested), json(&other));
    let relative = f.run(&["check", "--root", "repository", "--json"]);
    code(&relative, 0);
    assert_eq!(json(&relative), json(&other));
}

#[test]
fn check_json_and_text_keep_output_streams_separate() {
    let f = Fixture::new();
    let output = f
        .asset_command()
        .args(["check", "--json", "--scenarios"])
        .output()
        .unwrap();
    code(&output, 0);
    assert!(output.stderr.is_empty());
    let value = json(&output);
    assert_eq!(value["status"], "passed");
    assert_eq!(value["errors"], serde_json::json!([]));
    assert_eq!(value["behavior_evaluation"], "not_run");
    assert_eq!(value["bundle"]["version"], "asset-9");
    assert_eq!(value["bundle"]["fingerprint_algorithm"], "sha256-json-v3");
    assert!(value["bundle"]["roles"][0]["model"].is_null());
    assert!(value["bundle"]["roles"][0]["reasoning_effort"].is_null());
    let text = f.asset_command().arg("check").output().unwrap();
    code(&text, 0);
    assert!(
        String::from_utf8(text.stdout)
            .unwrap()
            .contains("资产检查通过")
    );
    assert!(String::from_utf8(text.stderr).unwrap().contains("不代表"));
    let missing = f
        .command()
        .args(["check", "--root"])
        .arg(&f.home)
        .arg("--json")
        .output()
        .unwrap();
    code(&missing, 1);
    assert!(missing.stderr.is_empty());
    let value = json(&missing);
    assert_eq!(value["status"], "failed");
    assert_eq!(value["behavior_evaluation"], "not_run");
    assert!(value.get("bundle").is_none());
    assert!(!value["errors"].as_array().unwrap().is_empty());
}

#[test]
fn invalid_scenarios_keep_bundle_but_never_evaluate_behavior() {
    let f = Fixture::new();
    fs::write(
        f.root.join("evals/scenarios.json"),
        r#"{"schema_version":1,"status":"seed_not_run","scenarios":[]}"#,
    )
    .unwrap();
    let output = f
        .asset_command()
        .args(["check", "--json", "--scenarios"])
        .output()
        .unwrap();
    code(&output, 1);
    let value = json(&output);
    assert!(value.get("bundle").is_some());
    assert_eq!(value["behavior_evaluation"], "not_run");
    assert!(output.stderr.is_empty());
}

#[test]
fn rejected_assets_produce_json_failure_and_no_installation() {
    let f = Fixture::new();
    fs::write(f.root.join("agents/extra.toml"), "name='extra'").unwrap();
    let check = f
        .asset_command()
        .args(["check", "--json"])
        .output()
        .unwrap();
    code(&check, 1);
    assert_eq!(json(&check)["status"], "failed");
    let install = f.asset_command().arg("install").output().unwrap();
    code(&install, 1);
    assert!(!f.home.join(".codex").exists());
}

#[test]
fn installation_directory_priority_and_dry_run() {
    let f = Fixture::new();
    let env_home = f.dir.path().join("env-config");
    let explicit = f.dir.path().join("explicit-config");
    let preview = f
        .asset_command()
        .env("CODEX_HOME", &env_home)
        .arg("install")
        .arg("--codex-home")
        .arg(&explicit)
        .arg("--dry-run")
        .output()
        .unwrap();
    code(&preview, 0);
    assert!(!explicit.exists());
    assert!(!env_home.exists());
    let install = f
        .asset_command()
        .env("CODEX_HOME", &env_home)
        .arg("install")
        .arg("--codex-home")
        .arg(&explicit)
        .output()
        .unwrap();
    code(&install, 0);
    assert!(!env_home.exists());
    let check = f
        .asset_command()
        .arg("check")
        .arg("--codex-home")
        .arg(&explicit)
        .arg("--json")
        .output()
        .unwrap();
    code(&check, 0);
    code(
        &f.asset_command()
            .env("CODEX_HOME", &env_home)
            .arg("install")
            .output()
            .unwrap(),
        0,
    );
    code(
        &f.asset_command()
            .env("CODEX_HOME", "")
            .arg("install")
            .output()
            .unwrap(),
        0,
    );
    assert!(f.home.join(".codex/agents/worker.toml").exists());
    assert!(env_home.join("agents/reader.toml").exists());
}

#[test]
fn tilde_and_missing_target_checks() {
    let f = Fixture::new();
    code(
        &f.asset_command()
            .args(["install", "--codex-home", "~/new", "--dry-run"])
            .output()
            .unwrap(),
        0,
    );
    assert!(!f.home.join("new").exists());
    for root in ["~", "~/"] {
        code(
            &f.command()
                .env("HOME", &f.root)
                .args(["check", "--root", root, "--json"])
                .output()
                .unwrap(),
            0,
        );
    }
    let alias = f
        .asset_command()
        .args(["install", "--codex-home", "~another", "--dry-run"])
        .output()
        .unwrap();
    code(&alias, 1);
    let missing = f
        .asset_command()
        .args(["check", "--codex-home"])
        .arg(f.dir.path().join("missing"))
        .arg("--json")
        .output()
        .unwrap();
    code(&missing, 1);
    assert_eq!(json(&missing)["errors"].as_array().unwrap().len(), 2);
}

#[cfg(unix)]
#[test]
fn conflicts_are_backed_up_and_repeat_install_is_idempotent() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let home = f.dir.path().join("config");
    fs::create_dir_all(home.join("agents")).unwrap();
    let worker = home.join("agents/worker.toml");
    fs::write(&worker, "user original").unwrap();
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o600)).unwrap();
    symlink("../missing.toml", home.join("agents/reader.toml")).unwrap();
    fs::write(home.join("agents/custom.toml"), "custom").unwrap();
    fs::write(home.join("config.toml"), "user config").unwrap();
    let preview = f
        .asset_command()
        .arg("install")
        .arg("--codex-home")
        .arg(&home)
        .arg("--dry-run")
        .output()
        .unwrap();
    code(&preview, 0);
    assert!(!home.join("backups").exists());
    assert_eq!(fs::read_to_string(&worker).unwrap(), "user original");
    for _ in 0..2 {
        code(
            &f.asset_command()
                .arg("install")
                .arg("--codex-home")
                .arg(&home)
                .output()
                .unwrap(),
            0,
        );
    }
    let backups: Vec<_> = fs::read_dir(home.join("backups/my-codex-harness"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        fs::read_to_string(backups[0].join("worker.toml")).unwrap(),
        "user original"
    );
    assert_eq!(
        fs::metadata(backups[0].join("worker.toml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::read_link(backups[0].join("reader.toml")).unwrap(),
        Path::new("../missing.toml")
    );
    assert_eq!(
        fs::read_to_string(home.join("config.toml")).unwrap(),
        "user config"
    );
    assert_eq!(
        fs::read_to_string(home.join("agents/custom.toml")).unwrap(),
        "custom"
    );
    let source = f.root.join("agents/worker.toml").canonicalize().unwrap();
    assert_eq!(fs::read_link(&worker).unwrap(), source);
}

#[cfg(unix)]
#[test]
fn preflight_rejects_all_targets_before_any_installation() {
    let f = Fixture::new();
    let home = f.dir.path().join("config");
    fs::create_dir_all(home.join("agents/worker.toml")).unwrap();
    for dry in [false, true] {
        let mut command = f.asset_command();
        command.arg("install").arg("--codex-home").arg(&home);
        if dry {
            command.arg("--dry-run");
        }
        code(&command.output().unwrap(), 1);
        assert!(!home.join("agents/reader.toml").exists());
        assert!(!home.join("backups").exists());
    }
}

#[cfg(unix)]
#[test]
fn already_linked_agents_directory_does_not_modify_sources() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let home = f.dir.path().join("config");
    fs::create_dir(&home).unwrap();
    symlink(f.root.join("agents"), home.join("agents")).unwrap();
    let source = f.root.join("agents/worker.toml");
    let original = fs::read(&source).unwrap();
    code(
        &f.asset_command()
            .arg("install")
            .arg("--codex-home")
            .arg(&home)
            .output()
            .unwrap(),
        0,
    );
    assert!(
        !fs::symlink_metadata(&source)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(!home.join("backups").exists());
}

// macOS filesystems reject non-UTF-8 names before the CLI can read them.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_root_paths_are_accepted() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let f = Fixture::new();
    let nonutf = f.dir.path().join(OsString::from_vec(b"root-\xff".to_vec()));
    fs::rename(&f.root, &nonutf).unwrap();
    code(
        &f.command()
            .arg("check")
            .arg("--root")
            .arg(nonutf)
            .arg("--json")
            .output()
            .unwrap(),
        0,
    );
}
