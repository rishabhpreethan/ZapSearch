use criterion::{criterion_group, criterion_main, Criterion};
use core::tokenizer::tokenize;

fn bench_tokenize(c: &mut Criterion) {
    let text = include_str!("../README.md");
    c.bench_function("tokenize_readme", |b| b.iter(|| tokenize(text)));
}

criterion_group!(benches, bench_tokenize);
criterion_main!(benches);
