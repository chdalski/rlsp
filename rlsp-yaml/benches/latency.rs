// SPDX-License-Identifier: MIT

mod fixtures;

use criterion::{Criterion, criterion_group, criterion_main};
use rlsp_yaml::semantic_tokens::semantic_tokens;

fn bench_semantic_tokens_smoke(c: &mut Criterion) {
    let text = fixtures::tiny();
    c.bench_function("semantic_tokens/tiny", |b| {
        b.iter(|| semantic_tokens(&text));
    });
}

criterion_group!(benches, bench_semantic_tokens_smoke);
criterion_main!(benches);
