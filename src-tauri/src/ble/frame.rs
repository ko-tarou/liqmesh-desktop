//! BLE JSON wire frames (per `docs/BLE_CONTRACT.md`).
//!
//! A transport carries the FULL frame set (hello / msg / reaction / delete /
//! read), not just `msg` — see Contract v1.1 "Transport semantics". Missing or
//! unrecognized `type` values are ignored for forward compatibility:
//! [`Frame::decode`] returns [`Frame::Unknown`] rather than panicking. A
//! **known** `type` with a malformed body, however, is a protocol violation and
//! decodes to `None` (it is not silently downgraded to `Unknown`).
//!
//! JSON keys are camelCase to match the existing iOS/Android wire.
//!
//! roomId default (Contract v1.2/v1.3): a **missing** `roomId` key is restored
//! to `"general"` via serde `default`; an **empty-string** `roomId` is mapped to
//! `"general"` by [`Frame::normalized`]. No other default is permitted.

use serde::{Deserialize, Deserializer, Serialize};

/// Lenient deserializer for `createdAt` (epoch milliseconds).
///
/// Accepts a JSON **number** (the contract form, what iOS/Android send/expect)
/// OR a **string** — a numeric string is parsed as the millis; any other string
/// (e.g. a legacy ISO-8601 value an older Desktop wrote) falls back to `0`
/// rather than failing the whole frame. The goal is to never DROP a `msg` over a
/// timestamp shape, matching the lenient-decode policy the other platforms use.
fn de_epoch_millis<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
    }
    Ok(match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => n,
        NumOrStr::Str(s) => s.trim().parse::<i64>().unwrap_or(0),
    })
}

/// The canonical default room id (Contract v1.2/v1.3): when `roomId` is **absent
/// or empty**, every platform falls back to the literal string `"general"`.
pub const DEFAULT_ROOM_ID: &str = "general";

/// serde `default` for `room_id`: applied when the `roomId` key is **missing**
/// from the wire JSON. (An *empty* `roomId` string is a present-but-blank value,
/// which serde does not treat as missing — that case is handled by
/// [`Frame::normalized`].)
fn default_room_id() -> String {
    DEFAULT_ROOM_ID.to_string()
}

/// A single logical wire frame, internally tagged by `type`.
///
/// The `type` discriminator is rendered in lowerCamelCase to match the wire:
/// `hello`, `msg`, `reaction`, `delete`, `read`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Frame {
    /// Sent in both directions immediately after connect.
    #[serde(rename_all = "camelCase")]
    Hello {
        sender_id: String,
        sender_name: String,
        /// Protocol version; `1` in Contract v1.
        proto_ver: u32,
    },
    /// A chat message.
    #[serde(rename_all = "camelCase")]
    Msg {
        id: String,
        sender_id: String,
        sender_name: String,
        body: String,
        /// Epoch milliseconds (Contract: integer). iOS (`Int64`) and Android
        /// (`getLong`) REQUIRE a JSON number here — an ISO string makes them
        /// throw and drop the whole frame. We serialize a number and accept
        /// either form on decode (number, or a numeric/ISO-ish string → best
        /// effort) so older payloads never hard-fail. See [`de_epoch_millis`].
        #[serde(deserialize_with = "de_epoch_millis")]
        created_at: i64,
        /// roomId is optional-with-default per Contract v1.2/v1.3: a **missing**
        /// `roomId` key restores to `"general"` via serde `default`; an **empty
        /// string** is normalized to `"general"` by [`Frame::normalized`].
        #[serde(default = "default_room_id")]
        room_id: String,
        /// Present only when this message is a reply.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        reply_to_id: Option<String>,
    },
    /// Add/remove an emoji reaction on a message.
    #[serde(rename_all = "camelCase")]
    Reaction {
        message_id: String,
        sender_id: String,
        emoji: String,
        /// Operation, e.g. `"add"` / `"remove"` (kept as the raw wire string).
        op: String,
    },
    /// Tombstone a message.
    #[serde(rename_all = "camelCase")]
    Delete {
        message_id: String,
        sender_id: String,
    },
    /// Read receipt up to a given message in a room.
    #[serde(rename_all = "camelCase")]
    Read {
        /// See [`Frame::Msg::room_id`]: missing → serde default `"general"`;
        /// empty string → normalized to `"general"`.
        #[serde(default = "default_room_id")]
        room_id: String,
        up_to_message_id: String,
        sender_id: String,
    },
    /// Any unrecognized `type` (forward compatibility — never panics).
    #[serde(skip)]
    Unknown,
}

