use criterion::{Criterion, black_box, criterion_group, criterion_main};
use time_ledger_sim_rust::messaging::store::{ZoneStat, fold_stats, fraud_verdict};

fn bench_pure(c: &mut Criterion) {
    c.bench_function("fraud_verdict", |b| {
        b.iter(|| fraud_verdict(black_box(Some(3600))))
    });
    c.bench_function("fold_stats", |b| {
        b.iter(|| fold_stats(black_box(ZoneStat::default()), black_box(100)))
    });
}

fn bench_serialize(c: &mut Criterion) {
    let payload = serde_json::json!({
        "event_id": "11111111-1111-1111-1111-111111111111",
        "transaction_id": "22222222-2222-2222-2222-222222222222",
        "zone_id": "zone-eu",
        "amount_units": 5000,
    });
    c.bench_function("event_serialize", |b| {
        b.iter(|| serde_json::to_vec(black_box(&payload)).unwrap())
    });
}

criterion_group!(benches, bench_pure, bench_serialize);
criterion_main!(benches);
