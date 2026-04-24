use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum ArgvError {
    NoConfigFlag,
    UnknownFlag(String),
    MissingValue(String),
    Trailing(String),
}

impl std::fmt::Display for ArgvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoConfigFlag => write!(
                f,
                "expected exactly one of `-c <path>` or `--config-path <path>`",
            ),
            Self::UnknownFlag(flag) => write!(f, "unknown argument: {flag}"),
            Self::MissingValue(flag) => write!(f, "{flag} requires a path argument"),
            Self::Trailing(arg) => write!(f, "unexpected trailing argument: {arg}"),
        }
    }
}

impl std::error::Error for ArgvError {}

/// Phase 01 accepts exactly one flag: `-c <path>` or `--config-path <path>`.
/// `clap` is deliberately avoided (not on the D-3.2 permitted-foundations list).
/// When argv grows past a single path, land an ADR and revisit.
pub fn parse_argv<I, S>(args: I) -> Result<PathBuf, ArgvError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut iter = args.into_iter().map(Into::into);
    let _ = iter.next();
    let mut path: Option<PathBuf> = None;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-c" | "--config-path" => {
                let value = iter.next().ok_or(ArgvError::MissingValue(arg.clone()))?;
                if path.is_some() {
                    return Err(ArgvError::Trailing(value));
                }
                path = Some(PathBuf::from(value));
            }
            other => return Err(ArgvError::UnknownFlag(other.to_string())),
        }
    }
    path.ok_or(ArgvError::NoConfigFlag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("envoy-bin")
            .chain(args.iter().copied())
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn accepts_short_flag() {
        let p = parse_argv(argv(&["-c", "/etc/envoy-rust.yaml"])).unwrap();
        assert_eq!(p, PathBuf::from("/etc/envoy-rust.yaml"));
    }

    #[test]
    fn accepts_long_flag() {
        let p = parse_argv(argv(&["--config-path", "/tmp/e.yaml"])).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/e.yaml"));
    }

    #[test]
    fn rejects_missing_flag() {
        assert_eq!(parse_argv(argv(&[])), Err(ArgvError::NoConfigFlag));
    }

    #[test]
    fn rejects_missing_value() {
        assert_eq!(
            parse_argv(argv(&["-c"])),
            Err(ArgvError::MissingValue("-c".into())),
        );
    }

    #[test]
    fn rejects_unknown_flag() {
        assert_eq!(
            parse_argv(argv(&["--foo", "bar"])),
            Err(ArgvError::UnknownFlag("--foo".into())),
        );
    }

    #[test]
    fn rejects_duplicate_config_flag() {
        let err = parse_argv(argv(&["-c", "/a", "-c", "/b"])).unwrap_err();
        assert!(matches!(err, ArgvError::Trailing(_)), "got {err:?}");
    }
}
