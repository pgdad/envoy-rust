# 0086 — `Route.redirect` action

Sub-phase **76.2**. The **first differential witness of `Route.redirect`** in the corpus.

18 HTTP/1.1 probes against a **backend-free** HCM listener whose every route is a `redirect:`
action, requiring identical `(status, body, header-set-modulo-allow-list)` between upstream Envoy
`v1.33.0` and envoy-rust.

## What it witnesses

The whole `location` construction rule set, MEASURED against `envoyproxy/envoy:v1.33.0`:

| rule | probes |
|---|---|
| **(a) scheme** — `scheme_redirect` wins and is NOT validated against any allow-list (literal `ftp` emitted verbatim); else `https_redirect: true` forces `https`; else the scheme the request arrived on | `r06`, `r11`, `r12` |
| **(b) authority — THE ASYMMETRY** — `host_redirect` **set** ⇒ the request's original port is **DROPPED**; **unset** ⇒ authority preserved **including** its port; `port_redirect` overrides both, rendered verbatim with **no range clamp**; a scheme-only change does **not** normalise a now-redundant `:443` | `r01`, `r09`, `r14`, **`q01`**, **`q03`** |
| **(c) path** — none ⇒ used as-is; `path_redirect` replaces it wholesale; `prefix_rewrite` replaces only the span matched by the route's `prefix:` and appends the remainder | `r03`, `r05`, `r10` |
| **(d) query** — **preserved by default**, even when `path_redirect` replaced the path wholesale; `strip_query: true` drops it | `r02`, `r04`, `r08`, `r13` |
| **(e) status** — default 301, plus all five `response_code` values on the wire | `r07` (307), `r13` (303), `r15` (302), `r16` (308), rest 301 |

Expected `location` per probe: `r01` `http://example.com/a-host` · `r02`
`http://example.com/b-query/deep?a=b` · `r03` `http://envoy-rust.test/newpath` · `r04`
`http://envoy-rust.test/newpath?k=v` · `r05` `http://envoy-rust.test/replaced/sub` · `r06`
`https://envoy-rust.test/f-https/x` · `r07` `http://example.com/g-c307` · `r08`
`http://example.com/h-strip/a` · `r09` `http://example.com:8443/i-port` · `r10`
`http://envoy-rust.test/j-bare/deep` · `r11` `ftp://envoy-rust.test/k-scheme/x` · `r12`
`https://e.com/l-both/y` · `r13` `http://e.com/m-see/y` · `r14`
`https://envoy-rust.test:443/n-hport/y` · `r15` `http://example.com/o-found` · `r16`
`http://example.com/p-perm` · `q01` `https://envoy-rust.test:1234/q1-hostport/x` · `q03`
`http://envoy-rust.test:1234/q3-hostport/d`.

## Why it needs zero new harness machinery

1. **`location` is NOT on the harness's 3-entry `HEADER_ALLOW_LIST`**
   (`server`, `date`, `x-envoy-upstream-service-time`). `diff_headers` skips value comparison
   *only* for allow-listed names and compares every other name **byte-exact**, so `location` and
   `content-length` are both compared value-exact for free.
   > **NEVER add `location` to that allow-list.** That comparison **is** this fixture's entire
   > witness; allow-listing it would silently vacate the whole thing while leaving it green.
2. **The name-set check catches the `content-type` hazard.** A redirect carries **no
   `content-type`**; a `direct_response` does. `diff_headers` compares lowercased name sets first
   and bails with `only-in-envoy-rust=[…]`, so a redirect accidentally built on the shared
   `synth_with` (which always emits `content-type`) fails loudly rather than subtly.
3. **Both proxies receive an identical `Host:`.** The two proxies listen on **different** ports —
   upstream on a testcontainers-mapped port, the subject on a reserved ephemeral port — but the
   authority in `location` comes from the **`Host` header, not the socket**. That is what makes
   `location` byte-comparable across two differently-ported proxies. Probes **`q01`/`q03`
   deliberately send `Host: envoy-rust.test:1234`, matching neither listen port**, to prove exactly
   that.

## Authoring constraints — binding, and each one can silently vacate a probe

- **Every probe carries a DISTINCT `path:` AND selects a DIFFERENT route.** Verified mechanically:
  18 routes, 18 probes, 18 distinct paths, 18 distinct routes selected, no route left unprobed.
- **No prefix may be a prefix of another** — prefix overlap **silently shadows** a probe (a
  parent-recon cell was lost exactly this way when `/scheme` preceded `/schemehost`). Verified
  mechanically: zero shadowing pairs. This is why `q01`/`q03` get their **own** routes
  (`/q1-hostport`, `/q3-hostport`) rather than re-probing `/f-https` and `/j-bare` with a different
  `Host:`.
- **Every route is `prefix:`-matched, never `path:`.** This keeps the fixture clean of the open
  carry-forward **CF-76-1** — upstream strips the query before route matching while envoy-rust
  matches the raw target, so an exact-`path:` route plus a query would diverge for reasons having
  nothing to do with redirect. A live design constraint, not a footnote.
- **`{{PORT}}` is the only token this driver substitutes** — `Http1ProbeList` is not in
  `driver_needs_admin_port`, so **`{{ADMIN_PORT}}` must not appear.** Verified: absent.
- **`expected_headers` is a BARE SCALAR**, not a map (`Http1HeaderRule` is an externally-tagged
  unit-variant enum). Do not confuse it with the sibling `HeaderRule`, which *is* a map and belongs
  to `Driver::Http1WithAccessLog`. `Http1Probe` is `deny_unknown_fields`, so a typo'd key fails to
  deserialize rather than being ignored.

## Running it

**Backend-free** (`clusters: []`, no `{{BACKEND_PORT}}` marker ⇒ no backend container spawns), so
unlike the backend-routing fixtures this one is **fully verifiable on a developer host** — a
deliberate property of this phase's pick.

```bash
cargo build -p envoy-bin     # MANDATORY: the harness runs target/debug/envoy-bin, not release
cargo test -p differential --test route_redirect_action
```

Always pass `-p differential`; 33 test-binary names are duplicated with `crates/envoy-bin/tests/`,
so a bare `--test <name>` can run the wrong binary.

**`Http1ProbeList` ABORTS AT THE FIRST FAILING PROBE**, so one red run names exactly **one** probe.
Fix that cell, re-run, and expect the next to surface — do **not** read a single named probe as
"only one cell broke". Never weaken a probe, or the allow-list, to make it pass.

## Not witnessed here

The reason phrases (`303 See Other` etc.) are **invisible** to this fixture — the harness parses
the status **code** only — so they are pinned in-process instead. Likewise `prefix_rewrite`'s
mutation of the logged `:path` is a *log* observable, not a response one, and is pinned in-process.
