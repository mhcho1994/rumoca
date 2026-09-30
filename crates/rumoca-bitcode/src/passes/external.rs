//! External passes run inside the compiler: `--pass exec:COMMAND`.
//!
//! The command is split on whitespace and run as
//! `COMMAND IN.rbc -o OUT.rbc`, the calling convention of every example in
//! `examples/bitcode-passes` and of docs/writing-a-bitcode-pass.md. `IN.rbc`
//! is the model as CBOR; `OUT.rbc` may be CBOR or JSON. The program's
//! standard output and standard error both go to the compiler's standard
//! error, so a pass cannot corrupt output the compiler writes to stdout.
//!
//! What comes back is untrusted: it is decoded with the header check (a
//! version this build cannot read is refused), then validated and rebuilt
//! through the checked constructors like any built-in pass's result.

use std::process::{Command, Stdio};

use super::PassError;
use crate::codec::{Encoding, decode, encode};
use crate::schema::RbcFile;

/// Run one external pass over `file`; return whether it changed the model.
pub fn run(file: &mut RbcFile, command: &str) -> Result<bool, PassError> {
    let failed = |detail: String| PassError(format!("`exec:{command}`: {detail}"));
    let mut words = command.split_whitespace();
    let program = words.next().ok_or_else(|| failed("empty command".into()))?;
    let directory = tempfile::tempdir().map_err(|error| failed(format!("temp dir: {error}")))?;
    let input = directory.path().join("in.rbc");
    let output = directory.path().join("out.rbc");
    let before = encode(file, Encoding::Cbor).map_err(|error| failed(error.to_string()))?;
    std::fs::write(&input, &before).map_err(|error| failed(format!("write input: {error}")))?;

    let status = Command::new(program)
        .args(words)
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::io::stderr()))
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| failed(format!("could not start `{program}`: {error}")))?;
    if !status.success() {
        return Err(failed(format!("exited with {status}")));
    }
    let bytes = std::fs::read(&output)
        .map_err(|error| failed(format!("wrote no output artifact: {error}")))?;
    let (rewritten, _) = decode(&bytes).map_err(|error| failed(error.to_string()))?;
    let after = encode(&rewritten, Encoding::Cbor).map_err(|error| failed(error.to_string()))?;
    *file = rewritten;
    Ok(after != before)
}
