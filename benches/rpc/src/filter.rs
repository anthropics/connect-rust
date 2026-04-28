//! Shared types and helpers for the filter (redaction) benchmark.

use std::collections::HashMap;

pub use crate::connect::anthropic::connectrpc::filter::v1::*;
pub use crate::proto::anthropic::connectrpc::filter::v1::__buffa::view::RecordView;
pub use crate::proto::anthropic::connectrpc::filter::v1::Record;

pub type OwnedRecordView = buffa::view::OwnedView<RecordView<'static>>;

/// True if any sensitive field is non-empty.
pub fn has_sensitive(r: &RecordView<'_>) -> bool {
    !r.email.is_empty() || !r.ssn.is_empty() || !r.notes.is_empty()
}

/// Clear sensitive fields in place.
pub fn scrub(r: &mut Record) {
    r.email.clear();
    r.ssn.clear();
    r.notes.clear();
}

/// Build a sample record. `sensitive` toggles whether `email` is set
/// (the trigger for the redact path).
pub fn sample_record(i: u32, sensitive: bool) -> Record {
    Record {
        id: format!("rec-{i:08}"),
        name: "Some Reasonably Long Display Name For Padding".into(),
        description: "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod".into(),
        email: if sensitive {
            "user@example.invalid".into()
        } else {
            String::new()
        },
        ssn: String::new(),
        notes: String::new(),
        tags: vec![
            "alpha".into(),
            "beta".into(),
            "gamma".into(),
            "delta".into(),
        ],
        attributes: HashMap::from([
            ("region".into(), "us-west-2".into()),
            ("tier".into(), "gold".into()),
            ("source".into(), "api".into()),
        ]),
        ..Default::default()
    }
}

/// Build a batch of `n` encoded request bytes where `pct` percent have a
/// sensitive field set.
pub fn sample_batch(n: usize, pct: u32) -> Vec<bytes::Bytes> {
    use buffa::Message as _;
    (0..n)
        .map(|i| {
            let sensitive = (i as u32 * 100 / n as u32) < pct;
            sample_record(i as u32, sensitive).encode_to_bytes()
        })
        .collect()
}