impl Frame {
    /// Serializes the frame to its JSON wire bytes.
    ///
    /// Serializing [`Frame::Unknown`] is a programming error (it has no wire
    /// representation) and yields an empty buffer.
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Parses JSON wire bytes into a [`Frame`].
    ///
    /// Returns `None` for:
    /// - input that is not valid JSON, and
    /// - a frame whose `type` is **known** (`hello`/`msg`/`reaction`/`delete`/
    ///   `read`) but whose body fails to parse (missing/wrong-typed fields) —
    ///   this is a protocol violation and must not be silently swallowed.
    ///
    /// Returns `Some(Frame::Unknown)` for valid JSON whose `type` is **missing
    /// or unrecognized**, preserving forward compatibility with future frame
    /// types rather than dropping the connection.
    pub fn decode(bytes: &[u8]) -> Option<Frame> {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let type_str = value.get("type").and_then(|v| v.as_str());
        match type_str {
            Some("hello") | Some("msg") | Some("reaction") | Some("delete")
            | Some("read") => {
                // Known type: a parse failure is a protocol violation, not an
                // unknown frame.
                serde_json::from_value::<Frame>(value).ok()
            }
            // Missing or unrecognized `type`: forward-compatible Unknown.
            _ => Some(Frame::Unknown),
        }
    }

