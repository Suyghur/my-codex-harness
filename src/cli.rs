use crate::commands::{check::Check, install::Install, version::Version};
use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, builder::TypedValueParser};
use std::{ffi::OsString, io::Write, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "my-codex-harness",
    about = "检查和部署本地 Codex 角色资产",
    disable_version_flag = true
)]
struct Arguments {
    /// 资产仓库；默认从当前目录向上查找 harness.toml
    #[arg(long, global = true, value_parser = clap::builder::OsStringValueParser::new().try_map(nonempty_path))]
    root: Option<PathBuf>,
    /// 显示 CLI 版本与运行平台
    #[arg(long = "version")]
    show_version: bool,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// 只读检查资产和可选的安装链接
    Check(Check),
    /// 链接登记角色，冲突文件全部先备份
    Install(Install),
    /// 显示 CLI 版本与构建信息
    Version(Version),
}

pub(crate) fn nonempty_path(value: OsString) -> std::result::Result<PathBuf, String> {
    if value.is_empty() {
        Err("显式路径参数不能为空".into())
    } else {
        Ok(value.into())
    }
}

/// Execute one independent invocation, leaving process exit to the binary.
pub fn execute(
    args: impl IntoIterator<Item = OsString>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let arguments = match Arguments::try_parse_from(args) {
        Ok(a) => a,
        Err(e) => {
            let code = e.exit_code() as u8;
            let writer: &mut dyn Write = if e.use_stderr() { &mut *err } else { &mut *out };
            return if write!(writer, "{e}").is_ok() {
                code
            } else {
                1
            };
        }
    };
    if arguments.show_version && arguments.command.is_some() {
        let _ = writeln!(err, "--version 不能与子命令同时使用");
        return 2;
    }
    match dispatch(arguments, out, err) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(err, "{e:#}");
            1
        }
    }
}

fn dispatch(args: Arguments, out: &mut dyn Write, err: &mut dyn Write) -> Result<u8> {
    match args.command {
        None if args.show_version => Version::default().run(out),
        None => {
            write!(out, "{}", Arguments::command().render_help())?;
            writeln!(out)?;
            Ok(0)
        }
        Some(Action::Version(command)) => command.run(out),
        Some(Action::Check(command)) => command.run(args.root.as_deref(), out, err),
        Some(Action::Install(command)) => command.run(args.root.as_deref(), out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_failure_is_operational_failure() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        for args in [
            vec!["my-codex-harness", "version", "--json"],
            vec!["my-codex-harness", "--help"],
            vec!["my-codex-harness", "--version"],
        ] {
            let mut diagnostic = Vec::new();
            assert_eq!(
                execute(
                    args.into_iter().map(OsString::from),
                    &mut Broken,
                    &mut diagnostic
                ),
                1
            );
        }
    }
    #[test]
    fn invocations_do_not_share_flags() {
        let mut first = Vec::new();
        let mut second = Vec::new();
        let mut diagnostic = Vec::new();
        assert_eq!(
            execute(
                ["my-codex-harness", "version", "--json"].map(OsString::from),
                &mut first,
                &mut diagnostic
            ),
            0
        );
        assert_eq!(
            execute(
                ["my-codex-harness", "version"].map(OsString::from),
                &mut second,
                &mut diagnostic
            ),
            0
        );
        assert!(serde_json::from_slice::<serde_json::Value>(&first).is_ok());
        assert!(
            String::from_utf8(second)
                .unwrap()
                .starts_with("my-codex-harness ")
        );
    }
}
