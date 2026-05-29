//! Real-tracker memory benchmark for CI
//!
//! Starts an embedded tracker, feeds production-pattern announce traffic,
//! samples RSS periodically.  Outputs CSV for trend analysis.
//!
//! Usage: cargo run --release --example memory_tracker_bench

mod memory_bench_common;

fn main() {
    memory_bench_common::run_large_bench("large");
}
