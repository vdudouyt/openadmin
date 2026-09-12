//! Running a `Launch` off-PTY and capturing what it said.
//!
//! The PTY path (`term::session`) deliberately does not surface an exit status —
//! it only sees EOF. The agent needs one, so this spawns with pipes and waits,
//! following `mount::mount` (`src/mount.rs:41-83`) with two additions it needs:
//! a wall-clock timeout, and a cap on captured output so one `journalctl` cannot
//! fill the model's context window.

use crate::ssh::Launch;
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a finished command left behind.
#[derive(Debug, Clone, PartialEq)]
pub struct Captured {
    /// `None` when the process was killed by a signal or the timeout.
    pub exit: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    /// Output exceeded the cap and was elided in the middle.
    pub truncated: bool,
}

impl Captured {
    #[allow(dead_code)] // used by the plan executor, next commit
    pub fn success(&self) -> bool {
        self.exit == Some(0)
    }

    /// A compact rendering for the model: status first, then what was said.
    pub fn summarize(&self) -> String {
        let mut s = if self.timed_out {
            "timed out".to_string()
        } else {
            match self.exit {
                Some(0) => "exit 0".to_string(),
                Some(c) => format!("exit {c}"),
                None => "killed".to_string(),
            }
        };
        if self.truncated {
            s.push_str(" (output truncated)");
        }
        if !self.stdout.is_empty() {
            s.push('\n');
            s.push_str(&self.stdout);
        }
        if !self.stderr.is_empty() {
            s.push_str("\nstderr:\n");
            s.push_str(&self.stderr);
        }
        s
    }
}

/// A line of output as it happens, so a long-running step can stream.
pub type LineSink = Sender<(bool, String)>; // (is_stderr, line)

/// Spawn `launch`, optionally feed it `stdin_data`, and capture its output.
///
/// `cap` bounds each stream; beyond it the middle is dropped and `truncated` is
/// set — the head and tail of a log are what diagnose it, the middle rarely is.
pub fn run_capture(
    launch: &Launch,
    stdin_data: Option<&[u8]>,
    timeout: Duration,
    cap: usize,
    sink: Option<&LineSink>,
) -> Result<Captured> {
    let mut cmd = Command::new(&launch.program);
    cmd.args(&launch.args)
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &launch.env {
        cmd.env(k, v);
    }
    // Its own process group, so a timeout can kill the whole tree. Killing just
    // the direct child leaves grandchildren holding the pipes open — `ssh` and
    // its remote session, or a shell and its `sleep` — and the readers then
    // block until those exit, which is the very thing the timeout is for.
    cmd.process_group(0);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("run {}", launch.program))?;

    if let Some(data) = stdin_data
        && let Some(mut stdin) = child.stdin.take()
    {
        // Dropped at the end of this block, closing the pipe — without that a
        // `bash -s` on the far end waits forever for EOF.
        stdin.write_all(data).context("send stdin")?;
    }

    let out_buf = Arc::new(Mutex::new(Collector::new(cap)));
    let err_buf = Arc::new(Mutex::new(Collector::new(cap)));
    let out_thread = pump(
        child.stdout.take(),
        Arc::clone(&out_buf),
        sink.cloned(),
        false,
    );
    let err_thread = pump(
        child.stderr.take(),
        Arc::clone(&err_buf),
        sink.cloned(),
        true,
    );

    // std has no timed wait, so poll. 20 ms keeps a fast command fast without
    // spinning.
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().context("wait for command")? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            kill_group(child.id());
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    // Join after the child is gone, so the pipes are at EOF.
    let _ = out_thread.map(|t| t.join());
    let _ = err_thread.map(|t| t.join());

    let out = out_buf.lock().unwrap().finish();
    let err = err_buf.lock().unwrap().finish();

    Ok(Captured {
        exit: status.and_then(|s| s.code()),
        truncated: out.1 || err.1,
        stdout: out.0,
        stderr: err.0,
        timed_out,
    })
}

/// Kill a whole process group. `-pid` addresses the group whose leader is
/// `pid`, which is what `process_group(0)` made this child.
fn kill_group(pid: u32) {
    // SIGTERM first so ssh can tear the session down, then SIGKILL for anything
    // that ignored it.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    std::thread::sleep(Duration::from_millis(150));
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

fn pump<R: Read + Send + 'static>(
    reader: Option<R>,
    buf: Arc<Mutex<Collector>>,
    sink: Option<LineSink>,
    is_stderr: bool,
) -> Option<std::thread::JoinHandle<()>> {
    let mut reader = reader?;
    Some(std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        let mut pending = String::new();
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&chunk[..n]).into_owned();
                    buf.lock().unwrap().push(&text);
                    if let Some(sink) = &sink {
                        // Emit whole lines as they complete, so the UI sees
                        // progress rather than one dump at the end.
                        pending.push_str(&text);
                        while let Some(i) = pending.find('\n') {
                            let line: String = pending.drain(..=i).collect();
                            if sink
                                .send((is_stderr, line.trim_end_matches('\n').to_string()))
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        if let Some(sink) = &sink
            && !pending.is_empty()
        {
            let _ = sink.send((is_stderr, pending));
        }
    }))
}

