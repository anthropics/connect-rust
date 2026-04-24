//! Codec-layer echo benchmark for ViewEncode.
//!
//! Measures all four `{owned, view} × {decode, encode}` combinations on a
//! ~2-3 KB string-heavy `BloatEcho` payload, isolating the
//! decode → build response → encode path from HTTP/tokio overhead.
//! This is the path a connect-rust unary handler exercises between body
//! receipt and body emission.
//!
//! The hypothesis (verified at the buffa level at 2.19× on a smaller
//! payload) is that view-decode + view-encode together is super-additive
//! versus either half alone, because only the combination eliminates the
//! per-field `String` allocation pass entirely — request-buffer `&str`
//! borrows flow straight through to the response wire bytes.

use std::collections::HashMap;

use buffa::{Message, MessageView, ViewEncode};
use criterion::{black_box, Criterion, Throughput, criterion_group, criterion_main};

use rpc_bench::{BloatEcho, BloatEchoView, BloatHeader, BloatHeaderView};

/// Build the request payload. 8 plain strings (40-60 chars each), 9 tags,
/// 11 label pairs, 2 singular nested headers, 4 repeated nested headers.
/// Target: ~2-3 KB encoded, ~60 string allocations on the owned path.
fn bloat_echo() -> BloatEcho {
    let header = |name: &str, value: &str| BloatHeader {
        name: name.into(),
        value: value.into(),
        source: "client-supplied".into(),
        note: "validated against allowlist; forwarded as-is".into(),
        ..Default::default()
    };
    let labels: HashMap<String, String> = (0..11)
        .map(|i| {
            (
                format!("k8s.label.app.example.com/tier-{i:02}"),
                format!("workload-partition-{i:02}-us-west-2a-r5.2xlarge"),
            )
        })
        .collect();
    BloatEcho {
        tenant_id: "tenant-0193fae1-7d4c-77a2-b8e0-0e9c6ab2d041".into(),
        trace_id: "4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7".into(),
        span_id: "00f067aa0ba902b7".into(),
        service: "api-gateway.ingress.svc.cluster.local".into(),
        region: "us-west-2".into(),
        instance_id: "i-0a1b2c3d4e5f67890-spot-r5.2xlarge".into(),
        request_path: "/api/v2/orders/0193fae1-7d4c/line-items?expand=product,inventory".into(),
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_4) AppleWebKit/605.1.15".into(),
        timestamp_nanos: 1_714_000_000_000_000_000,
        status_code: 200,
        tags: (0..9).map(|i| format!("tag-{i:02}-canary-rollout-cohort")).collect(),
        labels,
        auth: header("authorization", "Bearer eyJhbGciOiJIUzI1NiJ9.dGVzdA.x").into(),
        origin: header("x-forwarded-for", "203.0.113.42, 198.51.100.7, 10.0.0.1").into(),
        extra_headers: vec![
            header("x-request-id", "req-0193fae1-7d4c-77a2-b8e0-0e9c6ab2d041"),
            header("x-correlation-id", "corr-77a2b8e0-0e9c-6ab2-d041-4bf92f3577b3"),
            header("x-client-version", "mobile-ios/4.21.0 (build 8412; arm64)"),
            header("accept-language", "en-US,en;q=0.9,fr-CA;q=0.5"),
        ],
        ..Default::default()
    }
}

/// Borrow a `BloatHeaderView` from an owned `&BloatHeader`.
fn borrow_header(h: &BloatHeader) -> BloatHeaderView<'_> {
    BloatHeaderView {
        name: &h.name,
        value: &h.value,
        source: &h.source,
        note: &h.note,
        ..Default::default()
    }
}

/// Re-borrow a `BloatHeaderView<'a>` (request view) into a fresh response
/// `BloatHeaderView<'a>`. The `&'a str` fields flow through unchanged; only
/// the `__buffa_cached_size` slot is fresh (reset for re-encode).
fn echo_header_view<'a>(h: &BloatHeaderView<'a>) -> BloatHeaderView<'a> {
    BloatHeaderView {
        name: h.name,
        value: h.value,
        source: h.source,
        note: h.note,
        ..Default::default()
    }
}

fn header_to_owned(h: &BloatHeaderView<'_>) -> BloatHeader {
    BloatHeader {
        name: h.name.into(),
        value: h.value.into(),
        source: h.source.into(),
        note: h.note.into(),
        ..Default::default()
    }
}

fn bench_echo_bloat(c: &mut Criterion) {
    let request = bloat_echo();
    let input = request.encode_to_vec();
    let payload_size = input.len() as u64;
    eprintln!("echo_bloat payload: {} bytes", payload_size);

    // One-time wire-equivalence sanity: all four paths produce a response
    // that decodes equal to the request.
    {
        let baseline =
            BloatEcho::decode_from_slice(&owned_owned(&input)).expect("owned/owned roundtrip");
        for (name, out) in [
            ("view/owned", view_owned(&input)),
            ("owned/view", owned_view(&input)),
            ("view/view", view_view(&input)),
        ] {
            let got = BloatEcho::decode_from_slice(&out).expect("decode roundtrip output");
            assert_eq!(got, baseline, "{name} output diverges from owned/owned");
        }
    }

    let mut group = c.benchmark_group("echo_bloat/codec");
    group.throughput(Throughput::Bytes(payload_size));

    group.bench_function("owned_decode_owned_encode", |b| {
        b.iter(|| black_box(owned_owned(black_box(&input))))
    });
    group.bench_function("view_decode_owned_encode", |b| {
        b.iter(|| black_box(view_owned(black_box(&input))))
    });
    group.bench_function("owned_decode_view_encode", |b| {
        b.iter(|| black_box(owned_view(black_box(&input))))
    });
    group.bench_function("view_decode_view_encode", |b| {
        b.iter(|| black_box(view_view(black_box(&input))))
    });

    group.finish();
}

