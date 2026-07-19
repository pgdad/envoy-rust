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

use crate::error::AccessLogError;
use crate::filter::LogFilter;
use crate::log_format::LogFormat;
use crate::record::AccessLogRecord;

/// FileSink — concrete on-disk access-log sink.
///
/// Owns an `Arc<tokio::sync::Mutex<File>>` so concurrent emissions
/// on the same `Arc<FileSink>` serialize at the mutex boundary
/// rather than racing at the kernel append boundary. The path is
/// retained for error reporting via `AccessLogError::Write`. The
/// `format` is the compiled access-log format the sink renders each
/// record through (the Envoy default via `CompiledFormat::default()`,
/// or a config-derived custom format). The `filter` is the optional
/// compiled per-record emission predicate (phase 70); `None` means the
/// sink logs every record.
#[derive(Debug)]
pub struct FileSink {
    path: PathBuf,
    handle: Arc<Mutex<File>>,
    format: LogFormat,
    filter: Option<LogFilter>,
}

impl FileSink {
    /// Open (or create + truncate-disabled) the file at `path` in
    /// append mode. Returns `AccessLogError::Open` on filesystem
    /// failure (permissions, parent-directory-missing, path is a
    /// directory, etc.). Per 06.2 SPEC §6 signpost 6 + signpost 7,
    /// the constructor does NOT mkdir -p, does NOT pre-validate
    /// path shape, and does NOT truncate existing files.
    pub async fn new(
        path: PathBuf,
        format: impl Into<LogFormat>,
        filter: Option<LogFilter>,
    ) -> Result<Self, AccessLogError> {
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
            format: format.into(),
            filter,
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
    #[cfg(any(test, feature = "test-util"))]
    pub fn from_file_for_test(
        path: PathBuf,
        file: File,
        format: impl Into<LogFormat>,
        filter: Option<LogFilter>,
    ) -> Self {
        Self {
            path,
            handle: Arc::new(Mutex::new(file)),
            format: format.into(),
            filter,
        }
    }

    /// Phase 70/71: returns `true` iff a record with final response `status`
    /// and `response_flags` token should be emitted to this sink. A sink with
    /// no filter always logs.
    pub fn should_log(&self, status: u16, response_flags: &str) -> bool {
        match &self.filter {
            Some(f) => f.should_log(status, response_flags),
            None => true,
        }
    }

    /// Render `record` through the sink's compiled `format` and append
    /// the result VERBATIM to the underlying file. The sink does NOT
    /// append a trailing `\n` of its own — the newline (if any) is part
    /// of the format string itself (the Envoy default carries a trailing
    /// `\n`; a custom format controls its own line terminator). Returns
    /// `AccessLogError::Write` on filesystem failure. The HCM dispatch
    /// site at `envoy-http1::hcm` does NOT propagate this error —
    /// emission failures are logged via `tracing::warn!` and discarded
    /// per parent-06 SPEC §6 architectural Rule 4 (fire-and-forget).
    ///
    /// Concurrent emissions on the same `Arc<FileSink>` serialize
    /// at the per-sink `Mutex<File>` — no two records will
    /// interleave in the file.
    pub async fn emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError> {
        let line = self.format.render(record);
        let mut file = self.handle.lock().await;
        file.write_all(line.as_bytes())
            .await
            .map_err(|source| AccessLogError::Write {
                path: self.path.clone(),
                source,
            })?;
        // Flush the single write. `tokio::fs::File` buffers writes on a
        // blocking-pool thread and can return `Ok` from the first
        // `write_all` even when the underlying `write(2)` will fail
        // (e.g. an `O_RDONLY` FD); the OS error then only surfaces on a
        // later op. When `emit` did TWO writes (line + `\n`) the second
        // write forced that surfacing; now that the format string carries
        // its own newline there is a SINGLE write, so we `flush()` here to
        // observe the write error in the same `emit` call (preserving the
        // fire-and-forget error-reporting contract at the HCM dispatch
        // site). This is a buffer flush, not an fsync — durability still
        // rides on file close.
        file.flush().await.map_err(|source| AccessLogError::Write {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_operator::CompiledFormat;
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
            upstream_cluster: None,
            route_name: None,
            response_code_details: None,
            dynamic_metadata: std::collections::BTreeMap::new(),
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
        let sink = FileSink::new(path.clone(), CompiledFormat::default(), None)
            .await
            .expect("open");
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
        let sink = FileSink::new(path.clone(), CompiledFormat::default(), None)
            .await
            .expect("open");
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
        let sink = Arc::new(
            FileSink::new(path.clone(), CompiledFormat::default(), None)
                .await
                .expect("open"),
        );
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
        let err = FileSink::new(path.clone(), CompiledFormat::default(), None)
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

    #[tokio::test]
    async fn file_sink_emits_json_object() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("access.log");
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "status".to_string(),
            crate::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        let fmt = crate::CompiledJsonFormat::from_map(&map).unwrap();
        let sink = FileSink::new(path.clone(), fmt, None).await.unwrap(); // CompiledJsonFormat: Into<LogFormat>
        sink.emit(&make_record()).await.unwrap();
        drop(sink);
        assert_eq!(read_to_string(&path).await, "{\"status\":200}\n");
    }

    #[tokio::test]
    async fn should_log_gates_on_filter() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("al.log");
        let filter = Some(crate::LogFilter::StatusCode(crate::StatusCodeComparison {
            op: crate::FilterOp::Ge,
            threshold: 500,
        }));
        let sink = FileSink::new(path, CompiledFormat::default(), filter)
            .await
            .expect("open");
        assert!(!sink.should_log(200, "-"));
        assert!(sink.should_log(503, "-"));

        // A sink with no filter logs everything.
        let dir2 = tempdir().expect("tempdir");
        let sink2 = FileSink::new(dir2.path().join("al2.log"), CompiledFormat::default(), None)
            .await
            .expect("open");
        assert!(sink2.should_log(200, "-"));
        assert!(sink2.should_log(503, "NR"));
    }

    #[tokio::test]
    async fn file_sink_writes_custom_format_verbatim() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("access.log");
        let fmt = CompiledFormat::from_inline("%RESPONSE_CODE%").expect("valid");
        let sink = FileSink::new(path.clone(), fmt, None).await.expect("open");
        sink.emit(&make_record()).await.expect("emit");
        drop(sink);
        let contents = read_to_string(&path).await;
        assert_eq!(contents, "200"); // verbatim, NO trailing newline
    }
}
