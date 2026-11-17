#![forbid(unsafe_code)]

use silverwave_cli::{GlobalOptions, RecoveryMode, ReplArgs, Subcommand, execute_cli_subcommand};
use std::process::ExitCode;

fn main() -> ExitCode {
    let global = GlobalOptions::default();
    let report = execute_cli_subcommand(
        &global,
        Subcommand::Repl(ReplArgs {
            file: None,
            sample_rate: 48_000,
            buffer_size: 128,
            cps: 0.5,
            headless_repl: true,
            recovery_mode: RecoveryMode::ResumeLastGood,
        }),
    );
    // `Subcommand::Repl` always returns a non-empty banner, so no empty-stdout
    // guard is needed here.
    println!("{}", report.stdout_text);
    report.exit_code.into()
}
