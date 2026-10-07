//! Bounded G001 adapter to the canonical cofi-audit event codecs.
//!
//! The SHA-256 canonicalization and audit chain exist only in cofi-audit.
//! No hash or private audit-index reimplementation is introduced.

pub use cofi_audit::codec::{
    AuditCodecError, AuditStreamAnchor, decode_audit_event, encode_audit_event,
    replay_audit_events, replay_audit_events_anchored,
};