// ── The four paths ────────────────────────────────────────────────────
//
// Each one models a server-side echo handler: decode request bytes →
// build a response struct field-by-field from request data → encode
// response bytes. Baseline (owned/owned) uses move semantics, the most
// charitable owned story.

/// Baseline: owned decode, move every field into a fresh owned response,
/// owned encode. Allocates on decode (per-string `String`, `Vec`/`HashMap`
/// containers) but the build phase is pure moves.
fn owned_owned(input: &[u8]) -> Vec<u8> {
    let req = BloatEcho::decode_from_slice(input).unwrap();
    let resp = BloatEcho {
        tenant_id: req.tenant_id,
        trace_id: req.trace_id,
        span_id: req.span_id,
        service: req.service,
        region: req.region,
        instance_id: req.instance_id,
        request_path: req.request_path,
        user_agent: req.user_agent,
        timestamp_nanos: req.timestamp_nanos,
        status_code: req.status_code,
        tags: req.tags,
        labels: req.labels,
        auth: req.auth,
        origin: req.origin,
        extra_headers: req.extra_headers,
        ..Default::default()
    };
    resp.encode_to_vec()
}

/// Zero-copy decode, but the owned-response API forces `.to_owned()` on
/// every borrowed field — the alloc pass moves from decode to build.
fn view_owned(input: &[u8]) -> Vec<u8> {
    let req = BloatEchoView::decode_view(input).unwrap();
    let resp = BloatEcho {
        tenant_id: req.tenant_id.into(),
        trace_id: req.trace_id.into(),
        span_id: req.span_id.into(),
        service: req.service.into(),
        region: req.region.into(),
        instance_id: req.instance_id.into(),
        request_path: req.request_path.into(),
        user_agent: req.user_agent.into(),
        timestamp_nanos: req.timestamp_nanos,
        status_code: req.status_code,
        tags: req.tags.iter().map(|s| (*s).into()).collect(),
        labels: req
            .labels
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        auth: req.auth.as_option().map(header_to_owned).into(),
        origin: req.origin.as_option().map(header_to_owned).into(),
        extra_headers: req.extra_headers.iter().map(header_to_owned).collect(),
        ..Default::default()
    };
    resp.encode_to_vec()
}

/// Owned decode allocates per-field; the response borrows from those
/// allocations and ViewEncode skips the encode-side `String` pass.
fn owned_view(input: &[u8]) -> Vec<u8> {
    let req = BloatEcho::decode_from_slice(input).unwrap();
    let resp = BloatEchoView {
        tenant_id: &req.tenant_id,
        trace_id: &req.trace_id,
        span_id: &req.span_id,
        service: &req.service,
        region: &req.region,
        instance_id: &req.instance_id,
        request_path: &req.request_path,
        user_agent: &req.user_agent,
        timestamp_nanos: req.timestamp_nanos,
        status_code: req.status_code,
        tags: req.tags.iter().map(String::as_str).collect(),
        labels: req
            .labels
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect(),
        auth: req.auth.as_option().map(borrow_header).map(From::from).unwrap_or_default(),
        origin: req.origin.as_option().map(borrow_header).map(From::from).unwrap_or_default(),
        extra_headers: req.extra_headers.iter().map(borrow_header).collect(),
        ..Default::default()
    };
    resp.encode_to_vec()
}

/// Zero-copy decode, response view borrows directly from the request view's
/// `&'a str` fields — request-buffer bytes flow straight to response-buffer
/// bytes with zero intermediate `String` allocation.
fn view_view(input: &[u8]) -> Vec<u8> {
    let req = BloatEchoView::decode_view(input).unwrap();
    let resp = BloatEchoView {
        tenant_id: req.tenant_id,
        trace_id: req.trace_id,
        span_id: req.span_id,
        service: req.service,
        region: req.region,
        instance_id: req.instance_id,
        request_path: req.request_path,
        user_agent: req.user_agent,
        timestamp_nanos: req.timestamp_nanos,
        status_code: req.status_code,
        tags: req.tags.iter().copied().collect(),
        labels: req.labels.iter().map(|(k, v)| (*k, *v)).collect(),
        auth: req.auth.as_option().map(echo_header_view).map(From::from).unwrap_or_default(),
        origin: req.origin.as_option().map(echo_header_view).map(From::from).unwrap_or_default(),
        extra_headers: req.extra_headers.iter().map(echo_header_view).collect(),
        ..Default::default()
    };
    resp.encode_to_vec()
}

criterion_group!(benches, bench_echo_bloat);
criterion_main!(benches);
