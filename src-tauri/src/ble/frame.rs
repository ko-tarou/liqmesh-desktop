//! BLE JSON wire frames (per `docs/BLE_CONTRACT.md`).
//!
//! A transport carries the FULL frame set (hello / msg / reaction / delete /
//! read), not just `msg` — see Contract v1.1 "Transport semantics". Unknown
//! `type` values are ignored for forward compatibility: [`Frame::decode`]
//! returns [`Frame::Unknown`] rather than panicking.
//!
//! JSON keys are camelCase to match the existing iOS/Android wire.

// stub for red phase

use serde::{Deserialize, Serialize};

/// A single logical wire frame, internally tagged by `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Frame {
    Hello {},
    Msg {},
    Reaction {},
    Delete {},
    Read {},
    Unknown,
}

impl Frame {
    /// Serializes the frame to its JSON wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Parses JSON wire bytes into a [`Frame`], returning `None` only for
    /// malformed JSON. Unknown `type` values decode to [`Frame::Unknown`].
    pub fn decode(_bytes: &[u8]) -> Option<Frame> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(f: &Frame) {
        let bytes = f.encode();
        let back = Frame::decode(&bytes).expect("decode ok");
        assert_eq!(&back, f);
    }

    #[test]
    fn hello_round_trip() {
        round_trip(&Frame::Hello {
            sender_id: "u1".into(),
            sender_name: "Alice".into(),
            proto_ver: 1,
        });
    }

    #[test]
    fn msg_round_trip_with_reply() {
        round_trip(&Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "Alice".into(),
            body: "hi".into(),
            created_at: "2026-06-06T00:00:00Z".into(),
            room_id: "r1".into(),
            reply_to_id: Some("m0".into()),
        });
    }

    #[test]
    fn msg_round_trip_without_reply() {
        round_trip(&Frame::Msg {
            id: "m2".into(),
            sender_id: "u2".into(),
            sender_name: "Bob".into(),
            body: "yo".into(),
            created_at: "2026-06-06T00:01:00Z".into(),
            room_id: "r1".into(),
            reply_to_id: None,
        });
    }

    #[test]
    fn reaction_round_trip() {
        round_trip(&Frame::Reaction {
            message_id: "m1".into(),
            sender_id: "u2".into(),
            emoji: "👍".into(),
            op: "add".into(),
        });
    }

    #[test]
    fn delete_round_trip() {
        round_trip(&Frame::Delete {
            message_id: "m1".into(),
            sender_id: "u1".into(),
        });
    }

    #[test]
    fn read_round_trip() {
        round_trip(&Frame::Read {
            room_id: "r1".into(),
            up_to_message_id: "m9".into(),
            sender_id: "u2".into(),
        });
    }

    #[test]
    fn wire_keys_are_camel_case() {
        let f = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "Alice".into(),
            body: "hi".into(),
            created_at: "t".into(),
            room_id: "r1".into(),
            reply_to_id: None,
        };
        let json = String::from_utf8(f.encode()).unwrap();
        assert!(json.contains("\"type\":\"msg\""), "{json}");
        assert!(json.contains("\"senderId\""), "{json}");
        assert!(json.contains("\"senderName\""), "{json}");
        assert!(json.contains("\"createdAt\""), "{json}");
        assert!(json.contains("\"roomId\""), "{json}");
    }

    #[test]
    fn omitted_reply_to_id_is_not_serialized() {
        let f = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "A".into(),
            body: "b".into(),
            created_at: "t".into(),
            room_id: "r1".into(),
            reply_to_id: None,
        };
        let json = String::from_utf8(f.encode()).unwrap();
        assert!(!json.contains("replyToId"), "{json}");
    }

    #[test]
    fn msg_decodes_without_reply_to_id_field() {
        // a wire message that omits replyToId entirely must still decode.
        let json = br#"{"type":"msg","id":"m1","senderId":"u1","senderName":"A","body":"b","createdAt":"t","roomId":"r1"}"#;
        let f = Frame::decode(json).expect("decode");
        match f {
            Frame::Msg { reply_to_id, .. } => assert_eq!(reply_to_id, None),
            other => panic!("expected Msg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_decodes_to_unknown_not_panic() {
        let json = br#"{"type":"typing","senderId":"u1","extra":42}"#;
        assert_eq!(Frame::decode(json), Some(Frame::Unknown));
    }

    #[test]
    fn missing_type_decodes_to_unknown() {
        let json = br#"{"foo":"bar"}"#;
        assert_eq!(Frame::decode(json), Some(Frame::Unknown));
    }

    #[test]
    fn malformed_json_returns_none() {
        assert_eq!(Frame::decode(b"not json at all"), None);
        assert_eq!(Frame::decode(b"{"), None);
    }

    #[test]
    fn hello_decodes_from_contract_shaped_json() {
        let json = br#"{"type":"hello","senderId":"u1","senderName":"Alice","protoVer":1}"#;
        assert_eq!(
            Frame::decode(json),
            Some(Frame::Hello {
                sender_id: "u1".into(),
                sender_name: "Alice".into(),
                proto_ver: 1,
            })
        );
    }
}
