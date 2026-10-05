#![cfg(feature = "cli")]

//! Interactive confirmation shared by every subcommand that can actuate hardware.

use std::io::{self, Write};

use crate::error::CliError;

/// Prints `prompt` to stderr and returns whether the operator typed `yes` (any case).
///
/// Anything else, including EOF on a closed stdin, is a refusal and prints why.
pub(crate) fn confirm(prompt: &str) -> Result<bool, CliError> {
    eprint!("{prompt}");
    io::stderr().flush().unwrap_or(());
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim().eq_ignore_ascii_case("yes") {
        return Ok(true);
    }
    eprintln!("Aborted (expected 'yes').");
    Ok(false)
}
