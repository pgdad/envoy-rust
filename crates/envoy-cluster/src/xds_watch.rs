//! Phase 27 Task 3 (D2, ADR-0067): `XdsFileWatcher` — the domain-free,
//! poll-based file-watch core extracted from phase 26's `RdsWatcher`.
//!
//! This is the "second instance reveals the abstraction" refactor: phase 26
//! built a poll/mtime/cancel loop for RDS route-table hot-reload; phase 27
//! needs a SECOND watcher (for EDS endpoint files), so the domain-free loop is
//! lifted here and BOTH RDS (migrated in this task) and EDS (a later task)
//! consume it.
//!
//! The core mirrors the 12.2 `envoy_health::Scheduler` topology: it holds the
//! `JoinHandle`s of every spawned watch task plus the shared
//! `CancellationToken`; `shutdown(self)` cancels the token and awaits the
//! handles for a clean drain. It is DOMAIN-FREE: a [`WatchTarget`] carries only
//! a `path` to stat plus a boxed `reload` action invoked on a detected mtime
//! change. ALL domain knowledge (reparse/revalidate/store pipelines, counter
//! ticks, warm-reject logging) lives inside the caller's closure — so this
//! crate gains NO dependency on `envoy-http1`/`HCMConfig` (which would be a
//! dependency cycle, since `envoy-http1` already depends on `envoy-cluster`).
//!
//! §5.2 inertness: an empty target list yields zero watch tasks (the watcher is
//! constructed but inert), exactly mirroring how the health scheduler / outlier
//! manager spawn nothing when their feature is unconfigured.

use std::path::PathBuf;
use std::time::SystemTime;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// The file poll cadence. A `tokio::time::interval` ticks every `POLL_INTERVAL`;
/// on each tick the loop stats the file and compares its mtime against the
/// last-seen value. (Inherited from phase 26's `RdsWatcher::POLL_INTERVAL` —
/// 1s matches the other periodic primitives' "sensible default" cadence.)
pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// One domain-free watch target: a `path` to stat plus a `reload` action run on
/// a detected mtime change.
///
/// The `reload` closure owns ALL domain behaviour (reparse, revalidate, store
/// swap, counter ticks, warm-reject logging) and returns nothing — a reload
/// must never propagate an error that takes the proxy down; any failure is
/// handled (logged + counted) INSIDE the closure. The watcher fires it exactly
/// once per detected mtime change (see [`watch_loop`] for the one-tick
/// coalescing caveat carried forward from phase 26 / M26-3).
pub struct WatchTarget {
    /// The file to stat (the domain caller re-reads it inside `reload`).
    pub path: PathBuf,
    /// The action to run on a detected mtime change. Boxed `FnMut` (the
    /// lower-ceremony option vs a bespoke trait); `Send` so it can cross the
    /// `tokio::spawn` boundary into the watch task.
    pub reload: Box<dyn FnMut() + Send>,
}

impl std::fmt::Debug for WatchTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchTarget")
            .field("path", &self.path)
            .field("reload", &"<closure>")
            .finish()
    }
}

/// The domain-free file watcher. Holds the `JoinHandle`s of every spawned
/// `watch_loop` and the shared `CancellationToken`. Drop without `shutdown()`
/// is safe — the tasks observe the runtime shutdown via the token — but
/// `shutdown().await` is preferred for a clean drain (mirrors the 12.2
/// `Scheduler`).
#[derive(Debug)]
pub struct XdsFileWatcher {
    handles: Vec<JoinHandle<()>>,
    cancel: CancellationToken,
}