    /// Returns the frame with its `roomId` canonicalized to the default room.
    ///
    /// serde `default` only fills in a `roomId` that was **missing** from the
    /// wire; a present-but-**empty** string survives deserialization unchanged.
    /// This applies the remaining half of the Contract v1.2/v1.3 rule: an empty
    /// `roomId` on a [`Frame::Msg`] / [`Frame::Read`] becomes
    /// [`DEFAULT_ROOM_ID`] (`"general"`). All other frames pass through
    /// untouched.
    pub fn normalized(mut self) -> Frame {
        match &mut self {
            Frame::Msg { room_id, .. } | Frame::Read { room_id, .. } => {
                if room_id.is_empty() {
                    *room_id = default_room_id();
                }
            }
            _ => {}
        }
        self
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
            created_at: 1_749_168_000_000,
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
            created_at: 1_749_168_060_000,
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
            created_at: 1,
            room_id: "r1".into(),
            reply_to_id: None,
        };
        let json = String::from_utf8(f.encode()).unwrap();
        assert!(json.contains("\"type\":\"msg\""), "{json}");
        assert!(json.contains("\"senderId\""), "{json}");
        assert!(json.contains("\"senderName\""), "{json}");
        assert!(json.contains("\"createdAt\""), "{json}");
        assert!(json.contains("\"roomId\""), "{json}");
        // CONTRACT: createdAt MUST be a JSON number (epoch ms), NOT a string —
        // iOS (Int64) / Android (getLong) throw and drop the whole msg otherwise.
        assert!(json.contains("\"createdAt\":1"), "createdAt must be a number: {json}");
        assert!(!json.contains("\"createdAt\":\""), "createdAt must NOT be a string: {json}");
    }

    #[test]
    fn created_at_decodes_from_number_and_string() {
        // Number (the contract / phone form) decodes to the exact millis.
        let n = br#"{"type":"msg","id":"m","senderId":"u","senderName":"A","body":"b","createdAt":1749168000000,"roomId":"r"}"#;
        match Frame::decode(n).expect("decode num") {
            Frame::Msg { created_at, .. } => assert_eq!(created_at, 1_749_168_000_000),
            other => panic!("expected Msg, got {other:?}"),
        }
        // A numeric STRING (lenient) parses to the millis rather than failing.
        let s = br#"{"type":"msg","id":"m","senderId":"u","senderName":"A","body":"b","createdAt":"1749168000000","roomId":"r"}"#;
        match Frame::decode(s).expect("decode numeric str") {
            Frame::Msg { created_at, .. } => assert_eq!(created_at, 1_749_168_000_000),
            other => panic!("expected Msg, got {other:?}"),
        }
        // A non-numeric (legacy ISO) string must NOT drop the frame — falls to 0.
        let iso = br#"{"type":"msg","id":"m","senderId":"u","senderName":"A","body":"b","createdAt":"2026-06-06T00:00:00Z","roomId":"r"}"#;
        match Frame::decode(iso).expect("decode iso str (lenient)") {
            Frame::Msg { created_at, .. } => assert_eq!(created_at, 0),
            other => panic!("expected Msg, got {other:?}"),
        }
    }

    #[test]
    fn omitted_reply_to_id_is_not_serialized() {
        let f = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "A".into(),
            body: "b".into(),
            created_at: 1,
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
    fn known_type_with_missing_required_field_returns_none() {
        // `msg` is a known type but is missing required fields → protocol
        // violation, not a forward-compat unknown frame.
        let json = br#"{"type":"msg","id":"m1"}"#;
        assert_eq!(Frame::decode(json), None);
    }

    #[test]
    fn known_type_with_wrong_field_type_returns_none() {
        // `hello.protoVer` must be a number; a string is a malformed known frame.
        let json = br#"{"type":"hello","senderId":"u1","senderName":"A","protoVer":"1"}"#;
        assert_eq!(Frame::decode(json), None);
    }

    #[test]
    fn malformed_json_returns_none() {
        assert_eq!(Frame::decode(b"not json at all"), None);
        assert_eq!(Frame::decode(b"{"), None);
    }

    #[test]
    fn msg_missing_room_id_defaults_to_general() {
        // roomId is optional-with-default (Contract v1.2): a missing key restores
        // to "general" via serde `default`, NOT to None / a decode failure.
        let json = br#"{"type":"msg","id":"m1","senderId":"u1","senderName":"A","body":"b","createdAt":"t"}"#;
        match Frame::decode(json).expect("decode") {
            Frame::Msg { room_id, .. } => assert_eq!(room_id, "general"),
            other => panic!("expected Msg, got {other:?}"),
        }
    }

    #[test]
    fn read_missing_room_id_defaults_to_general() {
        let json = br#"{"type":"read","upToMessageId":"m9","senderId":"u2"}"#;
        match Frame::decode(json).expect("decode") {
            Frame::Read { room_id, .. } => assert_eq!(room_id, "general"),
            other => panic!("expected Read, got {other:?}"),
        }
    }

    #[test]
    fn other_missing_required_field_still_returns_none() {
        // The roomId default must NOT loosen the rest of the schema: a `msg`
        // missing a genuinely required field (e.g. `body`) is still a protocol
        // violation and decodes to None.
        let json = br#"{"type":"msg","id":"m1","senderId":"u1","senderName":"A","createdAt":"t","roomId":"r1"}"#;
        assert_eq!(Frame::decode(json), None);
    }

    #[test]
    fn normalized_maps_empty_room_id_to_general() {
        // serde default only covers a *missing* key; an explicit empty string is
        // canonicalized by normalized().
        let f = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "A".into(),
            body: "b".into(),
            created_at: 1,
            room_id: "".into(),
            reply_to_id: None,
        }
        .normalized();
        match f {
            Frame::Msg { room_id, .. } => assert_eq!(room_id, "general"),
            other => panic!("expected Msg, got {other:?}"),
        }
    }

    #[test]
    fn normalized_leaves_nonempty_room_id_untouched() {
        let f = Frame::Read {
            room_id: "lobby".into(),
            up_to_message_id: "m9".into(),
            sender_id: "u2".into(),
        }
        .normalized();
        match f {
            Frame::Read { room_id, .. } => assert_eq!(room_id, "lobby"),
            other => panic!("expected Read, got {other:?}"),
        }
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
