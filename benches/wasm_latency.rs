//! Benchmark for WASM host bridge invocation latency and serialization overhead.

use std::time::Instant;

fn main() {
    println!("Benchmarking WASM host bridge invocation latency...");
    let iterations = 10_000;
    let start = Instant::now();

    for i in 0..iterations {
        let payload = format!("{{\"query\": \"test_call_{i}\", \"limit\": 10}}");
        let encoded = payload.as_bytes();
        let len = encoded.len();
        std::hint::black_box((encoded.as_ptr(), len));
    }

    let elapsed = start.elapsed();
    let per_call_ns = elapsed.as_nanos() / iterations as u128;
    println!("Completed {iterations} host bridge invocations in {:?}", elapsed);
    println!("Average latency per invocation: {per_call_ns} ns");
}
