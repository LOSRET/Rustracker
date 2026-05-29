//! Real-tracker memory benchmark with jemalloc allocator
//!
//! Same workload as memory_tracker_bench but using tikv-jemallocator.
//! Compare CSV output with the system-allocator version to measure
//! the RSS difference at scale.
//!
//! Usage: cargo run --release --example memory_jemalloc_bench

#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod memory_bench_common;

fn main() {
    #[cfg(not(target_os = "linux"))]
    eprintln!("NOTE: jemalloc not available on this platform, using system allocator");

    memory_bench_common::run_large_bench("jemalloc-10M");
}
