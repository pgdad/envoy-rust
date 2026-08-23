# Sub-phase 110.2 — §5 state-3 IMPLEMENTATION — PROGRESS

> Written for a reader with ZERO prior context (D-3.4). This file records what
> was **BUILT and RUN**, not what `PLAN.md`'s tables promised — the `110.1`
> REVIEW finding M-3 lesson: a PLAN's coverage table and its test list are TWO
> separate claims, and `110.1/PROGRESS.md` inherited the table's language.
> Every command output quoted here was produced by this session.

**Entry state, re-verified on disk rather than taken from the handoff.**
`git status --porcelain` clean, branch `main` at
`0b6f2f63824cc109b5c4ce40db335c8e36363280`, `git fetch origin --prune` exit `0`,
`origin/main` identical to `HEAD`. The phase directory held **`SPEC.md` +
`PLAN.md` ONLY** — no `PROGRESS.md`, no `REVIEW.md` — which IS the §5 state-3
detection rule. `ROADMAP.md` census re-derived **116 rows / 114 `done` /
1 `in-progress` / 1 `planned`**, the not-done set exactly
`[('110','in-progress'), ('110.2','planned')]`. `STATE.md` and `ROADMAP.md`
AGREE, so no `superpowers:systematic-debugging` detour was needed.
**Skill: `superpowers:executing-plans`**, TDD per
`superpowers:test-driven-development` on every task.

**X-item preconditions re-confirmed FRESH before any fixture ran** (each was a
CLAIM inherited from the state-2, not a fact):

| item | re-derived this session |
|---|---|
| **X-8** DEBUG `envoy-bin` rebuilt FIRST | `cargo build -p envoy-bin` exit **0** before any probe. A stale binary fails with `unknown field` errors that look like real divergences. |
| **X-1** pinned image digest verified BEFORE probing | `docker image inspect envoyproxy/envoy:v1.33.0 --format '{{index .RepoDigests 0}}'` → `envoyproxy/envoy@sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, **matching `ENVOY_TARGET.md` exactly**. |
| **X-4** fixture census | `git ls-files 'tests/fixtures/**' \| cut -d/ -f3 \| sort -u \| wc -l` = **88**, highest `0088-runtime-fraction-route-gating`, `ls -d tests/fixtures/0089*` → `No such file or directory`; **88** differential test files. |
| **X-5** the four harness facts | `HEADER_ALLOW_LIST` is **3 entries** (`server`, `date`, `x-envoy-upstream-service-time`) with `location` count **0**; `Http1BodyRule::ByteExact { body: String }` the ONLY variant; `Http1Method` exactly `Get`/`Options`/`Post`; `drive_http1` interpolates `extra_headers` **RAW** (`req.push_str(&format!("{n}: {v}\r\n"))`, no lower-casing, no validation) and emits `Host:`/`Connection: close` itself. |
| driver gating | `driver_needs_admin_port` matches only `AdminScrape`/`Http1KeepAlive`/`Http2KeepAlive`/`TcpWithStats` — `Http1ProbeList` is NOT among them, so `admin.port_value` is a LITERAL `0`. `{{PORT}}` IS substituted for `Http1ProbeList`. |

---

## Task 1 — the fixture skeleton, the 11 mapping cells and the 4 controls — **COMPLETE**

**Built:** `tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml`,
`…/envoy-rust.yaml`, `…/expectations.yaml` (probes **1–15**), and
`tests/differential/tests/grpc_aware_local_replies.rs` (43 lines).
Registration is **cargo auto-discovery** — no `Cargo.toml` edit, no registry
list, no macro, exactly as `PLAN.md` Global Constraint 14 specifies.

**Both configs are BYTE-IDENTICAL**, asserted with the byte count as well as
the hash because a uniform md5 can be the empty-file md5
`d41d8cd98f00b204e9800998ecf8427e`:

```
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy.yaml
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy-rust.yaml
 6561 envoy.yaml
 6561 envoy-rust.yaml
