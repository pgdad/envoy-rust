//! RFC 8586 `CDN-Loop` header parser + the `envoy.filters.http.cdn_loop`
//! runtime filter (phase 31; ADR-0077). The parser (`parse_cdn_loop` /
//! `count_cdn_id`) is the correctness engine; `CdnLoopFilter` is the
//! decode-side filter that drives it (count/append/reject) — see the
//! `CdnLoopFilter` section below.
//!
//! §6.2-LOCKED against envoyproxy/envoy:v1.33.0.
//!
//! ## Grammar (RFC 8586 §2, layered on RFC 7230 list/token/parameter rules)
//! The `CDN-Loop` header value is a comma-separated list of `cdn-info`; each
//! `cdn-info` = a `cdn-id` (a bare RFC 7230 `token`) optionally followed by
//! `;`-separated `parameter`s (`name=value`, where `value` is a token or a
//! quoted-string).
//!
//! - **cdn-id MUST be a bare token.** A quoted-string id (even a well-formed
//!   `"mycdn.example"`) → MALFORMED. A non-`tchar` in the id → MALFORMED.
//! - **Parameters must be `name=value`.** A bare parameter (`a;b`) → MALFORMED.
//!   A quoted-string value (`a; b="c"`) is OK. An unterminated quoted-string
//!   anywhere → MALFORMED.
//! - **Empty list entries are NOT malformed** (`a,,b`, `a,`, `,a`, `,,,`): they
//!   parse as zero-id placeholders and are preserved verbatim by the caller's
//!   append step (Task 3). Only a structurally-bad `cdn-info` makes the WHOLE
//!   header malformed.
//! - **OWS** (SP / HTAB) is trimmed around each list entry; an all-whitespace
//!   entry is an empty entry, not malformed.
//! - **Matching is case-sensitive** on the `cdn-id` token; parameters are
//!   ignored for matching.
//! - **Multiple `CDN-Loop` request headers** coalesce into one comma-joined
//!   list — hence the `&[&[u8]]` input.
//!
//! Pure function over bytes: no I/O, no `unsafe`, no allocation beyond the
//! returned `Vec`.

use bytes::Bytes;
use thiserror::Error;

use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

const CDN_LOOP_HEADER: &str = "cdn-loop";
/// ADR-0077 §6.2-LOCKED: the 502 loop-detected body — 44 bytes, NO newline.
const LOOP_BODY: &[u8] = b"The server has detected a loop between CDNs.";
/// ADR-0077 §6.2-LOCKED: the 400 malformed body — 35 bytes, NO newline.
const MALFORMED_BODY: &[u8] = b"Invalid CDN-Loop header in request.";

/// One parsed `cdn-info` list entry.
///
/// Carries the trimmed `cdn-id` token bytes (empty for an empty list entry).
/// This is all the filter (Task 3) needs: counting is a case-sensitive token
/// comparison ignoring parameters, and the append/preserve step operates on the
/// raw coalesced header bytes directly (so the original empty entries survive
/// `a,` → `a,,mycdn.example`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdnInfo {
    /// The `cdn-id` token bytes (verbatim, case-preserved). Empty `Vec` for an
    /// empty list entry (a zero-id placeholder).
    pub cdn_id: Vec<u8>,
}

impl CdnInfo {
    /// `true` iff this is an empty list entry (a zero-id placeholder produced by
    /// `a,,b` / `a,` / `,a` / OWS-only entries).
    #[must_use]
    pub fn is_empty_entry(&self) -> bool {
        self.cdn_id.is_empty()
    }
}

/// The `CDN-Loop` header value (coalesced) violated the RFC 8586 / RFC 7230
/// grammar. The caller (Task 3) maps this to a 400 rejection.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("malformed CDN-Loop header value")]
pub struct MalformedCdnLoop;

