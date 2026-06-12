// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded process helpers for live visual proof orchestration.
//!
//! The live runner eventually starts PipeWire, WirePlumber, Mutter, capture
//! helpers, and LushText itself. This module keeps process supervision separate
//! from scenario expansion so tests can prove timeout cleanup and log caps
//! without requiring a compositor.

#![allow(
    dead_code,
    reason = "process supervision lands before live-runner wiring so timeout and log-cap tests can anchor the contract"
)]

use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

/// Maximum combined stdout/stderr bytes retained for one supervised command.
///
/// Sixty-four KiB is enough for actionable headless setup diagnostics while
/// keeping CI artifacts and automation-client summaries away from unbounded
/// toolkit or renderer logs.
pub(crate) const MAX_LOG_BYTES: usize = 64 * 1024;

/// Result of a command that was supervised through the bounded log path.
#[derive(Debug)]
pub(crate) struct LoggedCommandResult {
    /// Exit status code when the process exited normally.
    pub(crate) exit_code: Option<i32>,
    /// Whether the command exceeded its timeout and was killed.
    pub(crate) timed_out: bool,
    /// Log artifact written for this command.
    pub(crate) log_path: PathBuf,
    /// Bytes intentionally omitted after `MAX_LOG_BYTES`.
    pub(crate) truncated_bytes: usize,
}

/// Long-running child process with stdout/stderr redirected to one log file.
#[derive(Debug)]
pub(crate) struct LoggedChild {
    child: Child,
    log_path: PathBuf,
    log: Arc<Mutex<BoundedLog<File>>>,
    stdout_reader: Option<thread::JoinHandle<Result<(), String>>>,
    stderr_reader: Option<thread::JoinHandle<Result<(), String>>>,
    logging_finished: bool,
}

impl LoggedChild {
    /// Return the child process id.
    pub(crate) fn id(&self) -> u32 {
        self.child.id()
    }

    /// Return the log artifact path.
    pub(crate) fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// Return whether the child has already exited.
    pub(crate) fn has_exited(&mut self) -> Result<bool, String> {
        let exited = self
            .child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|error| format!("cannot poll child process: {error}"))?;
        if exited {
            self.finish_logging()?;
        }
        Ok(exited)
    }

    /// Wait briefly for natural child exit without forcing cleanup.
    pub(crate) fn wait_for_exit(&mut self, timeout: Duration) -> Result<Option<i32>, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|error| format!("cannot poll child process: {error}"))?
            {
                self.finish_logging()?;
                return Ok(status.code());
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Wait for the child to finish, killing it if the timeout expires.
    pub(crate) fn terminate(&mut self, timeout: Duration) -> Result<(), String> {
        if self.has_exited()? {
            return Ok(());
        }
        self.request_termination()?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.has_exited()? {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.child
            .kill()
            .map_err(|error| format!("cannot kill child process {}: {error}", self.child.id()))?;
        self.child.wait().map_err(|error| {
            format!(
                "cannot reap killed child process {}: {error}",
                self.child.id()
            )
        })?;
        self.finish_logging()
    }

    #[cfg(unix)]
    fn request_termination(&mut self) -> Result<(), String> {
        let pid = self.child.id().to_string();
        let status = Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .map_err(|error| format!("cannot send SIGTERM to child process {pid}: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            self.child
                .kill()
                .map_err(|error| format!("cannot kill child process {pid}: {error}"))
        }
    }

    #[cfg(not(unix))]
    fn request_termination(&mut self) -> Result<(), String> {
        self.child
            .kill()
            .map_err(|error| format!("cannot kill child process {}: {error}", self.child.id()))
    }

    fn finish_logging(&mut self) -> Result<(), String> {
        if self.logging_finished {
            return Ok(());
        }
        join_reader(self.stdout_reader.take())?;
        join_reader(self.stderr_reader.take())?;
        let mut log = lock_log(&self.log, &self.log_path)?;
        log.finish()
            .map_err(|error| format!("cannot finish log {}: {error}", self.log_path.display()))?;
        drop(log);
        self.logging_finished = true;
        Ok(())
    }
}

/// Start a long-running process with stdout/stderr redirected to a log file.
pub(crate) fn start_logged_child(
    program: &str,
    args: &[&str],
    env: &[(String, String)],
    log_path: &Path,
) -> Result<LoggedChild, String> {
    let parent = log_path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", log_path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create log dir {}: {error}", parent.display()))?;
    let file = File::create(log_path)
        .map_err(|error| format!("cannot create log {}: {error}", log_path.display()))?;
    let log = Arc::new(Mutex::new(BoundedLog::new(file)));
    let child = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot launch {program}: {error}"))?;
    let mut child = child;
    let stdout_reader = child
        .stdout
        .take()
        .map(|stdout| spawn_log_reader(stdout, &log));
    let stderr_reader = child
        .stderr
        .take()
        .map(|stderr| spawn_log_reader(stderr, &log));
    Ok(LoggedChild {
        child,
        log_path: log_path.to_path_buf(),
        log,
        stdout_reader,
        stderr_reader,
        logging_finished: false,
    })
}