```

**24 routes** landed in Task 1 so that no later task edits the yamls. No `node:`
block and no unquoted `y`/`n`/`on`/`off` scalar (`grep -nE ': *(y|n|on|off|yes|no)
*$'` returns nothing). All four binding constraints hold in the config as
written: no `201`/`3xx` `direct_response` (CF-110-3), every `direct_response`
carries an explicit `body:` (CF-110-7), the empty cell is
`body: { inline_string: "" }`, and there is no `header_mutation` anywhere
(CF-110-8).

**First run — GREEN, and that green proves only that the fixture EXECUTES:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.25s
```

`110.1` already landed the behaviour, so `0089` is a **CHARACTERIZATION PIN**
that passes on its first run. The mutation is the RED evidence.

### DEVIATION D-1 (SUBSTANTIVE) — `PLAN.md`'s mutation **V1 is MISAIMED and returns a FALSE GREEN**; the corrected one-sided form is what this session ran

`PLAN.md` Task 1 Step 5 specifies changing `/m-403`'s status from `403` to `500`
in **BOTH** yamls, and predicts: *"the RED must come from `diff_headers`,
proving the `grpc-status` VALUE is genuinely compared."* **Run as written, it
returns GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.17s
```

**Root cause, established on disk rather than inferred.** `diff_headers`
(`tests/differential/src/lib.rs:1204-1208`) has the signature

```rust
pub fn diff_headers(
    envoy: &[(String, String)],
    envoy_rust: &[(String, String)],
    allow_list: &[(&str, AllowMode)],
) -> anyhow::Result<()>
```

— it takes **only the two proxies' headers**. There is no fixture-declared
expected header VALUE anywhere in the harness: `Http1HeaderRule` is a unit
variant (`SetEqualModuloAllowList`) carrying no data. So the comparison is
**purely CROSS-PROXY**, and a mutation applied to BOTH configs moves both
proxies in lockstep — upstream maps `500`→`2`, envoy-rust maps `500`→`2`, the
two agree, and `expected_status: 200` / `expected_body: ""` still pass because
the transform fires either way. **This is the "mutation that moves an
implementation and its own witness together and returns a GREEN reading as
'these cells are vacuous'" failure mode, in its cross-proxy form.**

**Correction — break the SYMMETRY: mutate the UPSTREAM side ONLY.** This is
precisely the shape `PLAN.md`'s own V3 (Task 3 Step 3) already prescribes for
the same reason; V1 and V3 were simply inconsistent with each other, and V1 is
the one that is wrong.

Guard first — the anchor must occur EXACTLY ONCE in the one file mutated:

```
$ grep -c 'status: 403, body: { inline_string: "B403" }' envoy.yaml
1
```

Mutation (`envoy.yaml` only, so upstream serves `500` while envoy-rust still
serves `403`) → **RED, naming exactly the intended probe**:

```
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.16s

---- grpc_aware_local_replies stdout ----
thread 'grpc_aware_local_replies' panicked at tests/differential/tests/grpc_aware_local_replies.rs:42:10:
fixture green: probe g-403-maps-to-7: diff_headers

Caused by:
    header `grpc-status`: envoy=`2` envoy-rust=`7`
```

That is the **direct** proof `PLAN.md` intended and did not obtain: the
`grpc-status` VALUE is genuinely compared cross-proxy, and the `403`→`7` cell is
live. The `test result` line EXISTS, so this is a real mutation RED and not a
compile error.

**Revert adjudicated by md5, never by eye** — `git checkout --` would have been
a NO-OP here because the file was still UNTRACKED at this point in Task 1:

```
$ md5sum -c /tmp/v1.md5
envoy.yaml: OK
envoy-rust.yaml: OK
 6561 envoy.yaml
 6561 envoy-rust.yaml
```

**Unmutated CONTROL re-run from the same tree — GREEN:**

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s
```

Both directions of the causal experiment therefore hold: RED with the
asymmetry, GREEN without it. A one-direction result would have proven nothing.
