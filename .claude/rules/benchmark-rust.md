---
paths:
  - "benches/**/*.rs"
---

# Rust Benchmarking

## Criterion.rs

Use Criterion for benchmarks — Rust's built-in `#[bench]`
requires nightly and provides no statistical analysis.
Criterion runs benchmarks multiple times, computes
confidence intervals, detects regressions, and generates
HTML reports — this statistical rigor prevents chasing
meaningless performance variations.

### Setup

Add Criterion as a dev dependency and declare each
benchmark binary with `harness = false` — this disables
the default test harness so Criterion manages execution:

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "my_benchmark"
harness = false
```

Place benchmark files in `benches/` at the project root.

### Writing Benchmarks

```rust
use criterion::{
    black_box, criterion_group, criterion_main,
    BenchmarkId, Criterion,
};
use mycrate::function_to_benchmark;

fn bench_example(c: &mut Criterion) {
    c.bench_function("descriptive_name", |b| {
        b.iter(|| function_to_benchmark(black_box(input)))
    });
}

criterion_group!(benches, bench_example);
criterion_main!(benches);
```

### black_box

Always wrap benchmark inputs and outputs with
`std::hint::black_box()` — without it, the compiler may
constant-fold or dead-code-eliminate the benchmarked
computation, producing artificially fast results that
measure nothing.

### Input Scaling

Benchmark across multiple input sizes — small inputs hide
algorithmic issues (O(n^2) vs O(n log n)) that only
surface at scale. Use `BenchmarkGroup` with
`bench_with_input`:

```rust
fn bench_sorting(c: &mut Criterion) {
    let mut group = c.benchmark_group("sorting");
    for size in [100, 1_000, 10_000] {
        let data: Vec<i32> = (0..size).rev().collect();
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &data,
            |b, data| b.iter(|| data.clone().sort()),
        );
    }
    group.finish();
}
```

### Running and Results

Run with `cargo bench`. Criterion reports mean execution
time with confidence intervals, performance change vs.
previous run, and outlier detection. HTML reports are
generated at `target/criterion/report/index.html`.

## Profile Before Optimizing

Benchmarks confirm improvements — profiling identifies
where to look. Use `cargo flamegraph` for visual hotspot
analysis. Build release with debug symbols for accurate
profiles:

```toml
[profile.release]
debug = true
```

For release builds that reflect production performance,
enable link-time optimization:

```toml
[profile.release]
lto = true
codegen-units = 1
```

## Common Pitfalls

| Pitfall | Impact | Fix |
|---|---|---|
| Missing `black_box()` | Compiler elides the computation | Wrap inputs and outputs |
| Only small inputs | Hides algorithmic complexity | Test across multiple sizes |
| Allocations in `iter()` | Measures allocator, not your code | Pre-allocate outside the closure |
| Ignoring outliers | System load skews results | Run on a quiet machine, check reports |
| Cloning in hot path | Measures clone cost | Benchmark the operation in isolation |