// Horizontal whitespace permitted as OWS (RFC 7230 §3.2.3): SP and HTAB.
const fn is_ows(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

// RFC 7230 §3.2.6 `tchar`:
//   "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." /
//   "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA
const fn is_tchar(b: u8) -> bool {
    matches!(b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+'
        | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
        | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
}

/// Parse a (possibly multi-header) coalesced `CDN-Loop` value into its list of
/// `cdn-info` entries.
///
/// Multiple header values are treated as one comma-joined list (RFC 7230
/// §3.2.2). Empty entries are preserved as zero-id placeholders. Any
/// structurally-invalid `cdn-info` (non-token id, quoted-string id, bare
/// parameter, unterminated quoted-string) makes the whole input malformed.
///
/// # Errors
/// Returns [`MalformedCdnLoop`] if any `cdn-info` violates the grammar.
pub fn parse_cdn_loop(values: &[&[u8]]) -> Result<Vec<CdnInfo>, MalformedCdnLoop> {
    let mut out = Vec::new();
    for value in values {
        for entry in split_on_comma(value) {
            out.push(parse_cdn_info(entry)?);
        }
    }
    Ok(out)
}

/// Count case-sensitive token matches of `cdn_id` among the parsed entries,
/// ignoring parameters.
#[must_use]
pub fn count_cdn_id(cdn_id: &[u8], parsed: &[CdnInfo]) -> usize {
    parsed.iter().filter(|c| c.cdn_id == cdn_id).count()
}

/// Split a single header value on unquoted `,`. Quoted-strings are NOT scanned
/// here: per the grammar a comma can only legally appear inside a parameter's
/// quoted-string value, and a quoted-string never contains a bare cdn-id comma
/// boundary at the *list* level. We split naively on `,` and let
/// `parse_cdn_info` validate; a comma inside a quoted parameter value would land
/// as a split here, but RFC 7230 list parsing also forbids that at the element
/// boundary, and ADR-0077 §6.2 confirms live Envoy treats every `,` as an
/// element separator. So naive comma-split is the locked behaviour.
fn split_on_comma(value: &[u8]) -> impl Iterator<Item = &[u8]> {
    value.split(|&b| b == b',')
}

/// Parse one OWS-trimmed list entry as a `cdn-info`.
///
/// An empty (or OWS-only) entry yields a zero-id placeholder. Otherwise the
/// entry must be a `token` cdn-id optionally followed by `;`-parameters.
fn parse_cdn_info(entry: &[u8]) -> Result<CdnInfo, MalformedCdnLoop> {
    let entry = trim_ows(entry);
    if entry.is_empty() {
        return Ok(CdnInfo { cdn_id: Vec::new() });
    }

    // cdn-id = 1*tchar
    let mut i = 0;
    while i < entry.len() && is_tchar(entry[i]) {
        i += 1;
    }
    if i == 0 {
        // First byte is not a tchar (e.g. a quoted-string id `"abc"`, or a
        // space/`@`/`/` lead): malformed.
        return Err(MalformedCdnLoop);
    }
    let cdn_id = entry[..i].to_vec();

    // After the id, only OWS then either end-of-entry or `;`-parameters.
    let rest = trim_ows(&entry[i..]);
    if rest.is_empty() {
        return Ok(CdnInfo { cdn_id });
    }
    // A non-`;` trailing byte (e.g. a stray non-tchar like `a@b`, `a b`, `a/b`)
    // is malformed — the id consumed all leading tchars, so anything left that
    // is not a parameter list is invalid.
    if rest[0] != b';' {
        return Err(MalformedCdnLoop);
    }

    parse_parameters(rest)?;
    Ok(CdnInfo { cdn_id })
}

/// Validate a `*( OWS ";" OWS parameter )` tail (`rest` begins at the first
/// `;`). Returns `Ok` iff every parameter is a well-formed `name=value`.
fn parse_parameters(mut rest: &[u8]) -> Result<(), MalformedCdnLoop> {
    while !rest.is_empty() {
        // rest starts at `;` (caller guarantee / loop invariant).
        debug_assert_eq!(rest[0], b';');
        rest = trim_ows(&rest[1..]);

        // parameter name = 1*tchar
        let mut n = 0;
        while n < rest.len() && is_tchar(rest[n]) {
            n += 1;
        }
        if n == 0 {
            // empty parameter name (e.g. `a;;b`, `a; =b`) — malformed.
            return Err(MalformedCdnLoop);
        }
        rest = trim_ows(&rest[n..]);

        // bare parameter (`a;b`) — must be followed by `=value`.
        if rest.is_empty() || rest[0] != b'=' {
            return Err(MalformedCdnLoop);
        }
        rest = trim_ows(&rest[1..]);

        // value = token / quoted-string
        if rest.first() == Some(&b'"') {
            rest = consume_quoted_string(&rest[1..])?;
        } else {
            let mut v = 0;
            while v < rest.len() && is_tchar(rest[v]) {
                v += 1;
            }
            if v == 0 {
                // missing/empty value (`a; b=`) — malformed.
                return Err(MalformedCdnLoop);
            }
            rest = &rest[v..];
        }

        rest = trim_ows(rest);
        if rest.is_empty() {
            break;
        }
        // Only another `;`-parameter may follow.
        if rest[0] != b';' {
            return Err(MalformedCdnLoop);
        }
    }
    Ok(())
}

/// Consume a quoted-string body (the opening `"` already stripped). Returns the
/// remainder after the closing `"`. An unterminated quoted-string → malformed.
///
/// RFC 7230 §3.2.6: `quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE`,
/// `quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )`.
fn consume_quoted_string(mut body: &[u8]) -> Result<&[u8], MalformedCdnLoop> {
    while let Some((&first, tail)) = body.split_first() {
        match first {
            b'"' => return Ok(tail),
            b'\\' => {
                // quoted-pair: the next byte is escaped and consumed verbatim.
                let (_, after) = tail.split_first().ok_or(MalformedCdnLoop)?;
                body = after;
            }
            _ => body = tail,
        }
    }
    // Ran off the end without a closing DQUOTE.
    Err(MalformedCdnLoop)
}

/// Trim leading and trailing OWS (SP / HTAB) from a byte slice.
fn trim_ows(mut s: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = s.split_first() {
        if is_ows(b) {
            s = rest;
        } else {
            break;
        }
    }
    while let Some((&b, rest)) = s.split_last() {
        if is_ows(b) {
            s = rest;
        } else {
            break;
        }
    }
    s
}

// ---------------------------------------------------------------------------
// CdnLoopFilter — the runtime decode-side filter (phase 31 Task 3; ADR-0077)
// ---------------------------------------------------------------------------

/// The `envoy.filters.http.cdn_loop` runtime filter (RFC 8586 loop detection).
///
/// Built once per filter-chain from a `CdnLoopConfig`. On the decode side it
/// coalesces all `cdn-loop` request-header values, parses them, and:
/// - malformed → 400 `Invalid CDN-Loop header in request.`;
/// - `count(cdn_id) > max_allowed_occurrences` → 502
///   `The server has detected a loop between CDNs.`;
/// - else appends this proxy's `cdn_id` (comma-only, on the RAW coalesced bytes
///   to preserve empty entries) and `Continue`s.
///
/// Encode-side is inert. No per-route config this phase. No stats (ADR-0077).
#[derive(Debug, Clone)]
pub struct CdnLoopFilter {
    cdn_id: String,
    max_allowed_occurrences: u32,
}

impl CdnLoopFilter {
    /// Build from the chain-level `CdnLoopConfig`. Infallible — the `cdn_id`
    /// token validity is enforced at config-load time by
    /// `envoy_config::validate_cdn_loop_config` (boot-fatal), not here.
    pub(crate) fn new(cfg: &envoy_config::CdnLoopConfig) -> Self {
        Self {
            cdn_id: cfg.cdn_id.clone(),
            max_allowed_occurrences: cfg.max_allowed_occurrences,
        }
    }

    /// Decode-side entry point (ADR-0077 §6.2-LOCKED).
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        // Coalesce all cdn-loop values in arrival order (RFC 8586 / RFC 7230).
        let raw_values: Vec<Vec<u8>> = req
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(CDN_LOOP_HEADER))
            .map(|(_, v)| v.as_bytes().to_vec())
            .collect();
        let value_refs: Vec<&[u8]> = raw_values.iter().map(Vec::as_slice).collect();

        // Parse → malformed → 400.
        let parsed = match parse_cdn_loop(&value_refs) {
            Ok(p) => p,
            Err(_) => return Decision::StopAndSend(malformed_response()),
        };

        // Loop detection: count > max → 502.
        let count = count_cdn_id(self.cdn_id.as_bytes(), &parsed);
        if count > self.max_allowed_occurrences as usize {
            return Decision::StopAndSend(loop_response());
        }

        // Within limit → append `cdn_id` (comma-only join on the RAW coalesced
        // bytes, preserving empty entries) and forward ONE coalesced header.
        if raw_values.is_empty() {
            // No existing cdn-loop header → add the bare cdn_id (lowercase key).
            req.headers
                .push((CDN_LOOP_HEADER.to_string(), self.cdn_id.clone()));
        } else {
            // Coalesce existing values with a comma (RFC 7230 §3.2.2), then append
            // `,{cdn_id}`. Operate on raw bytes so empty entries survive.
            let mut appended = raw_values.join(&b","[..]);
            appended.push(b',');
            appended.extend_from_slice(self.cdn_id.as_bytes());
            let new_value = String::from_utf8_lossy(&appended).into_owned();

            // Preserve the FIRST existing entry's key string; set its value to the
            // appended bytes; drop the redundant cdn-loop entries.
            let mut first_done = false;
            req.headers.retain_mut(|(k, v)| {
                if k.eq_ignore_ascii_case(CDN_LOOP_HEADER) {
                    if first_done {
                        return false; // drop redundant entries
                    }
                    first_done = true;
                    *v = new_value.clone();
                }
                true
            });
        }
        Decision::Continue
    }

    /// CDN-Loop is decode-side only; encode is the trivial `Continue` arm (the
    /// exhaustive-match arm for the `HttpFilterInstance` wiring).
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

/// The 502 loop-detected local reply (ADR-0077 §6.2). `content-type`,
/// `content-length`, `server`(, `connection`) are stamped by the H1/H2 synth
/// decorators downstream (the csrf/buffer/rbac precedent).
fn loop_response() -> FilterResponse {
    FilterResponse {
        status: 502,
        reason: Some("Bad Gateway"),
        headers: Vec::new(),
        body: Bytes::from_static(LOOP_BODY),
    }
}

/// The 400 malformed-header local reply (ADR-0077 §6.2).
fn malformed_response() -> FilterResponse {
    FilterResponse {
        status: 400,
        reason: Some("Bad Request"),
        headers: Vec::new(),
        body: Bytes::from_static(MALFORMED_BODY),
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::pipeline::Decision;
    use crate::types::FilterRequest;

    fn filter(cdn_id: &str, max: u32) -> CdnLoopFilter {
        CdnLoopFilter::new(&envoy_config::CdnLoopConfig {
            cdn_id: cdn_id.to_string(),
            max_allowed_occurrences: max,
        })
    }

    fn req(headers: &[(&str, &str)]) -> FilterRequest {
        FilterRequest {
            method: "GET".into(),
            path: "/".into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: None,
        }
    }

    // Extract the (single) cdn-loop header value (case-insensitively) post-decode.
    fn cdn_loop_value(r: &FilterRequest) -> Option<String> {
        let mut found: Vec<&str> = r
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("cdn-loop"))
            .map(|(_, v)| v.as_str())
            .collect();
        match found.len() {
            0 => None,
            1 => Some(found.remove(0).to_string()),
            _ => panic!("expected exactly one cdn-loop header after decode, got {found:?}"),
        }
    }

    // §A probe: no header → Continue AND the request now carries `cdn-loop: mycdn.example`.
    #[test]
    fn no_header_appends_bare_cdn_id_and_continues() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(cdn_loop_value(&r).as_deref(), Some("mycdn.example"));
    }

    // §A probe: foreign id → Continue + `cdn-loop: othercdn.example,mycdn.example` (comma-only).
    #[test]
    fn foreign_id_appends_comma_only_and_continues() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "othercdn.example")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(
            cdn_loop_value(&r).as_deref(),
            Some("othercdn.example,mycdn.example")
        );
    }

    // §A probe: self id at limit 0 → 502 loop body.
    #[test]
    fn self_id_over_limit_rejects_502() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "mycdn.example")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 502);
                assert_eq!(resp.reason, Some("Bad Gateway"));
                assert_eq!(
                    &resp.body[..],
                    b"The server has detected a loop between CDNs."
                );
                assert_eq!(resp.body.len(), 44);
                assert!(resp.headers.is_empty());
            }
            Decision::Continue => panic!("expected 502 loop rejection"),
        }
    }

    // §A probe: malformed → 400 invalid body.
    #[test]
    fn malformed_header_rejects_400() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "a@b")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 400);
                assert_eq!(resp.reason, Some("Bad Request"));
                assert_eq!(&resp.body[..], b"Invalid CDN-Loop header in request.");
                assert_eq!(resp.body.len(), 35);
                assert!(resp.headers.is_empty());
            }
            Decision::Continue => panic!("expected 400 malformed rejection"),
        }
    }

    // §A probe: max_allowed_occurrences: 1 boundary — one self entry → Continue+append.
    #[test]
    fn boundary_one_self_entry_within_limit_appends() {
        let mut f = filter("mycdn.example", 1);
        let mut r = req(&[("cdn-loop", "mycdn.example")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(
            cdn_loop_value(&r).as_deref(),
            Some("mycdn.example,mycdn.example")
        );
    }

    // §A probe: max_allowed_occurrences: 1 boundary — two self entries → 502.
    #[test]
    fn boundary_two_self_entries_over_limit_rejects_502() {
        let mut f = filter("mycdn.example", 1);
        let mut r = req(&[("cdn-loop", "mycdn.example,mycdn.example")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => assert_eq!(resp.status, 502),
            Decision::Continue => panic!("expected 502 at count=2 > max=1"),
        }
    }

    // Empty entries preserved on the RAW bytes: `othercdn.example,` → `othercdn.example,,mycdn.example`.
    #[test]
    fn empty_entries_preserved_on_append() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "othercdn.example,")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(
            cdn_loop_value(&r).as_deref(),
            Some("othercdn.example,,mycdn.example")
        );
    }

    // Multiple cdn-loop request headers are coalesced (arrival order) before count
    // AND before append; after append ONE header is emitted.
    #[test]
    fn multiple_headers_coalesced_then_appended_to_one() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "a"), ("cdn-loop", "b")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(cdn_loop_value(&r).as_deref(), Some("a,b,mycdn.example"));
    }

    // Append-to-existing preserves the FIRST existing entry's key casing.
    #[test]
    fn append_preserves_first_existing_key_casing() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("CDN-Loop", "othercdn.example")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        let key = r
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("cdn-loop"))
            .map(|(k, _)| k.as_str());
        assert_eq!(key, Some("CDN-Loop"));
        assert_eq!(
            cdn_loop_value(&r).as_deref(),
            Some("othercdn.example,mycdn.example")
        );
    }

    // Coalescing a malformed-in-any-value multi-header → 400.
    #[test]
    fn multi_header_malformed_in_any_value_rejects_400() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "ok"), ("cdn-loop", "a@b")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => assert_eq!(resp.status, 400),
            Decision::Continue => panic!("expected 400 for malformed coalesced value"),
        }
    }

    // -----------------------------------------------------------------------
    // Phase-31 Task 5 — §A.4 edge matrix at the FILTER level (gaps not already
    // pinned by Task 1's parser oracle nor Task 3's filter probes).
    // -----------------------------------------------------------------------

    // Case-sensitivity OBSERVED THROUGH THE FILTER: a capitalised variant of the
    // configured id is a FOREIGN id (no 502) → Continue + comma-only append.
    // (Task 1 pins case-sensitivity at the parser; Task 3 did not exercise it at
    // the filter disposition level.)
    #[test]
    fn case_variant_of_self_is_foreign_appends_not_502() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "MYCDN.EXAMPLE")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(
            cdn_loop_value(&r).as_deref(),
            Some("MYCDN.EXAMPLE,mycdn.example")
        );
    }

    // Parameter-IGNORING match + parameter-PRESERVING append: a foreign id that
    // carries a `;`-parameter is matched ignoring the param (so no 502 for a
    // DIFFERENT configured id), and the param survives byte-verbatim on the
    // forwarded (raw-appended) header.
    #[test]
    fn foreign_id_with_parameter_preserves_param_on_append() {
        let mut f = filter("othercdn.example", 0);
        let mut r = req(&[("cdn-loop", "mycdn.example; foo=bar")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(
            cdn_loop_value(&r).as_deref(),
            Some("mycdn.example; foo=bar,othercdn.example")
        );
    }

    // Parameter-ignoring match THROUGH THE FILTER also covers the loop case: a
    // self id carrying a parameter still counts as the self id → 502 at limit 0.
    #[test]
    fn self_id_with_parameter_still_loops_502() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "mycdn.example; trace=\"abc\"")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => assert_eq!(resp.status, 502),
            Decision::Continue => panic!("param-bearing self id must still loop → 502"),
        }
    }

    // Multi-header COALESCE → 502: two cdn-loop headers each carrying one self id
    // coalesce to count=2 > max=0 → loop rejection. (Task 3 pinned coalesce →
    // append and coalesce → 400; this fills the coalesce → 502 disposition.)
    #[test]
    fn multi_header_coalesced_self_count_over_limit_rejects_502() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "mycdn.example"), ("cdn-loop", "mycdn.example")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 502);
                assert_eq!(resp.body.len(), 44);
            }
            Decision::Continue => panic!("coalesced self count=2 > max=0 must be 502"),
        }
    }

    // Empty-entry / malformed-id BOUNDARY at the filter: an all-empty list
    // (`,,,`) is NOT malformed → Continue + append (empties preserved on raw
    // bytes); a malformed id (unterminated quote) → 400. Pins both sides of the
    // boundary at the disposition level.
    #[test]
    fn only_commas_not_malformed_appends_at_filter() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", ",,,")]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
        assert_eq!(cdn_loop_value(&r).as_deref(), Some(",,,,mycdn.example"));
    }

    #[test]
    fn unterminated_quote_id_rejects_400_at_filter() {
        let mut f = filter("mycdn.example", 0);
        let mut r = req(&[("cdn-loop", "\"abc")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => assert_eq!(resp.status, 400),
            Decision::Continue => panic!("unterminated-quote id must be 400 malformed"),
        }
    }

    // OWS around a list entry is TRIMMED for matching/counting, but the filter
    // appends on the RAW coalesced bytes — so the original OWS survives verbatim
    // on the forwarded header (it does not re-serialize the trimmed parse).
    #[test]
    fn ows_trimmed_for_count_but_raw_bytes_preserved_on_append() {
        let mut f = filter("mycdn.example", 0);
        // `  mycdn.example  ` trims to the self id → would loop at limit 0.
        let mut r = req(&[("cdn-loop", "  mycdn.example  ")]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => assert_eq!(resp.status, 502),
            Decision::Continue => panic!("OWS-trimmed self id must be counted → 502"),
        }
        // A foreign OWS-padded id continues, and the OWS survives on the raw append.
        let mut f2 = filter("mycdn.example", 0);
        let mut r2 = req(&[("cdn-loop", "  othercdn.example  ")]);
        assert!(matches!(f2.decode_headers(&mut r2), Decision::Continue));
        assert_eq!(
            cdn_loop_value(&r2).as_deref(),
            Some("  othercdn.example  ,mycdn.example")
        );
    }

    // `max_allowed_occurrences > 0` general boundary (Task 3 pinned max=1; this
    // pins max=2): count==max → Continue+append; count==max+1 → 502.
    #[test]
    fn boundary_max_two_at_limit_continues_over_limit_502() {
        let mut f = filter("mycdn.example", 2);
        let mut r = req(&[("cdn-loop", "mycdn.example,mycdn.example")]);
        assert!(
            matches!(f.decode_headers(&mut r), Decision::Continue),
            "count=2 == max=2 must Continue"
        );
        assert_eq!(
            cdn_loop_value(&r).as_deref(),
            Some("mycdn.example,mycdn.example,mycdn.example")
        );

        let mut f2 = filter("mycdn.example", 2);
        let mut r2 = req(&[("cdn-loop", "mycdn.example,mycdn.example,mycdn.example")]);
        match f2.decode_headers(&mut r2) {
            Decision::StopAndSend(resp) => assert_eq!(resp.status, 502),
            Decision::Continue => panic!("count=3 > max=2 must be 502"),
        }
    }

    // Encode is inert.
    #[test]
    fn encode_is_inert() {
        use crate::types::FilterResponse;
        let mut f = filter("mycdn.example", 0);
        let mut resp = FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
        };
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Convenience: parse a single header value (the common case), panicking on
    // malformed so the oracle reads cleanly for the OK rows.
    fn parse_one(value: &[u8]) -> Vec<CdnInfo> {
        parse_cdn_loop(&[value]).expect("expected well-formed CDN-Loop value")
    }

    fn ids(parsed: &[CdnInfo]) -> Vec<&[u8]> {
        parsed.iter().map(|c| c.cdn_id.as_slice()).collect()
    }

    // -----------------------------------------------------------------------
    // §A.4 pinned oracle — count_cdn_id
    // -----------------------------------------------------------------------

    #[test]
    fn empty_input_counts_zero() {
        let parsed = parse_one(b"");
        // A single empty value is one empty entry (zero-id placeholder).
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 0);
    }

    #[test]
    fn bare_self_append_counts_one() {
        let parsed = parse_one(b"mycdn.example");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
    }

    #[test]
    fn foreign_then_self_counts_one_each() {
        let parsed = parse_one(b"othercdn.example, mycdn.example");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
        assert_eq!(count_cdn_id(b"othercdn.example", &parsed), 1);
        assert_eq!(count_cdn_id(b"notpresent.example", &parsed), 0);
    }

    #[test]
    fn repeated_self_counts_each_occurrence() {
        let parsed = parse_one(b"mycdn.example, foo, mycdn.example, mycdn.example");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 3);
    }

    #[test]
    fn matching_is_case_sensitive() {
        let parsed = parse_one(b"MYCDN.EXAMPLE");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 0);
        assert_eq!(count_cdn_id(b"MYCDN.EXAMPLE", &parsed), 1);
    }

    #[test]
    fn parameters_are_ignored_for_matching() {
        let parsed = parse_one(b"mycdn.example; foo=bar");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
    }

    #[test]
    fn multiple_parameters_are_ignored_for_matching() {
        let parsed = parse_one(b"mycdn.example; foo=bar; baz=qux");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
    }

    #[test]
    fn quoted_string_parameter_value_without_comma_is_ok_and_ignored() {
        let parsed = parse_one(b"mycdn.example; trace=\"abc\"");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
    }

    // -----------------------------------------------------------------------
    // §A.4 pinned oracle — empty / OWS entries are NOT malformed
    // -----------------------------------------------------------------------

    #[test]
    fn double_comma_is_not_malformed() {
        let parsed = parse_one(b"a,,b");
        assert_eq!(ids(&parsed), vec![&b"a"[..], &b""[..], &b"b"[..]]);
        assert_eq!(count_cdn_id(b"a", &parsed), 1);
        assert_eq!(count_cdn_id(b"b", &parsed), 1);
    }

    #[test]
    fn trailing_comma_is_not_malformed() {
        let parsed = parse_one(b"a,");
        assert_eq!(ids(&parsed), vec![&b"a"[..], &b""[..]]);
    }

    #[test]
    fn leading_comma_is_not_malformed() {
        let parsed = parse_one(b",a");
        assert_eq!(ids(&parsed), vec![&b""[..], &b"a"[..]]);
    }

    #[test]
    fn only_commas_is_not_malformed() {
        let parsed = parse_one(b",,,");
        // 4 splits of "" → 4 empty entries.
        assert_eq!(parsed.len(), 4);
        assert!(parsed.iter().all(CdnInfo::is_empty_entry));
    }

    #[test]
    fn ows_around_entry_is_trimmed() {
        let parsed = parse_one(b"  othercdn.example  ");
        assert_eq!(ids(&parsed), vec![&b"othercdn.example"[..]]);
        assert_eq!(count_cdn_id(b"othercdn.example", &parsed), 1);
    }

    #[test]
    fn tab_ows_around_entry_is_trimmed() {
        let parsed = parse_one(b"\tmycdn.example\t");
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
    }

    #[test]
    fn all_whitespace_entry_is_empty_not_malformed() {
        let parsed = parse_one(b"a,   ,b");
        assert_eq!(ids(&parsed), vec![&b"a"[..], &b""[..], &b"b"[..]]);
    }

    // -----------------------------------------------------------------------
    // §A.4 pinned oracle — malformed cases → Err
    // -----------------------------------------------------------------------

    #[test]
    fn quoted_string_id_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"\"abc\""]), Err(MalformedCdnLoop));
    }

    #[test]
    fn well_formed_quoted_id_is_still_malformed_not_a_match() {
        // A perfectly-quoted "mycdn.example" must NOT count as a match — it is
        // malformed because the id is a quoted-string, not a bare token.
        assert_eq!(
            parse_cdn_loop(&[b"\"mycdn.example\""]),
            Err(MalformedCdnLoop)
        );
    }

    #[test]
    fn space_in_id_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"a b"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn at_sign_in_id_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"a@b"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn slash_in_id_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"a/b"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn tab_inside_id_is_malformed() {
        // A HTAB mid-token (not leading/trailing OWS) splits the token; the
        // residue after trimming is a non-`;` trailing byte → malformed.
        assert_eq!(parse_cdn_loop(&[b"a\tb"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn bare_parameter_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"a;b"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn empty_parameter_name_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"a;;b"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn parameter_missing_value_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"a; b="]), Err(MalformedCdnLoop));
    }

    #[test]
    fn unterminated_quoted_string_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"\"abc"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn unterminated_quoted_parameter_value_is_malformed() {
        assert_eq!(
            parse_cdn_loop(&[b"a; b=\"unterminated"]),
            Err(MalformedCdnLoop)
        );
    }

    #[test]
    fn well_formed_quoted_parameter_value_is_ok() {
        let parsed = parse_cdn_loop(&[b"a; b=\"quoted value\""]).expect("ok");
        assert_eq!(count_cdn_id(b"a", &parsed), 1);
    }

    #[test]
    fn quoted_pair_escaped_dquote_in_parameter_value_is_ok() {
        let parsed = parse_cdn_loop(&[b"a; b=\"he said \\\"hi\\\"\""]).expect("ok");
        assert_eq!(count_cdn_id(b"a", &parsed), 1);
    }

    // -----------------------------------------------------------------------
    // §A.4 pinned oracle — multi-value (multi-header) coalescing
    // -----------------------------------------------------------------------

    #[test]
    fn multi_header_values_are_coalesced() {
        // ["a", "mycdn.example"] → 2 entries → count of mycdn.example is 1.
        let parsed = parse_cdn_loop(&[b"a", b"mycdn.example"]).expect("ok");
        assert_eq!(parsed.len(), 2);
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
        assert_eq!(count_cdn_id(b"a", &parsed), 1);
    }

    #[test]
    fn multi_header_each_value_split_independently() {
        let parsed = parse_cdn_loop(&[b"a, b", b"c, mycdn.example"]).expect("ok");
        assert_eq!(
            ids(&parsed),
            vec![&b"a"[..], &b"b"[..], &b"c"[..], &b"mycdn.example"[..],]
        );
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 1);
    }

    #[test]
    fn multi_header_malformed_in_any_value_is_malformed() {
        assert_eq!(parse_cdn_loop(&[b"ok", b"a@b"]), Err(MalformedCdnLoop));
    }

    #[test]
    fn no_header_values_yields_empty_parse() {
        let parsed = parse_cdn_loop(&[]).expect("ok");
        assert!(parsed.is_empty());
        assert_eq!(count_cdn_id(b"mycdn.example", &parsed), 0);
    }

    // -----------------------------------------------------------------------
    // Count boundary helper for Task 3's max_allowed_occurrences > 0 gate.
    // -----------------------------------------------------------------------

    #[test]
    fn count_supports_max_allowed_occurrences_boundary() {
        let parsed = parse_one(b"mycdn.example, mycdn.example");
        let count = count_cdn_id(b"mycdn.example", &parsed);
        let max_allowed: usize = 1;
        // Task 3 rejects with 502 when count > max_allowed_occurrences.
        assert_eq!(count, 2);
        assert!(count > max_allowed);
    }

    // -----------------------------------------------------------------------
    // Locked split behaviour: naive comma split (ADR-0077 §6.2).
    // -----------------------------------------------------------------------

    #[test]
    fn quoted_value_comma_splits_per_locked() {
        // A comma inside a quoted parameter value is split at the list level
        // (naive comma split is the LOCKED behaviour, matching live Envoy). The
        // first entry `mycdn.example; trace="a` has an unterminated quote →
        // therefore the WHOLE header is malformed. Pin that.
        assert_eq!(
            parse_cdn_loop(&[b"mycdn.example; trace=\"a,b\""]),
            Err(MalformedCdnLoop)
        );
    }
}
