//! JSON-lines simulation session input and transport.

use super::*;

/// Parse one `--input NAME=VALUE` initial input.
pub(super) fn parse_initial_input(text: &str) -> std::result::Result<(String, f64), String> {
    let (name, value) = text
        .split_once('=')
        .ok_or_else(|| format!("expected NAME=VALUE, got `{text}`"))?;
    let value = value
        .trim()
        .parse::<f64>()
        .map_err(|error| format!("input `{name}` value `{value}` is not a number: {error}"))?;
    Ok((name.trim().to_owned(), value))
}

/// `sim --serve-stdio`: drive one session from JSON-lines commands on stdin.
///
/// Stdout carries only protocol events; the process exits with the status of
/// how the controller ended the session (`0` after `close`).
pub(super) fn serve_stdio_session(dae: &Dae, opts: SimOptions) -> Result<()> {
    let mut session = rumoca_sim::SimulationSession::new(dae, opts)
        .map_err(|error| simulation_failure_error(&error))?;
    let exit = rumoca_sim::serve_session(
        &mut session,
        std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
    )?;
    match exit {
        rumoca_sim::SessionServeExit::Closed => Ok(()),
        ended => {
            eprintln!("rumoca sim --serve-stdio: session ended without close ({ended:?})");
            std::process::exit(ended.exit_code())
        }
    }
}
