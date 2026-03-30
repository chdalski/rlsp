// SPDX-License-Identifier: MIT

mod fixtures;

use criterion::{Criterion, criterion_group, criterion_main};
use rlsp_yaml::validators::validate_unused_anchors;

fn bench_validators_smoke(c: &mut Criterion) {
    let text = fixtures::tiny();
    c.bench_function("validate_unused_anchors/tiny", |b| {
        b.iter(|| validate_unused_anchors(&text));
    });
}

criterion_group!(benches, bench_validators_smoke);
criterion_main!(benches);
