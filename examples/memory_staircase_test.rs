//! 内存阶梯爬坡测试
//!
//! 测试 BTreeMap 是否消除了 HashMap 容量翻倍导致的内存阶梯爬坡。
//!
//! 运行方式：
//!   cargo run --release --example memory_staircase_test
//!
//! 输出说明：
//!   每插入 N 个种子后打印 RSS (物理内存)，观察增长曲线。
//!   BTreeMap: 应该是平滑线性增长
//!   HashMap (对照组): 会在 2^n 容量边界出现跳变

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

// ─── Windows RSS 测量 ───────────────────────────────────────────────

#[cfg(windows)]
mod mem {
    use std::mem;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_nonpaged_pool_usage: usize,
        quota_nonpaged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "psapi")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessMemoryInfo(
            process: isize,
            ppsmemcounters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    /// 返回当前进程的 RSS（物理内存，字节）
    pub fn rss_bytes() -> usize {
        unsafe {
            let mut info: ProcessMemoryCounters = mem::zeroed();
            info.cb = mem::size_of::<ProcessMemoryCounters>() as u32;
            GetProcessMemoryInfo(GetCurrentProcess(), &mut info, info.cb);
            info.working_set_size
        }
    }
}

#[cfg(unix)]
mod mem {
    use std::fs;

    /// 返回当前进程的 RSS（物理内存，字节）
    pub fn rss_bytes() -> usize {
        let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let kb: usize = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                return kb * 1024;
            }
        }
        0
    }
}

// ─── Swarm 结构（与 tracker.rs 一致）─────────────────────────────────

#[derive(Default)]
struct Swarm {
    complete: usize,
    incomplete: usize,
    downloaded: u64,
    /// 模拟 peer 数据：每个 peer 12 字节（IPv4 紧凑存储）
    peers: Vec<u8>,
}

impl Swarm {
    fn with_peers(n: usize) -> Self {
        let mut s = Self::default();
        s.peers.resize(n * 12, 0x42); // 每 peer 12 字节
        s.incomplete = n;
        s
    }
}

// ─── 测试配置 ───────────────────────────────────────────────────────

struct Config {
    total_torrents: usize,
    step: usize,         // 每 step 个种子打印一次
    peers_per_torrent: usize,
}

impl Config {
    fn default_test() -> Self {
        Self {
            total_torrents: 3_000_000,
            step: 50_000,
            peers_per_torrent: 5, // 平均 peer 数（大部分种子 peer 很少）
        }
    }
}

// ─── InfoHash（20 字节）────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct InfoHash([u8; 20]);

impl InfoHash {
    fn from_u64(n: u64) -> Self {
        let mut h = [0u8; 20];
        h[0..8].copy_from_slice(&n.to_le_bytes());
        // 让哈希分散一些
        h[8] = (n >> 8) as u8;
        h[9] = (n >> 16) as u8;
        h[10] = (n >> 24) as u8;
        InfoHash(h)
    }
}

// ─── 主测试逻辑 ────────────────────────────────────────────────────

fn main() {
    let cfg = Config::default_test();

    println!("═══════════════════════════════════════════════════════════════");
    println!("  内存阶梯爬坡测试");
    println!("  总种子数: {}  每种子平均 peer: {}", cfg.total_torrents, cfg.peers_per_torrent);
    println!("  每 {} 种子采样一次内存", cfg.step);
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    // 基线
    let baseline = mem::rss_bytes();
    println!("基线 RSS: {}", fmt_mb(baseline));
    println!();

    // ─── 测试 1: HashMap（对照组，模拟旧行为）────────────────────
    println!("─── HashMap (对照组，有阶梯跳变) ───────────────────────");
    let (hash_samples, hash_duration) = test_hashmap(&cfg, baseline);
    print_samples(&hash_samples);
    let hash_max_jump = max_jump(&hash_samples);
    println!("最大单步跳变: {} (应出现明显阶梯)", fmt_mb(hash_max_jump));
    println!("耗时: {:.2}s", hash_duration.as_secs_f64());
    println!();

    // ─── 测试 2: BTreeMap（目标）────────────────────────────────
    println!("─── BTreeMap (目标，应为平滑线性) ──────────────────────");
    let (btree_samples, btree_duration) = test_btreemap(&cfg, baseline);
    print_samples(&btree_samples);
    let btree_max_jump = max_jump(&btree_samples);
    println!("最大单步跳变: {} (应为平滑，无明显阶梯)", fmt_mb(btree_max_jump));
    println!("耗时: {:.2}s", btree_duration.as_secs_f64());
    println!();

    // ─── 对比总结 ─────────────────────────────────────────────
    println!("═══════════════════════════════════════════════════════════════");
    println!("  对比总结");
    println!("═══════════════════════════════════════════════════════════════");
    let hash_final = hash_samples.last().map(|s| s.rss).unwrap_or(0);
    let btree_final = btree_samples.last().map(|s| s.rss).unwrap_or(0);
    println!("最终内存  HashMap: {}  BTreeMap: {}  节省: {}",
        fmt_mb(hash_final), fmt_mb(btree_final),
        fmt_mb(hash_final.saturating_sub(btree_final)));
    println!("最大跳变  HashMap: {}  BTreeMap: {}",
        fmt_mb(hash_max_jump), fmt_mb(btree_max_jump));
    let btree_smooth = btree_max_jump < hash_max_jump / 3;
    println!("阶梯消除: {}", if btree_smooth { "✅ 是，BTreeMap 增长平滑" } else { "⚠️ 仍需验证" });
    println!();
}