/// Run one command to completion while bounding its combined stdout/stderr log.
pub(crate) fn run_logged_command(
    program: &str,
    args: &[&str],
    env: &[(String, String)],
    log_path: &Path,
    timeout: Duration,
) -> Result<LoggedCommandResult, String> {
    let parent = log_path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", log_path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create log dir {}: {error}", parent.display()))?;
    let file = File::create(log_path)
        .map_err(|error| format!("cannot create log {}: {error}", log_path.display()))?;
    let log = Arc::new(Mutex::new(BoundedLog::new(file)));

    let mut command = Command::new(program);
    command
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot launch {program}: {error}"))?;

    let stdout_reader = child
        .stdout
        .take()
        .map(|stdout| spawn_log_reader(stdout, &log));
    let stderr_reader = child
        .stderr
        .take()
        .map(|stderr| spawn_log_reader(stderr, &log));

    let (exit_code, timed_out) = wait_or_kill(&mut child, timeout)?;
    join_reader(stdout_reader)?;
    join_reader(stderr_reader)?;
    let truncated_bytes = {
        let mut log = lock_log(&log, log_path)?;
        log.finish()
            .map_err(|error| format!("cannot finish log {}: {error}", log_path.display()))?;
        log.truncated_bytes
    };

    Ok(LoggedCommandResult {
        exit_code,
        timed_out,
        log_path: log_path.to_path_buf(),
        truncated_bytes,
    })
}

fn wait_or_kill(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<(Option<i32>, bool), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot poll child process: {error}"))?
        {
            return Ok((status.code(), false));
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .map_err(|error| format!("cannot kill timed-out child process: {error}"))?;
            let status = child
                .wait()
                .map_err(|error| format!("cannot reap timed-out child process: {error}"))?;
            return Ok((status.code(), true));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn spawn_log_reader<R>(
    reader: R,
    log: &Arc<Mutex<BoundedLog<File>>>,
) -> thread::JoinHandle<Result<(), String>>
where
    R: Read + Send + 'static,
{
    let log = Arc::clone(log);
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buffer = [0u8; 4096];
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("cannot read process output: {error}"))?;
            if count == 0 {
                return Ok(());
            }
            let mut log = log
                .lock()
                .map_err(|_| "log writer mutex poisoned".to_string())?;
            log.write_chunk(&buffer[..count])
                .map_err(|error| format!("cannot write bounded process log: {error}"))?;
        }
    })
}

fn join_reader(handle: Option<thread::JoinHandle<Result<(), String>>>) -> Result<(), String> {
    if let Some(handle) = handle {
        handle
            .join()
            .map_err(|_| "process log reader panicked".to_string())??;
    }
    Ok(())
}

fn lock_log<'a>(
    log: &'a Arc<Mutex<BoundedLog<File>>>,
    log_path: &Path,
) -> Result<MutexGuard<'a, BoundedLog<File>>, String> {
    log.lock()
        .map_err(|_| format!("log writer poisoned for {}", log_path.display()))
}

#[derive(Debug)]
struct BoundedLog<W> {
    writer: W,
    written_bytes: usize,
    truncated_bytes: usize,
}

