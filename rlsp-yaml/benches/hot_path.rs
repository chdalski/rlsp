// SPDX-License-Identifier: MIT

mod fixtures;

use criterion::{Criterion, criterion_group, criterion_main};
use rlsp_yaml::parser::parse_yaml;

fn bench_parse_smoke(c: &mut Criterion) {
    let text = fixtures::tiny();
    c.bench_function("parse_yaml/tiny", |b| {
        b.iter(|| parse_yaml(&text));
    });
}

criterion_group!(benches, bench_parse_smoke);
criterion_main!(benches);
