//! One PTY-backed terminal session: a child process, a vt100 emulator fed by a
//! reader thread, and the plumbing to resize and write to it.

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub type SessionId = u64;

/// What a session tells the event loop. Output is coalesced: the reader thread
/// only sends `Output` on a clean->dirty transition, so `cat` of a large file
/// cannot flood the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermEvent {
    Output(SessionId),
    Exit(SessionId),
}

/// A spawnable command: program, args, and extra environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Spawn {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Spawn {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(program: impl Into<String>) -> Self {
        Spawn {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }
}

pub struct TerminalSession {
    /// The host nickname. Terminal-set titles are deliberately ignored so a tab
    /// always reads as the host it connects to.
    pub title: String,
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    dirty: Arc<AtomicBool>,
    size: (u16, u16),
    exited: Arc<AtomicBool>,
}

impl TerminalSession {
    /// Open a PTY, spawn `spawn` on it, and start pumping its output into a
    /// vt100 emulator.
    pub fn spawn(
        id: SessionId,
        title: impl Into<String>,
        spawn: &Spawn,
        size: (u16, u16),
        scrollback: usize,
        term: &str,
        tx: Sender<TermEvent>,
    ) -> Result<Self> {
        let (rows, cols) = sane_size(size);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open pty")?;

        let mut cmd = CommandBuilder::new(&spawn.program);
        cmd.args(&spawn.args);
        cmd.env("TERM", term);
        cmd.env("COLORTERM", "truecolor");
        // A nested session must not inherit the outer one's identity, and the
        // helper installs its own askpass wiring.
        for stale in [
            "TMUX",
            "TMUX_PANE",
            "STY",
            "SSH_ASKPASS",
            "SSH_ASKPASS_REQUIRE",
        ] {
            cmd.env_remove(stale);
        }
        for (k, v) in &spawn.env {
            cmd.env(k, v);
        }
        if let Some(home) = dirs::home_dir() {
            cmd.cwd(home);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("spawn command on pty")?;
        // Drop our handle on the slave so the master reader sees EOF once the
        // child and its descendants are gone.
        drop(pair.slave);

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, scrollback)));
        let dirty = Arc::new(AtomicBool::new(true));
        let exited = Arc::new(AtomicBool::new(false));

        let reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;

        {
            let parser = Arc::clone(&parser);
            let dirty = Arc::clone(&dirty);
            let exited = Arc::clone(&exited);
            std::thread::Builder::new()
                .name(format!("openadmin-pty-{id}"))
                .spawn(move || pump(id, reader, parser, dirty, exited, tx))
                .context("spawn pty reader thread")?;
        }

        Ok(TerminalSession {
            title: title.into(),
            parser,
            writer,
            master: pair.master,
            child,
            dirty,
            size: (rows, cols),
            exited,
        })
    }

    pub fn parser(&self) -> &Arc<Mutex<vt100::Parser>> {
        &self.parser
    }

    pub fn size(&self) -> (u16, u16) {
        self.size
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    /// Take the dirty flag, so the next write wakes the loop again.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// Resize both the PTY and the emulator.
    ///
    /// The `master.resize()` call is what issues `TIOCSWINSZ`, and that is what
    /// delivers SIGWINCH to the child — it is the single reason mc and GNU
    /// Screen re-lay-out correctly. Keeping the emulator in step matters just
    /// as much: a parser that disagrees with the child wraps every line wrong.
    pub fn resize(&mut self, size: (u16, u16)) -> Result<()> {
        let (rows, cols) = sane_size(size);
        if (rows, cols) == self.size {
            return Ok(());
        }
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("resize pty")?;
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_size(rows, cols);
        }
        self.size = (rows, cols);
        self.mark_dirty();
        Ok(())
    }

    /// Move this pane's view `lines` back through its history, or toward the
    /// live screen when negative — unless the program owns the wheel; see
    /// `scrollback::scroll`.
    pub fn scroll_history(&self, lines: isize) {
        if let Ok(mut p) = self.parser.lock() {
            super::scrollback::scroll(p.screen_mut(), lines);
        }
    }

    /// Back to the live screen.
    pub fn scroll_to_live(&self) {
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_scrollback(0);
        }
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() || self.has_exited() {
            return Ok(());
        }
        self.writer.write_all(bytes).context("write to pty")?;
        self.writer.flush().context("flush pty")?;
        Ok(())
    }

    /// Terminate the child. Called when a tab closes or the app exits.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if !self.has_exited() {
            self.kill();
        }
    }
}