// ─── 采样结构 ──────────────────────────────────────────────────────

struct Sample {
    count: usize,   // 已插入种子数
    rss: usize,     // 当前 RSS (字节)
    capacity: usize, // 容器容量（HashMap 用 capacity，BTreeMap 用 len）
}

fn test_hashmap(cfg: &Config, baseline: usize) -> (Vec<Sample>, Duration) {
    let mut map: HashMap<InfoHash, Swarm> = HashMap::new();
    let mut samples = Vec::new();
    let start = Instant::now();

    for i in 0..cfg.total_torrents {
        let hash = InfoHash::from_u64(i as u64);
        map.insert(hash, Swarm::with_peers(cfg.peers_per_torrent));

        if (i + 1) % cfg.step == 0 {
            samples.push(Sample {
                count: i + 1,
                rss: mem::rss_bytes().saturating_sub(baseline),
                capacity: map.capacity(),
            });
        }
    }

    (samples, start.elapsed())
}

fn test_btreemap(cfg: &Config, baseline: usize) -> (Vec<Sample>, Duration) {
    let mut map: BTreeMap<InfoHash, Swarm> = BTreeMap::new();
    let mut samples = Vec::new();
    let start = Instant::now();

    for i in 0..cfg.total_torrents {
        let hash = InfoHash::from_u64(i as u64);
        map.insert(hash, Swarm::with_peers(cfg.peers_per_torrent));

        if (i + 1) % cfg.step == 0 {
            samples.push(Sample {
                count: i + 1,
                rss: mem::rss_bytes().saturating_sub(baseline),
                capacity: map.len(), // BTreeMap 没有 capacity，用 len
            });
        }
    }

    (samples, start.elapsed())
}

// ─── 输出格式化 ────────────────────────────────────────────────────

fn print_samples(samples: &[Sample]) {
    println!("  种子数       RSS          ΔRSS      容量/len");
    println!("  ─────────  ───────────  ─────────  ─────────");

    let mut prev_rss = 0usize;
    for s in samples {
        let delta = if prev_rss == 0 { 0 } else { s.rss.saturating_sub(prev_rss) };
        let delta_str = if prev_rss == 0 {
            "    —    ".to_string()
        } else {
            format!("+{}", fmt_mb(delta))
        };
        // 跳变超过 5MB 标记
        let flag = if delta > 5 * 1024 * 1024 { "  ⚡跳变!" } else { "" };
        println!("  {:>8}  {:>11}  {:>9}  {:>9}{}",
            format_with_commas(s.count),
            fmt_mb(s.rss),
            delta_str,
            format_with_commas(s.capacity),
            flag,
        );
        prev_rss = s.rss;
    }
}

fn max_jump(samples: &[Sample]) -> usize {
    let mut max_delta = 0usize;
    let mut prev = 0usize;
    for s in samples {
        if prev > 0 {
            let delta = s.rss.saturating_sub(prev);
            if delta > max_delta {
                max_delta = delta;
            }
        }
        prev = s.rss;
    }
    max_delta
}

fn fmt_mb(bytes: usize) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 0.01 {
        "0 MB".to_string()
    } else {
        format!("{:.1} MB", mb)
    }
}

fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
