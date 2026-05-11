//! FileSink — concrete on-disk access-log sink.
//!
//! Opens the configured path with `OpenOptions::append(true).
//! create(true)` at constructor time; serializes per-emission
//! writes via an internal `Arc<tokio::sync::Mutex<File>>` so
//! concurrent emissions on the same `Arc<FileSink>` interleave at
//! the mutex boundary (not at the OS-level write boundary, which
//! would allow line interleaving on filesystems with weaker
//! append atomicity).
//!
//! The `Sink` trait is intentionally NOT shipped per parent-06
//! SPEC §3 D8.2 option (c); FileSink ships concretely.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::default_format::format;
use crate::error::AccessLogError;
use crate::record::AccessLogRecord;

/// FileSink — concrete on-disk access-log sink.
///
/// Owns an `Arc<tokio::sync::Mutex<File>>` so concurrent emissions
/// on the same `Arc<FileSink>` serialize at the mutex boundary
/// rather than racing at the kernel append boundary. The path is
/// retained for error reporting via `AccessLogError::Write`.
#[derive(Debug)]
pub struct FileSink {
    path: PathBuf,
    handle: Arc<Mutex<File>>,
}

impl FileSink {
    /// Open (or create + truncate-disabled) the file at `path` in
    /// append mode. Returns `AccessLogError::Open` on filesystem
    /// failure (permissions, parent-directory-missing, path is a
    /// directory, etc.). Per 06.2 SPEC §6 signpost 6 + signpost 7,
    /// the constructor does NOT mkdir -p, does NOT pre-validate
    /// path shape, and does NOT truncate existing files.
    pub async fn new(path: PathBuf) -> Result<Self, AccessLogError> {
        let file = tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .await
            .map_err(|source| AccessLogError::Open {
                path: path.clone(),
                source,
            })?;
        Ok(Self {
            path,
            handle: Arc::new(Mutex::new(file)),
        })
    }

    /// Test-only constructor wrapping a pre-opened `tokio::fs::File`.
    /// Used by `envoy-http1`'s HCM tests to inject a deliberately
    /// write-failing handle (e.g., a read-only file) so the
    /// fire-and-forget posture at the dispatch site can be verified
    /// in a platform-portable way (POSIX semantics keep an open FD
    /// writable after its parent directory is unlinked, defeating
    /// the dir-drop-then-write-fails trick on macOS/Linux).
    ///
    /// Gated by `#[doc(hidden)]` + `#[cfg(any(test, feature = "test-util"))]`
    /// — production code uses `FileSink::new` exclusively.
    #[doc(hidden)]
    pub fn from_file_for_test(path: PathBuf, file: File) -> Self {
        Self {
            path,
            handle: Arc::new(Mutex::new(file)),
        }
    }

    /// Format `record` per the Envoy default format and append the
    /// result + a trailing `\n` to the underlying file. Returns
    /// `AccessLogError::Write` on filesystem failure. The HCM
    /// dispatch site at `envoy-http1::hcm` does NOT propagate this
    /// error — emission failures are logged via `tracing::warn!`
    /// and discarded per parent-06 SPEC §6 architectural Rule 4
    /// (fire-and-forget).
    ///
    /// Concurrent emissions on the same `Arc<FileSink>` serialize
    /// at the per-sink `Mutex<File>` — no two records will
    /// interleave in the file.
    pub async fn emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError> {
        let line = format(record);
        let mut file = self.handle.lock().await;
        file.write_all(line.as_bytes())
            .await
            .map_err(|source| AccessLogError::Write {
                path: self.path.clone(),
                source,
            })?;
        file.write_all(b"\n")
            .await
            .map_err(|source| AccessLogError::Write {
                path: self.path.clone(),
                source,
            })?;
        // No explicit flush — the kernel will flush on file close.
        // Tests drop the FileSink (and let the runtime finalize the
        // drop chain via the test-internal tokio::time::sleep) to
        // force flush before reading.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::tempdir;
    use tokio::io::AsyncReadExt;

    fn make_record() -> AccessLogRecord {
        AccessLogRecord {
            start_time: UNIX_EPOCH,
            method: "GET".into(),
            path: "/".into(),
            protocol: "HTTP/1.1".into(),
            response_code: 200,
            response_flags: "-".into(),
            bytes_received: 0,
            bytes_sent: 3,
            duration: Duration::from_millis(5),
            upstream_service_time: None,
            forwarded_for: None,
            user_agent: None,
            request_id: None,
            authority: Some("envoy-rust.test".into()),
            upstream_host: None,
        }
    }

    async fn read_to_string(path: &std::path::Path) -> String {
        let mut buf = String::new();
        File::open(path)
            .await
            .expect("file exists")
            .read_to_string(&mut buf)
            .await
            .expect("read");
        buf
    }

    #[tokio::test]
    async fn file_sink_writes_one_record() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = FileSink::new(path.clone()).await.expect("open");
        let record = make_record();
        sink.emit(&record).await.expect("emit");
        drop(sink); // force OS-level flush via file close
        let contents = read_to_string(&path).await;
        assert_eq!(
            contents.lines().count(),
            1,
            "expected 1 line, got {} (contents: {:?})",
            contents.lines().count(),
            contents
        );
        let line = &contents.lines().next().unwrap();
        // The formatter output for make_record() has a known suffix
        // after the [START_TIME] bracket (per default_format::tests::
        // format_happy_path_direct_response).
        assert!(
            line.ends_with(
                "\"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\""
            ),
            "line: {}",
            line
        );
        // The trailing newline must be present.
        assert!(
            contents.ends_with('\n'),
            "contents should end with newline; got: {:?}",
            contents
        );
    }