impl<W> BoundedLog<W>
where
    W: Write,
{
    fn new(writer: W) -> Self {
        Self {
            writer,
            written_bytes: 0,
            truncated_bytes: 0,
        }
    }

    fn write_chunk(&mut self, chunk: &[u8]) -> std::io::Result<()> {
        let remaining = MAX_LOG_BYTES.saturating_sub(self.written_bytes);
        let keep = chunk.len().min(remaining);
        if keep > 0 {
            self.writer.write_all(&chunk[..keep])?;
            self.written_bytes += keep;
        }
        self.truncated_bytes += chunk.len() - keep;
        Ok(())
    }

    fn finish(&mut self) -> std::io::Result<()> {
        if self.truncated_bytes > 0 {
            writeln!(
                self.writer,
                "\n[truncated {} additional log bytes]",
                self.truncated_bytes
            )?;
        }
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_command_preserves_exit_code_and_log_path() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let log_path = tempdir.path().join("command.log");

        let result = run_logged_command(
            "/bin/sh",
            &["-c", "echo proof-log; exit 42"],
            &[],
            &log_path,
            Duration::from_secs(1),
        )
        .expect("logged command");

        assert_eq!(result.exit_code, Some(42));
        assert!(!result.timed_out);
        assert_eq!(result.log_path, log_path);
        assert!(
            fs::read_to_string(&result.log_path)
                .expect("log text")
                .contains("proof-log")
        );
    }

    #[test]
    fn logged_command_truncates_large_output() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let log_path = tempdir.path().join("large.log");

        let result = run_logged_command(
            "/bin/sh",
            &["-c", "printf '%70000s' ''"],
            &[],
            &log_path,
            Duration::from_secs(1),
        )
        .expect("logged command");
        let log_size = usize::try_from(fs::metadata(&result.log_path).expect("log metadata").len())
            .expect("test log size fits usize");
        let log_text = fs::read_to_string(&result.log_path).expect("log text");

        assert_eq!(result.exit_code, Some(0));
        assert!(result.truncated_bytes > 0);
        assert!(log_size < 70_000);
        assert!(log_text.contains("[truncated "));
    }

    #[test]
    fn logged_command_kills_timed_out_child() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let log_path = tempdir.path().join("timeout.log");

        let result = run_logged_command(
            "/bin/sh",
            &["-c", "echo started; sleep 5"],
            &[],
            &log_path,
            Duration::from_millis(20),
        )
        .expect("logged command");

        assert!(result.timed_out);
        assert_ne!(result.exit_code, Some(0));
        assert!(
            fs::read_to_string(&result.log_path)
                .expect("log text")
                .contains("started")
        );
    }

    #[test]
    fn logged_child_redirects_output_and_reports_exit() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let log_path = tempdir.path().join("child.log");

        let mut child = start_logged_child(
            "/bin/sh",
            &["-c", "echo child-ready; exit 7"],
            &[],
            &log_path,
        )
        .expect("logged child");
        let exit_code = child
            .wait_for_exit(Duration::from_secs(1))
            .expect("wait for child");

        assert_eq!(exit_code, Some(7));
        assert_eq!(child.log_path(), log_path);
        assert!(
            fs::read_to_string(&log_path)
                .expect("log text")
                .contains("child-ready")
        );
    }

    #[test]
    fn logged_child_terminate_kills_running_process() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let log_path = tempdir.path().join("long-running.log");

        let mut child = start_logged_child(
            "/bin/sh",
            &["-c", "echo child-started; exec sleep 60"],
            &[],
            &log_path,
        )
        .expect("logged child");
        assert_eq!(
            child
                .wait_for_exit(Duration::from_millis(20))
                .expect("child should still run"),
            None
        );

        child
            .terminate(Duration::from_secs(1))
            .expect("terminate child");

        assert!(child.has_exited().expect("poll terminated child"));
        assert!(
            fs::read_to_string(&log_path)
                .expect("log text")
                .contains("child-started")
        );
    }

    #[test]
    fn logged_child_log_is_bounded() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let log_path = tempdir.path().join("large-child.log");

        let mut child =
            start_logged_child("/bin/sh", &["-c", "printf '%70000s' ''"], &[], &log_path)
                .expect("logged child");
        let exit_code = child
            .wait_for_exit(Duration::from_secs(1))
            .expect("wait for child");
        let log_text = fs::read_to_string(&log_path).expect("log text");

        assert_eq!(exit_code, Some(0));
        assert!(fs::metadata(&log_path).expect("log metadata").len() < 70_000);
        assert!(log_text.contains("[truncated "));
    }
}