impl XdsFileWatcher {
    /// Spawn one `watch_loop` per target. Returns an `XdsFileWatcher` holding
    /// the task handles. `cancel` is the shared shutdown token — a caller
    /// cancelling it (via the envoy-bin signal token) or calling `shutdown()`
    /// terminates every loop at its next `tokio::select!` boundary.
    ///
    /// Spawn is INFALLIBLE (`-> Self`): the loop performs no fallible work at
    /// spawn time (the target's `reload` closure already owns any fallible work
    /// and handles its own errors).
    pub fn spawn(targets: Vec<WatchTarget>, cancel: CancellationToken) -> Self {
        let mut handles = Vec::with_capacity(targets.len());
        for target in targets {
            let cancel = cancel.clone();
            let handle = tokio::spawn(async move {
                watch_loop(target, cancel).await;
            });
            handles.push(handle);
        }
        XdsFileWatcher { handles, cancel }
    }

    /// Cancel every watch task and await their `JoinHandle`s. Returns once every
    /// loop has exited at its next `tokio::select!` boundary (mirrors the 12.2
    /// `Scheduler::shutdown`).
    pub async fn shutdown(self) {
        self.cancel.cancel();
        for handle in self.handles {
            let _ = handle.await;
        }
    }

    /// Count of spawned watch tasks (mirrors the 12.2 `Scheduler::task_count`).
    /// Zero when the target list is empty (the §5.2 inertness witness).
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }
}

/// The per-target poll loop. `tokio::select!`s between `cancel.cancelled()` and
/// a `tokio::time::interval` tick. On each tick it stats `target.path` and
/// compares the mtime against the last-seen value; on a change it invokes
/// `(target.reload)()`. The cancel branch exits the loop promptly (it does not
/// wait for the next tick — the §5.x clean-drain discipline shared with the
/// 12.2 `probe_loop`).
///
/// M26-3 caveat (carried forward): mtime has one-second resolution on many
/// filesystems, so two edits within the same `read_mtime` tick coalesce into a
/// single observed change → a single `reload` call. This is acceptable for a
/// poll-based watcher (the LATEST file contents are read on the next observed
/// change anyway).
async fn watch_loop(mut target: WatchTarget, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    // Burn the immediate first tick that `tokio::time::interval` fires at t=0
    // so the first mtime poll happens one `POLL_INTERVAL` after spawn (parity
    // with the probe_loop / sweeper cadence — they observe a real interval
    // before their first action).
    interval.tick().await;

    // Seed the last-seen mtime from the file as it stands at spawn time. A
    // missing/unreadable file seeds `None`; the first successful stat that
    // yields a different value (including: file appears) counts as a change.
    let mut last_mtime: Option<SystemTime> = read_mtime(&target.path);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                // Clean exit on shutdown — do not wait for the next tick.
                break;
            }
            _ = interval.tick() => {
                let current = read_mtime(&target.path);
                // Only a CHANGED, present mtime triggers a reload. A stable
                // mtime (the 0028 idle witness) or a vanished file is a no-op.
                if let Some(now) = current
                    && last_mtime != Some(now)
                {
                    last_mtime = Some(now);
                    // The domain closure owns reparse/revalidate/store + its own
                    // warm-reject logging + counter ticks; it returns nothing.
                    // The loop never propagates a reload error — a bad config
                    // file must NOT take the proxy down.
                    (target.reload)();
                }
            }
        }
    }
}

