use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::PathBuf;

fn real_config_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bench.json")
        .display()
        .to_string()
}

fn bench_parallel(c: &mut Criterion) {
    let config_path = real_config_path();
    c.bench_function("plan_runner", |b| {
        b.iter(|| lockfix::commands::plan::runner_par::run(black_box(&config_path)).unwrap())
    });
}

fn bench_sequential_baseline(c: &mut Criterion) {
    let config_path = real_config_path();
    c.bench_function("plan_runner/sequential_baseline", |b| {
        b.iter(|| lockfix::commands::plan::runner::run(black_box(&config_path)).unwrap())
    });
}

criterion_group!(benches, bench_parallel, bench_sequential_baseline);
criterion_main!(benches);