    #[tokio::test]
    async fn file_sink_appends_multiple_records() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = FileSink::new(path.clone()).await.expect("open");
        for i in 0..3 {
            let mut record = make_record();
            record.response_code = 200 + i;
            sink.emit(&record).await.expect("emit");
        }
        drop(sink);
        let contents = read_to_string(&path).await;
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        // Lines are in emit order.
        assert!(lines[0].contains(" 200 "), "line 0: {}", lines[0]);
        assert!(lines[1].contains(" 201 "), "line 1: {}", lines[1]);
        assert!(lines[2].contains(" 202 "), "line 2: {}", lines[2]);
    }

    #[tokio::test]
    async fn file_sink_serializes_concurrent_emissions() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let sink = Arc::new(FileSink::new(path.clone()).await.expect("open"));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let sink = Arc::clone(&sink);
            let record = make_record();
            handles.push(tokio::spawn(async move {
                sink.emit(&record).await.expect("emit");
            }));
        }
        for h in handles {
            h.await.expect("join");
        }
        // Drop our Arc so the inner FileSink can be dropped (and the
        // file flushed). We're the last Arc holder after the spawned
        // tasks completed and dropped their Arcs.
        drop(sink);
        // Small yield to let the runtime finalize the drop chain.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let contents = read_to_string(&path).await;
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(
            lines.len(),
            10,
            "expected 10 lines; got {} (contents bytes: {})",
            lines.len(),
            contents.len()
        );
        // Each line must be a complete formatter output (no
        // interleaving). The deterministic suffix is the ending of
        // make_record()'s output; every line must end with that
        // suffix (only the [START_TIME] prefix differs across lines
        // — they're all the same record, but each line is
        // independently rendered).
        for (i, line) in lines.iter().enumerate() {
            assert!(
                line.ends_with(
                    "\"GET / HTTP/1.1\" 200 - 0 3 5 - \"-\" \"-\" \"-\" \"envoy-rust.test\" \"-\""
                ),
                "line {} interleaved: {}",
                i,
                line
            );
        }
    }

    #[tokio::test]
    async fn file_sink_emit_returns_error_on_invalid_path() {
        // Attempt to open a sink at a path whose parent directory
        // does not exist. Per architecture decision 7 (signpost 6),
        // FileSink::new does NOT mkdir -p; the OS-level open() will
        // return ENOENT and FileSink::new maps to AccessLogError::Open.
        let path = PathBuf::from("/nonexistent-parent-directory-06-2-fixture/access.log");
        let err = FileSink::new(path.clone())
            .await
            .expect_err("expected open error");
        match err {
            AccessLogError::Open {
                path: got_path,
                source: _,
            } => {
                assert_eq!(got_path, path);
            }
            other => panic!("expected AccessLogError::Open; got {:?}", other),
        }
    }
}