/// Stat `path` and return its mtime, or `None` if the file is
/// missing/unreadable/exposes no mtime. The loop compares this against the
/// last-seen value to detect a change.
fn read_mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Lifecycle: spawn over a target whose file mtime never changes → the
    /// watcher idles (the §5.2 / 0028 inertness witness), fires NO reload, and
    /// `shutdown()` terminates the loop promptly. Driven on a paused clock so
    /// the poll interval advances deterministically without real sleeps.
    #[tokio::test(start_paused = true)]
    async fn idles_when_mtime_stable_fires_no_reload_and_shuts_down() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("watched.yaml");
        std::fs::write(&path, "resources: []\n").expect("write file");

        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        let target = WatchTarget {
            path,
            reload: Box::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        };
        let cancel = CancellationToken::new();
        let watcher = XdsFileWatcher::spawn(vec![target], cancel.clone());
        assert_eq!(watcher.task_count(), 1, "one watch task per target");

        // Advance several poll intervals WITHOUT touching the file.
        for _ in 0..5 {
            tokio::time::advance(POLL_INTERVAL).await;
        }

        let drained = tokio::time::timeout(Duration::from_secs(3), watcher.shutdown()).await;
        assert!(drained.is_ok(), "shutdown returned promptly on cancel");
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "stable mtime fires no reload"
        );
    }

    /// Lifecycle: an empty target list spawns ZERO tasks (the §5.2 inertness
    /// invariant) and `shutdown()` is a clean no-op.
    #[tokio::test(start_paused = true)]
    async fn empty_targets_spawn_no_tasks() {
        let cancel = CancellationToken::new();
        let watcher = XdsFileWatcher::spawn(vec![], cancel);
        assert_eq!(watcher.task_count(), 0, "no watch task when no target");
        watcher.shutdown().await;
    }

    /// Lifecycle: `cancel.cancel()` (rather than `shutdown()`) also terminates
    /// the loop — the watcher observes the shared token, mirroring the
    /// envoy-bin signal-token wiring.
    #[tokio::test(start_paused = true)]
    async fn cancel_token_terminates_loop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("watched.yaml");
        std::fs::write(&path, "resources: []\n").expect("write file");

        let target = WatchTarget {
            path,
            reload: Box::new(|| {}),
        };
        let cancel = CancellationToken::new();
        let watcher = XdsFileWatcher::spawn(vec![target], cancel.clone());
        tokio::time::advance(POLL_INTERVAL).await;

        cancel.cancel();
        let drained = tokio::time::timeout(Duration::from_secs(3), watcher.shutdown()).await;
        assert!(drained.is_ok(), "loop exits on external cancel");
    }

    /// An atomic-rename-detected mtime change invokes the target's reload
    /// closure exactly once. Driven on a paused clock; the closure increments an
    /// `Arc<AtomicUsize>` we assert on.
    #[tokio::test(start_paused = true)]
    async fn mtime_change_fires_reload_exactly_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("watched.yaml");
        std::fs::write(&path, "resources: []\n").expect("write file");

        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        let target = WatchTarget {
            path: path.clone(),
            reload: Box::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        };
        let cancel = CancellationToken::new();
        let watcher = XdsFileWatcher::spawn(vec![target], cancel.clone());

        // Let the spawned task run up to its first `interval.tick().await`
        // (the burned t=0 tick) and seed `last_mtime` from the file as it
        // stands now, BEFORE we mutate it.
        tokio::task::yield_now().await;

        // One interval with no change → no reload.
        tokio::time::advance(POLL_INTERVAL).await;
        tokio::task::yield_now().await;
        assert_eq!(count.load(Ordering::SeqCst), 0, "no change yet");

        // Atomic-rename a new file into place with a strictly-later mtime, then
        // let the watcher observe the change. We bump mtime explicitly so the
        // change is detected regardless of fs mtime resolution (and regardless
        // of the paused test clock, which would otherwise leave mtime stable).
        let tmp = dir.path().join("watched.yaml.tmp");
        std::fs::write(&tmp, "resources: [changed]\n").expect("write tmp");
        let later = std::time::SystemTime::now() + Duration::from_secs(10);
        filetime::set_file_mtime(&tmp, filetime::FileTime::from_system_time(later))
            .expect("set mtime");
        std::fs::rename(&tmp, &path).expect("atomic rename");

        // Advance one interval so the poll observes the changed mtime; yield so
        // the closure runs.
        tokio::time::advance(POLL_INTERVAL).await;
        tokio::task::yield_now().await;
        // A couple more intervals: the SAME (stable) mtime must NOT re-fire.
        tokio::time::advance(POLL_INTERVAL).await;
        tokio::task::yield_now().await;
        tokio::time::advance(POLL_INTERVAL).await;
        tokio::task::yield_now().await;

        let drained = tokio::time::timeout(Duration::from_secs(3), watcher.shutdown()).await;
        assert!(drained.is_ok(), "shutdown drained");
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "exactly one reload on a single mtime change"
        );
    }
}
