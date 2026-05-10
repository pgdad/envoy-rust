//! envoy-stats typed-error enum.

#[derive(Debug, thiserror::Error)]
pub enum StatsError {
    #[error("stat '{name}' is already registered with a different kind (expected {expected}, got {got})")]
    ConflictingKind {
        name: String,
        expected: &'static str,
        got: &'static str,
    },

    #[error("stat name '{name}' is invalid: {reason}")]
    InvalidName {
        name: String,
        reason: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_format_to_diagnostic_strings() {
        let e1 = StatsError::ConflictingKind {
            name: "foo".to_string(),
            expected: "counter",
            got: "gauge",
        };
        assert_eq!(
            format!("{e1}"),
            "stat 'foo' is already registered with a different kind (expected counter, got gauge)"
        );

        let e2 = StatsError::InvalidName {
            name: "bad name".to_string(),
            reason: "contains whitespace",
        };
        assert_eq!(
            format!("{e2}"),
            "stat name 'bad name' is invalid: contains whitespace"
        );
    }
}