/// Keeps the head and tail of a stream, dropping the middle once it overflows.
struct Collector {
    cap: usize,
    head: String,
    tail: std::collections::VecDeque<char>,
    overflowed: bool,
}

impl Collector {
    fn new(cap: usize) -> Self {
        Collector {
            cap: cap.max(64),
            head: String::new(),
            tail: std::collections::VecDeque::new(),
            overflowed: false,
        }
    }

    fn push(&mut self, text: &str) {
        let half = self.cap / 2;
        for c in text.chars() {
            if self.head.chars().count() < half {
                self.head.push(c);
            } else {
                self.tail.push_back(c);
                if self.tail.len() > half {
                    self.tail.pop_front();
                    self.overflowed = true;
                }
            }
        }
    }

    /// `(text, was_truncated)`.
    fn finish(&mut self) -> (String, bool) {
        let mut s = std::mem::take(&mut self.head);
        if self.overflowed {
            s.push_str("\n… output truncated …\n");
        }
        s.extend(self.tail.iter());
        (s, self.overflowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Launch {
        Launch {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            env: Vec::new(),
        }
    }

    #[test]
    fn captures_stdout_and_a_zero_exit() {
        let c = run_capture(&sh("echo hello"), None, Duration::from_secs(10), 4096, None).unwrap();
        assert!(c.success());
        assert_eq!(c.exit, Some(0));
        assert_eq!(c.stdout.trim(), "hello");
        assert!(!c.timed_out);
        assert!(!c.truncated);
    }

    #[test]
    fn a_failure_reports_its_code_and_stderr() {
        let c = run_capture(
            &sh("echo problem >&2; exit 3"),
            None,
            Duration::from_secs(10),
            4096,
            None,
        )
        .unwrap();
        assert!(!c.success());
        assert_eq!(c.exit, Some(3));
        assert_eq!(c.stderr.trim(), "problem");
        assert!(c.summarize().starts_with("exit 3"), "{}", c.summarize());
    }

    #[test]
    fn stdin_is_delivered_and_the_pipe_is_closed() {
        // `cat` only exits when it sees EOF, so this hangs if the pipe leaks.
        let c = run_capture(
            &sh("cat"),
            Some(b"from stdin\n"),
            Duration::from_secs(10),
            4096,
            None,
        )
        .unwrap();
        assert_eq!(c.stdout.trim(), "from stdin");
        assert_eq!(c.exit, Some(0));
    }

    #[test]
    fn a_hanging_command_is_killed_at_the_deadline() {
        let start = Instant::now();
        let c = run_capture(
            &sh("echo starting; sleep 30"),
            None,
            Duration::from_millis(400),
            4096,
            None,
        )
        .unwrap();
        assert!(c.timed_out);
        assert_eq!(c.exit, None);
        assert!(c.stdout.contains("starting"), "partial output is kept");
        assert!(start.elapsed() < Duration::from_secs(5), "killed promptly");
        assert!(c.summarize().starts_with("timed out"));
    }

    #[test]
    fn oversized_output_keeps_the_head_and_the_tail() {
        let c = run_capture(
            &sh("for i in $(seq 1 2000); do echo line-$i; done"),
            None,
            Duration::from_secs(20),
            400,
            None,
        )
        .unwrap();
        assert!(c.truncated);
        assert!(c.stdout.contains("line-1\n"), "head kept: {}", c.stdout);
        assert!(c.stdout.contains("line-2000"), "tail kept: {}", c.stdout);
        assert!(c.stdout.contains("truncated"));
        assert!(
            c.stdout.len() < 1200,
            "actually bounded: {}",
            c.stdout.len()
        );
    }

    #[test]
    fn lines_stream_to_the_sink_as_they_arrive() {
        let (tx, rx) = std::sync::mpsc::channel();
        let c = run_capture(
            &sh("echo one; echo two; echo err >&2"),
            None,
            Duration::from_secs(10),
            4096,
            Some(&tx),
        )
        .unwrap();
        drop(tx);
        assert!(c.success());
        let lines: Vec<(bool, String)> = rx.iter().collect();
        assert!(lines.contains(&(false, "one".to_string())), "{lines:?}");
        assert!(lines.contains(&(false, "two".to_string())), "{lines:?}");
        assert!(lines.contains(&(true, "err".to_string())), "{lines:?}");
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_panic() {
        let l = Launch {
            program: "/nonexistent/openadmin-test".to_string(),
            args: Vec::new(),
            env: Vec::new(),
        };
        assert!(run_capture(&l, None, Duration::from_secs(5), 1024, None).is_err());
    }
}
