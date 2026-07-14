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
}
