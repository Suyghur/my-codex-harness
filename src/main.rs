use std::{io, process::ExitCode};

fn main() -> ExitCode {
    ExitCode::from(my_codex_harness::cli::execute(
        std::env::args_os(),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    ))
}
