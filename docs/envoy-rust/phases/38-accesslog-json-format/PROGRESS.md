# Phase 38 — `38-accesslog-json-format` — PROGRESS

> State-3 implementation log (`superpowers:executing-plans`, TDD per task per
> `superpowers:test-driven-development`). Ground truth: **ADR-0092** §A–§F
> (empirically locked vs live `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…0c2`).
> PLAN: `PLAN.md` (9 TDD tasks). Append-only; one entry per task on completion.

---

## Task 1 — `SubstitutionFormatString` `{text_format_source | json_format}` oneof — DONE

**TDD:** wrote 2 failing tests first in `bootstrap.rs` `#[cfg(test)]`
(`json_format_parses_into_sorted_btreemap`, `text_format_source_arm_still_parses`);
confirmed RED (`no field json_format on type SubstitutionFormatString` /
`method unwrap not found for DataSourceInline`).

**Implemented:** widened `SubstitutionFormatString` (`bootstrap.rs:697`) from
`{text_format_source: DataSourceInline}` to the oneof
`{text_format_source: Option<DataSourceInline>, json_format: Option<BTreeMap<String,String>>}`
(both `#[serde(default)]`; `deny_unknown_fields` retained; `BTreeMap` = the SORTED
config model, ADR-0092 §A — NO custom serde, NO new dep). Made every downstream reader
compile (behavior unchanged until Tasks 2/7):
- `validate_access_logs` (`:4362`) — temporary `if let Some(ds) = &fmt.text_format_source`
  guard (Task 2 replaces with exactly-one-of).
- `compiled_log_format` H1 (`hcm.rs:1257`) — temporary `match &s.text_format_source`
  (Task 7 returns `LogFormat`).
- struct-literal ctors: `envoy-http1/src/hcm.rs:1767`, `envoy-http2/src/hcm.rs:1350`,
  `:1460` each gained `text_format_source: Some(...), json_format: None`.
- assert reader `bootstrap.rs:~11009` → `.text_format_source.as_ref().unwrap()`.

**Evidence:** `cargo test -p envoy-config json_format_parses_into_sorted_btreemap` →
`1 passed`; `cargo test -p envoy-config text_format_source_arm_still_parses` →
`1 passed`. `cargo build --workspace --all-targets` → `Finished` (clean — the public
struct widening leaves a workspace-green tree).

**Commit:** `phase 38 task 1: SubstitutionFormatString {text_format_source|json_format} oneof (BTreeMap, sorted) [ADR-0092]`