/// A PTY with a zero dimension is invalid and confuses every curses program, so
/// clamp to at least 1x1 regardless of how small the layout gets.
fn sane_size((rows, cols): (u16, u16)) -> (u16, u16) {
    (rows.max(1), cols.max(1))
}

/// Reader thread: feed bytes into the emulator and wake the UI.
fn pump(
    id: SessionId,
    mut reader: Box<dyn std::io::Read + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    dirty: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    tx: Sender<TermEvent>,
) {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(mut p) = parser.lock() {
                    p.process(&buf[..n]);
                }
                // Only wake the loop on a clean->dirty edge.
                if !dirty.swap(true, Ordering::AcqRel) && tx.send(TermEvent::Output(id)).is_err() {
                    return;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    exited.store(true, Ordering::Release);
    dirty.store(true, Ordering::Release);
    let _ = tx.send(TermEvent::Exit(id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Block until `f` holds, or the deadline passes.
    fn wait_for(rx: &std::sync::mpsc::Receiver<TermEvent>, mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            let _ = rx.recv_timeout(Duration::from_millis(100));
        }
        f()
    }

    fn contents(s: &TerminalSession) -> String {
        s.parser().lock().unwrap().screen().contents()
    }

    #[test]
    fn runs_a_command_and_captures_its_output() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut spawn = Spawn::new("/bin/sh");
        spawn.args = vec!["-c".into(), "echo hello-openadmin; sleep 30".into()];
        let s =
            TerminalSession::spawn(1, "test", &spawn, (24, 80), 100, "xterm-256color", tx).unwrap();

        assert!(
            wait_for(&rx, || contents(&s).contains("hello-openadmin")),
            "never saw the output; screen was:\n{}",
            contents(&s)
        );
        assert_eq!(s.size(), (24, 80));
    }

    #[test]
    fn input_reaches_the_child() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut spawn = Spawn::new("/bin/sh");
        spawn.args = vec!["-c".into(), "read line; echo GOT:$line; sleep 30".into()];
        let mut s =
            TerminalSession::spawn(2, "test", &spawn, (24, 80), 100, "xterm-256color", tx).unwrap();

        s.write(b"ping\r").unwrap();
        assert!(
            wait_for(&rx, || contents(&s).contains("GOT:ping")),
            "screen was:\n{}",
            contents(&s)
        );
    }

    /// The resize path is the highest-risk code in the app: the child must
    /// actually observe the new geometry, not just the emulator.
    #[test]
    fn resize_reaches_the_child_process() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut spawn = Spawn::new("/bin/sh");
        // Report the terminal width whenever SIGWINCH arrives.
        spawn.args = vec![
            "-c".into(),
            "trap 'stty size' WINCH; stty size; while :; do sleep 0.2; done".into(),
        ];
        let mut s =
            TerminalSession::spawn(3, "test", &spawn, (24, 80), 100, "xterm-256color", tx).unwrap();

        assert!(
            wait_for(&rx, || contents(&s).contains("24 80")),
            "initial size not reported"
        );

        s.resize((30, 100)).unwrap();
        assert_eq!(s.size(), (30, 100));
        assert_eq!(s.parser().lock().unwrap().screen().size(), (30, 100));
        assert!(
            wait_for(&rx, || contents(&s).contains("30 100")),
            "child never saw SIGWINCH; screen was:\n{}",
            contents(&s)
        );
    }

    #[test]
    fn exit_is_reported_once_the_child_is_gone() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut spawn = Spawn::new("/bin/sh");
        spawn.args = vec!["-c".into(), "exit 0".into()];
        let s =
            TerminalSession::spawn(4, "test", &spawn, (24, 80), 100, "xterm-256color", tx).unwrap();

        assert!(wait_for(&rx, || s.has_exited()), "exit was never observed");
        // Writing to a dead session is a no-op, not an error.
        let mut s = s;
        assert!(s.write(b"ignored").is_ok());
    }

    #[test]
    fn a_degenerate_layout_still_yields_a_valid_pty() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut spawn = Spawn::new("/bin/sh");
        spawn.args = vec!["-c".into(), "sleep 5".into()];
        let s =
            TerminalSession::spawn(5, "test", &spawn, (0, 0), 10, "xterm-256color", tx).unwrap();
        assert_eq!(s.size(), (1, 1), "zero dimensions must be clamped");
    }
}
