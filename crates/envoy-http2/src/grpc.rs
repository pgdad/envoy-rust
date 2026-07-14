//! Hand-rolled gRPC health-checking codec + a trailers-aware unary call.
//! `envoy-http2` is the sole user of `h2` (client.rs:2); this module keeps the
//! gRPC-over-H2 logic co-located. NO prost/tonic — the two health messages are
//! one field each (ADR-0139 PV-3).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingStatus {
    Unknown,
    Serving,
    NotServing,
    ServiceUnknown,
}

impl ServingStatus {
    fn from_u64(v: u64) -> ServingStatus {
        match v {
            1 => ServingStatus::Serving,
            2 => ServingStatus::NotServing,
            3 => ServingStatus::ServiceUnknown,
            _ => ServingStatus::Unknown, // 0 and any unknown enum value
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrpcDecodeError {
    ShortFrame,
    Compressed,
    LengthMismatch,
    BadVarint,
    BadWireType,
}

/// Encode `HealthCheckRequest { string service = 1 }` and wrap it in a gRPC
/// length-prefixed frame (1 flag byte 0x00 + 4-byte big-endian length).
pub fn encode_health_check_request(service: &str) -> Vec<u8> {
    // message body: field 1, wire type 2 (length-delimited). Empty service ⇒
    // omit the field (protobuf default) ⇒ empty message.
    let mut msg = Vec::new();
    if !service.is_empty() {
        msg.push(0x0A); // (1 << 3) | 2
        write_varint(&mut msg, service.len() as u64);
        msg.extend_from_slice(service.as_bytes());
    }
    let mut frame = Vec::with_capacity(5 + msg.len());
    frame.push(0x00); // uncompressed
    frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    frame.extend_from_slice(&msg);
    frame
}

/// Decode a gRPC-framed `HealthCheckResponse { ServingStatus status = 1 }`.
pub fn decode_health_check_response(frame: &[u8]) -> Result<ServingStatus, GrpcDecodeError> {
    if frame.len() < 5 {
        return Err(GrpcDecodeError::ShortFrame);
    }
    if frame[0] != 0x00 {
        return Err(GrpcDecodeError::Compressed);
    }
    let len = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
    let body = &frame[5..];
    if body.len() != len {
        return Err(GrpcDecodeError::LengthMismatch);
    }
    let mut status = 0u64; // UNKNOWN default (absent field)
    let mut i = 0usize;
    while i < body.len() {
        let (tag, n) = read_varint(&body[i..]).ok_or(GrpcDecodeError::BadVarint)?;
        i += n;
        let field = tag >> 3;
        let wire = tag & 0x07;
        match wire {
            0 => {
                let (v, n) = read_varint(&body[i..]).ok_or(GrpcDecodeError::BadVarint)?;
                i += n;
                if field == 1 {
                    status = v;
                }
            }
            2 => {
                let (l, n) = read_varint(&body[i..]).ok_or(GrpcDecodeError::BadVarint)?;
                i += n;
                let l = l as usize;
                if i + l > body.len() {
                    return Err(GrpcDecodeError::LengthMismatch);
                }
                i += l;
            }
            1 => { if i + 8 > body.len() { return Err(GrpcDecodeError::LengthMismatch); } i += 8; }
            5 => { if i + 4 > body.len() { return Err(GrpcDecodeError::LengthMismatch); } i += 4; }
            _ => return Err(GrpcDecodeError::BadWireType), // 3/4 groups
        }
    }
    Ok(ServingStatus::from_u64(status))
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

/// Returns (value, bytes_consumed), or None on truncation / >10-byte overrun.
fn read_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;
    for (i, &b) in buf.iter().enumerate() {
        if i >= 10 {
            return None; // varint too long
        }
        result |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
    }
    None // truncated
}

/// Errors surfaced by [`grpc_health_check_call`]: transport-level H2 failures,
/// a non-OK `grpc-status` trailer, a missing trailer block, a codec decode
/// failure, or a response that didn't carry `:status 200`.
#[derive(Debug)]
pub enum GrpcCallError {
    Http2(String),
    GrpcStatus(i64),
    MissingTrailer,
    Decode(GrpcDecodeError),
    BadResponse,
}

/// Perform one unary `grpc.health.v1.Health/Check` call over an existing H2
/// connection (`stream`). Builds the request per the gRPC-over-H2 wire
/// contract (`:method POST`, `:path /grpc.health.v1.Health/Check`, absolute
/// URI, `content-type: application/grpc`, `te: trailers`), sends the framed
/// `HealthCheckRequest` body, drains the response DATA frames releasing
/// flow-control capacity as it goes (mirrors `ClientStream::send_request`'s
/// drain loop at client.rs:193-200), then — unlike `send_request`, which
/// drops `recv_stream` before trailers are available — reads the trailer
/// block via `recv_stream.trailers().await` to recover the `grpc-status`
/// pseudo-trailer that gRPC uses to carry the RPC-level verdict (HTTP/2
/// trailers, RFC 7540 §8.1, cannot be observed any other way).
///
/// `Ok(status)` is produced only when `grpc-status == 0` (OK) AND the body
/// decodes cleanly; any other outcome surfaces the specific `GrpcCallError`
/// variant so the caller (the active-health-check probe, Task 5) can
/// distinguish transport failure from an RPC-level failure from a decoded
/// NOT_SERVING/SERVICE_UNKNOWN status (which is itself an `Ok` — the verdict
/// mapping is the caller's responsibility, not this call's).
pub async fn grpc_health_check_call(
    stream: &mut crate::client::ClientStream,
    authority: &str,
    service: &str,
) -> Result<ServingStatus, GrpcCallError> {
    let uri_str = format!("http://{authority}/grpc.health.v1.Health/Check");
    let http_req = http::Request::builder()
        .method("POST")
        .uri(uri_str.as_str())
        .version(http::Version::HTTP_2)
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .body(())
        .map_err(|e| GrpcCallError::Http2(e.to_string()))?;

    let (response_future, mut send_stream) = stream
        .send_request
        .send_request(http_req, false)
        .map_err(|e| GrpcCallError::Http2(e.to_string()))?;

    let frame = encode_health_check_request(service);
    send_stream
        .send_data(bytes::Bytes::from(frame), true)
        .map_err(|e| GrpcCallError::Http2(e.to_string()))?;

    let http_resp = response_future
        .await
        .map_err(|e| GrpcCallError::Http2(e.to_string()))?;
    let (resp_parts, mut recv_stream) = http_resp.into_parts();

    if resp_parts.status.as_u16() != 200 {
        return Err(GrpcCallError::BadResponse);
    }

    let mut body_bytes = bytes::BytesMut::new();
    while let Some(chunk_result) = recv_stream.data().await {
        let chunk = chunk_result.map_err(|e| GrpcCallError::Http2(e.to_string()))?;
        body_bytes.extend_from_slice(&chunk);
        recv_stream
            .flow_control()
            .release_capacity(chunk.len())
            .map_err(|e| GrpcCallError::Http2(e.to_string()))?;
    }

    let trailers = recv_stream
        .trailers()
        .await
        .map_err(|e| GrpcCallError::Http2(e.to_string()))?
        .ok_or(GrpcCallError::MissingTrailer)?;

    let grpc_status: i64 = trailers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or(GrpcCallError::MissingTrailer)?;

    if grpc_status != 0 {
        return Err(GrpcCallError::GrpcStatus(grpc_status));
    }

    decode_health_check_response(&body_bytes).map_err(GrpcCallError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_request_empty_service() {
        // service="" ⇒ empty message ⇒ frame = flag(0) + len(0)
        assert_eq!(encode_health_check_request(""), vec![0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn encode_request_named_service() {
        // service="svc.up" ⇒ 00 00 00 00 08 0A 06 73 76 63 2E 75 70
        assert_eq!(
            encode_health_check_request("svc.up"),
            vec![0x00, 0x00, 0x00, 0x00, 0x08, 0x0A, 0x06, 0x73, 0x76, 0x63, 0x2E, 0x75, 0x70]
        );
    }

    #[test]
    fn decode_serving() {
        // frame 00 00 00 00 02 08 01 ⇒ SERVING
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 2, 0x08, 0x01]).unwrap(), ServingStatus::Serving);
    }

    #[test]
    fn decode_not_serving() {
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 2, 0x08, 0x02]).unwrap(), ServingStatus::NotServing);
    }

    #[test]
    fn decode_empty_message_is_unknown() {
        // absent field ⇒ protobuf default 0 ⇒ UNKNOWN (NOT healthy)
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 0]).unwrap(), ServingStatus::Unknown);
    }

    #[test]
    fn decode_skips_unknown_field() {
        // an unknown field 2 (wire 2, len 1) before status: 12 01 FF 08 01 ⇒ still SERVING
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 5, 0x12, 0x01, 0xFF, 0x08, 0x01]).unwrap(), ServingStatus::Serving);
    }

    #[test]
    fn decode_rejects_short_frame() {
        assert!(matches!(decode_health_check_response(&[0, 0, 0]), Err(GrpcDecodeError::ShortFrame)));
    }

    #[test]
    fn decode_rejects_compressed() {
        assert!(matches!(decode_health_check_response(&[1, 0, 0, 0, 2, 0x08, 0x01]), Err(GrpcDecodeError::Compressed)));
    }

    #[test]
    fn decode_rejects_length_mismatch() {
        // declared len 9 but only 2 message bytes present
        assert!(matches!(decode_health_check_response(&[0, 0, 0, 0, 9, 0x08, 0x01]), Err(GrpcDecodeError::LengthMismatch)));
    }

    #[tokio::test]
    async fn call_serving_verdict() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Server: accept one H2 conn, read the request, reply SERVING + grpc-status:0 trailer.
        let srv = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut conn = h2::server::handshake(tcp).await.unwrap();
            if let Some(req) = conn.accept().await {
                let (_req, mut respond) = req.unwrap();
                let resp = http::Response::builder()
                    .status(200)
                    .header("content-type", "application/grpc")
                    .body(())
                    .unwrap();
                let mut send = respond.send_response(resp, false).unwrap();
                // SERVING frame: 00 00 00 00 02 08 01
                send.send_data(bytes::Bytes::from_static(&[0, 0, 0, 0, 2, 0x08, 0x01]), false).unwrap();
                let mut trailers = http::HeaderMap::new();
                trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
                send.send_trailers(trailers).unwrap();
            }
            // drive the connection to completion
            while conn.accept().await.is_some() {}
        });
        let mut stream = crate::client::Client::connect(addr, "hc.local").await.unwrap();
        let status = grpc_health_check_call(&mut stream, "hc.local", "").await.unwrap();
        assert_eq!(status, ServingStatus::Serving);
        srv.abort();
    }

    #[tokio::test]
    async fn call_not_serving_still_ok_grpc_status() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let srv = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut conn = h2::server::handshake(tcp).await.unwrap();
            if let Some(req) = conn.accept().await {
                let (_req, mut respond) = req.unwrap();
                let resp = http::Response::builder()
                    .status(200)
                    .header("content-type", "application/grpc")
                    .body(())
                    .unwrap();
                let mut send = respond.send_response(resp, false).unwrap();
                // NOT_SERVING frame: 00 00 00 00 02 08 02
                send.send_data(bytes::Bytes::from_static(&[0, 0, 0, 0, 2, 0x08, 0x02]), false).unwrap();
                let mut trailers = http::HeaderMap::new();
                trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
                send.send_trailers(trailers).unwrap();
            }
            while conn.accept().await.is_some() {}
        });
        let mut stream = crate::client::Client::connect(addr, "hc.local").await.unwrap();
        let status = grpc_health_check_call(&mut stream, "hc.local", "").await.unwrap();
        assert_eq!(status, ServingStatus::NotServing);
        srv.abort();
    }

    #[tokio::test]
    async fn call_nonzero_grpc_status_is_err() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let srv = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut conn = h2::server::handshake(tcp).await.unwrap();
            if let Some(req) = conn.accept().await {
                let (_req, mut respond) = req.unwrap();
                let resp = http::Response::builder()
                    .status(200)
                    .header("content-type", "application/grpc")
                    .body(())
                    .unwrap();
                let mut send = respond.send_response(resp, false).unwrap();
                let mut trailers = http::HeaderMap::new();
                trailers.insert("grpc-status", http::HeaderValue::from_static("5"));
                send.send_trailers(trailers).unwrap();
            }
            while conn.accept().await.is_some() {}
        });
        let mut stream = crate::client::Client::connect(addr, "hc.local").await.unwrap();
        let result = grpc_health_check_call(&mut stream, "hc.local", "").await;
        assert!(matches!(result, Err(GrpcCallError::GrpcStatus(5))), "expected GrpcStatus(5), got {result:?}");
        srv.abort();
    }
}
