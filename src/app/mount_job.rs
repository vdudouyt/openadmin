//! Mounting in the background, one host at a time, cancellably.
//!
//! sshfs used to run on the UI thread. The screen froze for as long as it took
//! to connect, and an unreachable host takes as long as the kernel's TCP timeout
//! — minutes, with no redraw and nothing the operator could press. The job runs
//! the mounts on a thread instead and keeps its progress where the event loop
//! can read it on every tick, the same way the loop already polls terminal
//! output rather than being told about it.
//!
//! The mount itself is passed in. The app hands over `mount::mount`; tests hand
//! over a function that blocks until cancelled, so a dialog, a Cancel and a
//! failure can all be exercised without sshfs or a network.

use crate::db::model::HostRecord;
use crate::mount::Outcome;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

/// How the job ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finish {
    /// Every host mounted.
    All,
    /// Stopped by the operator. Hosts mounted before the cancel stay mounted.
    Cancelled,
    /// A host failed; the ones after it were not attempted. Holds the error.
    Failed(String),
}

/// What the dialog draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// Hosts mounted so far.
    pub done: usize,
    /// Index of the host being mounted now.
    pub current: usize,
    pub finish: Option<Finish>,
}

pub struct MountJob {
    /// `(name, mount point)` for each host, in the order they are mounted.
    pub hosts: Vec<(String, String)>,
    pub started: Instant,
    progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MountJob {
    /// Start mounting `targets` with `mount_one` on a background thread.
    pub fn start<F>(targets: Vec<HostRecord>, mount_one: F) -> Self
    where
        F: Fn(&HostRecord, &AtomicBool) -> Result<Outcome> + Send + 'static,
    {
        let hosts = targets
            .iter()
            .map(|h| (h.name.clone(), h.mount_point.clone()))
            .collect();
        let progress = Arc::new(Mutex::new(Progress {
            done: 0,
            current: 0,
            finish: None,
        }));
        let cancel = Arc::new(AtomicBool::new(false));

        let (p, c) = (Arc::clone(&progress), Arc::clone(&cancel));
        let handle = std::thread::Builder::new()
            .name("openadmin-mount".into())
            .spawn(move || {
                let set = |f: &dyn Fn(&mut Progress)| {
                    let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
                    f(&mut g);
                };
                for (i, host) in targets.iter().enumerate() {
                    // Checked before each host too, so a cancel that lands
                    // between two of them does not start the next.
                    if c.load(Ordering::Acquire) {
                        set(&|g| g.finish = Some(Finish::Cancelled));
                        return;
                    }
                    set(&|g| g.current = i);
                    match mount_one(host, &c) {
                        Ok(Outcome::Mounted) => set(&|g| g.done += 1),
                        Ok(Outcome::Cancelled) => {
                            set(&|g| g.finish = Some(Finish::Cancelled));
                            return;
                        }
                        Err(e) => {
                            let msg = format!("{e:#}");
                            set(&|g| g.finish = Some(Finish::Failed(msg.clone())));
                            return;
                        }
                    }
                }
                set(&|g| g.finish = Some(Finish::All));
            })
            .expect("spawn mount thread");

        MountJob {
            hosts,
            started: Instant::now(),
            progress,
            cancel,
            handle: Some(handle),
        }
    }

    pub fn progress(&self) -> Progress {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Ask the job to stop. It stops when the host in flight has been killed,
    /// which is why the dialog stays up, saying so, until `finish` is set.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelling(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    /// Wait for the thread. Only called once `finish` is set, or on quit after
    /// a cancel, so it never waits on a connection.
    pub fn join(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// The progress bar, as `(filled, cell)` pairs for the renderer to style.
///
/// One segment per host. A mounted host's segment is solid and a pending one is
/// empty. The one in flight has a short block sweeping across it: sshfs reports
/// nothing while it connects, so there is no fraction to show, and a bar that
/// sat still would read as hung. With one host that is the whole bar, which is
/// what an indeterminate bar is. `tick` moves the block.
pub fn bar(width: usize, done: usize, total: usize, tick: usize) -> Vec<bool> {
    const SWEEP: usize = 3;
    if width == 0 || total == 0 {
        return Vec::new();
    }
    let seg = |s: usize| s * width / total; // start column of segment `s`
    let mut cells = vec![false; width];
    for (x, cell) in cells.iter_mut().enumerate() {
        // Which segment this column belongs to.
        let s = (x * total / width).min(total - 1);
        if s < done {
            *cell = true;
        } else if s == done {
            let (lo, hi) = (seg(s), seg(s + 1));
            let span = hi - lo;
            // The block enters from the left edge and leaves off the right.
            let pos = tick % (span + SWEEP);
            let off = x - lo;
            // Lit for the SWEEP columns just behind `pos`.
            *cell = off < pos && off + SWEEP >= pos;
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn host(name: &str) -> HostRecord {
        HostRecord {
            name: name.into(),
            mount_point: format!("/net/{name}"),
            ..HostRecord::default()
        }
    }

    fn wait_finish(job: &MountJob) -> Finish {
        for _ in 0..500 {
            if let Some(f) = job.progress().finish {
                return f;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the job never finished: {:?}", job.progress());
    }

    #[test]
    fn every_host_is_mounted_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&seen);
        let mut job = MountJob::start(vec![host("a"), host("b"), host("c")], move |h, _| {
            s.lock().unwrap().push(h.name.clone());
            Ok(Outcome::Mounted)
        });
        assert_eq!(wait_finish(&job), Finish::All);
        job.join();
        assert_eq!(job.progress().done, 3);
        assert_eq!(*seen.lock().unwrap(), vec!["a", "b", "c"]);
    }

    /// A cancel reaches the host in flight, and nothing after it starts.
    #[test]
    fn a_cancel_stops_the_host_in_flight_and_starts_no_more() {
        let started = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&started);
        let mut job = MountJob::start(vec![host("a"), host("b"), host("c")], move |h, cancel| {
            s.lock().unwrap().push(h.name.clone());
            if h.name == "a" {
                return Ok(Outcome::Mounted);
            }
            // Connecting, until told otherwise.
            while !cancel.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(Outcome::Cancelled)
        });
        // Wait until b is in flight.
        for _ in 0..500 {
            if job.progress().current == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!job.is_cancelling());
        job.cancel();
        assert!(job.is_cancelling());
        assert_eq!(wait_finish(&job), Finish::Cancelled);
        job.join();
        assert_eq!(job.progress().done, 1, "a stays mounted");
        assert_eq!(*started.lock().unwrap(), vec!["a", "b"], "c never started");
    }

    #[test]
    fn a_failure_stops_the_job_and_keeps_the_message() {
        let mut job = MountJob::start(vec![host("a"), host("b"), host("c")], |h, _| {
            if h.name == "b" {
                anyhow::bail!("mounting b failed: read: Connection reset by peer");
            }
            Ok(Outcome::Mounted)
        });
        match wait_finish(&job) {
            Finish::Failed(m) => assert!(m.contains("Connection reset by peer"), "{m}"),
            other => panic!("{other:?}"),
        }
        job.join();
        assert_eq!(job.progress().done, 1);
    }

    #[test]
    fn the_bar_fills_finished_segments_and_sweeps_the_one_in_flight() {
        let render = |cells: &[bool]| -> String {
            cells.iter().map(|&c| if c { '█' } else { '░' }).collect()
        };
        // 3 hosts over 12 columns, one done: the first four solid.
        let b = bar(12, 1, 3, 0);
        assert_eq!(b.len(), 12);
        assert_eq!(&render(&b)[..4 * '█'.len_utf8()], "████");
        // Nothing lights up past the segment in flight.
        assert!(b[8..].iter().all(|c| !c), "{}", render(&b));
        // The block moves with the tick, and stays inside its segment.
        let lit = |t| -> Vec<usize> {
            bar(12, 1, 3, t)
                .iter()
                .enumerate()
                .filter(|(x, c)| **c && *x >= 4)
                .map(|(x, _)| x)
                .collect()
        };
        assert_ne!(lit(2), lit(4), "it sweeps");
        // Mid-segment the block is exactly SWEEP wide.
        assert_eq!(bar(40, 0, 1, 20).iter().filter(|c| **c).count(), 3);
        for t in 0..20 {
            assert!(
                lit(t).iter().all(|x| (4..8).contains(x)),
                "tick {t}: {:?}",
                lit(t)
            );
        }
        // All done: solid throughout.
        assert!(bar(12, 3, 3, 7).iter().all(|c| *c));
        // One host is one sweeping segment — an indeterminate bar.
        assert!(bar(20, 0, 1, 5).iter().any(|c| *c));
        assert!(!bar(20, 0, 1, 5).iter().all(|c| *c));
        // Degenerate sizes do not panic.
        assert!(bar(0, 0, 3, 0).is_empty());
        assert!(bar(10, 0, 0, 0).is_empty());
        assert_eq!(bar(2, 0, 5, 3).len(), 2);
    }
}
