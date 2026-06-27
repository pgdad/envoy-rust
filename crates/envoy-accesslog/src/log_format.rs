//! LogFormat — the sink-level access-log format: the EXISTING text/default
//! `CompiledFormat` or the phase-38 `CompiledJsonFormat` (ADR-0092). `FileSink`
//! holds one of these and renders each record through it verbatim. The text arm
//! is byte-frozen — the JSON arm is a strict sibling.
use crate::command_operator::CompiledFormat;
use crate::json_format::CompiledJsonFormat;
use crate::record::AccessLogRecord;

/// The access-log format a `FileSink` renders each record through: the existing
/// text/default `CompiledFormat` (`Text`) or the phase-38 `CompiledJsonFormat`
/// (`Json`). Existing call sites pass a `CompiledFormat` and coerce via `Into`.
#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat {
    Text(CompiledFormat),
    Json(CompiledJsonFormat),
}

impl LogFormat {
    /// Render `record` through the held format into one owned line/object. The
    /// text arm is byte-identical to `CompiledFormat::render`; the JSON arm
    /// emits one sorted JSON object + trailing `\n` (ADR-0092).
    pub fn render(&self, record: &AccessLogRecord) -> String {
        match self {
            LogFormat::Text(f) => f.render(record),
            LogFormat::Json(f) => f.render(record),
        }
    }
}

impl From<CompiledFormat> for LogFormat {
    fn from(f: CompiledFormat) -> Self {
        LogFormat::Text(f)
    }
}

impl From<CompiledJsonFormat> for LogFormat {
    fn from(f: CompiledJsonFormat) -> Self {
        LogFormat::Json(f)
    }
}
