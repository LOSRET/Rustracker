//! Real-tracker memory benchmark with mimalloc allocator
//!
//! Same workload as memory_tracker_bench but using mimalloc.
//! Compare CSV output with the system-allocator and jemalloc versions
//! to measure the RSS difference at scale.
//!
//! Usage: cargo run --release --example memory_mimalloc_bench

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod memory_bench_common;

fn main() {
    memory_bench_common::run_large_bench("mimalloc-10M");
}
