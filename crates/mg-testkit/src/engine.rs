//! Engine tests: run a module in the game's headless server (`nwserver`)
//! with a throwaway user directory and read what its scripts log.
//!
//! Test scripts report with `WriteTimestampedLogEntry("...")`, which lands in
//! `<userdir>/logs.0/nwserverLog1.txt`; the runner stops the server once a
//! line containing the done marker appears, or at the timeout.
//!
//! The user directory is always one the test created, never the player's
//! real NWN folder.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

pub use mg_serve::server_binary;

/// What a server run logged.
#[derive(Debug, Clone)]
pub struct ServerRun {
    /// The server log (`nwserverLog1.txt`).
    pub log: String,
    /// Whether the done marker appeared before the timeout.
    pub finished: bool,
}

impl ServerRun {
    /// Every logged line containing `tag`, with the part after the tag.
    pub fn values(&self, tag: &str) -> Vec<String> {
        self.log
            .lines()
            .filter_map(|l| l.find(tag).map(|i| l[i + tag.len()..].trim().to_string()))
            .collect()
    }
}

/// Runs `<user_dir>/modules/<module>.mod`
/// in the headless server until a log line contains `done_marker`, the
/// server shuts down or `timeout` passes.
pub fn run_server(
    root: &Path,
    user_dir: &Path,
    module: &str,
    done_marker: &str,
    timeout: Duration,
) -> io::Result<ServerRun> {
    let server = server_binary(root)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, mg_serve::no_server(root)))?;
    // Never listed publicly, and locked against anyone who finds the port
    // anyway: what `mg_serve` does unless asked otherwise.
    let server = mg_serve::Server::start(&mg_serve::Options {
        server,
        user_dir: user_dir.to_path_buf(),
        module: module.to_string(),
        name: "moonglow-test".into(),
        ..Default::default()
    })?;

    let start = Instant::now();
    loop {
        let log = server.log();
        if log.contains(done_marker) {
            return Ok(ServerRun { log, finished: true });
        }
        // It quit (a module it can't load): nothing more will come.
        if start.elapsed() > timeout || log.contains("Server shutting down") {
            return Ok(ServerRun { log, finished: false });
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
