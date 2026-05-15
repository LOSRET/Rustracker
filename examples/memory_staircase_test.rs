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

    drop(hash_samples);
    drop(btree_samples);

    // ─── 精细 RSS 采样测试 ─────────────────────────────────
    println!("═══════════════════════════════════════════════════════════════");
    println!("  精细 RSS 采样：定位阶梯跳变的具体来源");
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    let new_baseline = mem::rss_bytes();
    test_vec_growth_fine(new_baseline);
    test_multi_swarm_growth(new_baseline);
    test_shrink_regrow(new_baseline);
    test_production_simulation(new_baseline);

    // ─── 精确匹配生产参数 ─────────────────────────────────
    test_production_exact();

    // ─── 64 Shard vs 单 BTreeMap 对比 ────────────────────
    test_shard_vs_single();

    // ─── DashMap 多规模内存实测 ──────────────────────────
    test_dashmap_vs_btreemap_scaling();

    println!("═══════════════════════════════════════════════════════════════");
    println!("  精细采样完成");
    println!("═══════════════════════════════════════════════════════════════");
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

// ══════════════════════════════════════════════════════════════════
// 精细 RSS 采样测试：定位阶梯跳变的具体来源
// ══════════════════════════════════════════════════════════════════

/// 模拟 PackedIpv4Peers::push() 的 5 次写入模式
fn push_5writes(vec: &mut Vec<u8>, key: &[u8; 4], port: u16, flags: u8, tag: u8, last_seen: u32) {
    vec.extend_from_slice(key);                      // ① 4 字节
    vec.extend_from_slice(&port.to_be_bytes());       // ② 2 字节
    vec.push(flags);                                  // ③ 1 字节
    vec.push(tag);                                    // ④ 1 字节
    vec.extend_from_slice(&last_seen.to_be_bytes());  // ⑤ 4 字节
}

/// 优化版：一次 reserve + 5 次写入（不触发中间 realloc）
fn push_5writes_reserved(vec: &mut Vec<u8>, key: &[u8; 4], port: u16, flags: u8, tag: u8, last_seen: u32) {
    vec.reserve(12);
    vec.extend_from_slice(key);
    vec.extend_from_slice(&port.to_be_bytes());
    vec.push(flags);
    vec.push(tag);
    vec.extend_from_slice(&last_seen.to_be_bytes());
}

/// 单次写入 12 字节（对照组）
fn push_1write(vec: &mut Vec<u8>, key: &[u8; 4], port: u16, flags: u8, tag: u8, last_seen: u32) {
    let mut buf = [0u8; 12];
    buf[0..4].copy_from_slice(key);
    buf[4..6].copy_from_slice(&port.to_be_bytes());
    buf[6] = flags;
    buf[7] = tag;
    buf[8..12].copy_from_slice(&last_seen.to_be_bytes());
    vec.extend_from_slice(&buf);
}

/// 采样点
#[derive(Clone)]
struct FineSample {
    label: String,
    rss_delta: usize,  // 相对上次采样的 RSS 增量
    capacity: usize,
    len: usize,
}

fn rss_now() -> usize {
    mem::rss_bytes()
}

fn print_fine_samples(title: &str, samples: &[FineSample]) {
    println!("  {}", title);
    println!("  操作                          RSS增量      容量         数据量      标记");
    println!("  ────────────────────────────  ───────────  ──────────  ──────────  ────");
    let mut prev_rss = 0usize;
    for s in samples {
        let delta = if prev_rss == 0 { s.rss_delta } else { s.rss_delta };
        let flag = if delta > 5 * 1024 * 1024 { " ⚡>5MB" }
            else if delta > 1024 * 1024 { " ⚡>1MB" }
            else if delta > 256 * 1024 { " +256KB" }
            else { "" };
        println!("  {:30} {:>11}  {:>10}  {:>10}{}",
            s.label,
            fmt_mb(delta),
            format_with_commas(s.capacity),
            format_with_commas(s.len),
            flag,
        );
        prev_rss = s.rss_delta;
    }
    println!();
}

// ─── 测试 3: 提取实际内存后，测试 Vec<u8> 倍增精细行为 ──────────

fn test_vec_growth_fine(baseline: usize) {
    println!("═══ 测试 A: 单 Vec<u8> 写入模式对比 (0→10M 条目) ═══");
    println!();

    // 先让 allocator 稳定
    let _warm = vec![0u8; 1024 * 1024];

    // ── A1: 5 次写入 (模拟当前 PackedIpv4Peers::push) ──
    {
        let key: [u8; 4] = [1, 2, 3, 4];
        let before = rss_now();
        let mut vec: Vec<u8> = Vec::new();
        let mut samples: Vec<FineSample> = Vec::new();
        let mut last_rss = before;
        let total = 1_000_000; // 100 万条目 = 12 MB
        let step = 100_000;

        for i in 0..total {
            push_5writes(&mut vec, &key, (i as u16) % 65535, 1, 0, i as u32);
            if (i + 1) % step == 0 {
                let rss = rss_now();
                samples.push(FineSample {
                    label: format!("5-write @ {}", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: vec.capacity(),
                    len: vec.len(),
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  5-write 模式: 初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("Vec<u8> 5 次写入增长曲线", &samples);
        drop(vec);
    }

    // ── A2: 1 次写入 (对照组) ──
    {
        let key: [u8; 4] = [1, 2, 3, 4];
        let before = rss_now();
        let mut vec: Vec<u8> = Vec::new();
        let mut samples: Vec<FineSample> = Vec::new();
        let mut last_rss = before;
        let total = 1_000_000;
        let step = 100_000;

        for i in 0..total {
            push_1write(&mut vec, &key, (i as u16) % 65535, 1, 0, i as u32);
            if (i + 1) % step == 0 {
                let rss = rss_now();
                samples.push(FineSample {
                    label: format!("1-write @ {}", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: vec.capacity(),
                    len: vec.len(),
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  1-write 模式: 初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("Vec<u8> 单次写入增长曲线", &samples);
        drop(vec);
    }

    // ── A3: reserve + 5 次写入 ──
    {
        let key: [u8; 4] = [1, 2, 3, 4];
        let before = rss_now();
        let mut vec: Vec<u8> = Vec::new();
        let mut samples: Vec<FineSample> = Vec::new();
        let mut last_rss = before;
        let total = 1_000_000;
        let step = 100_000;

        for i in 0..total {
            push_5writes_reserved(&mut vec, &key, (i as u16) % 65535, 1, 0, i as u32);
            if (i + 1) % step == 0 {
                let rss = rss_now();
                samples.push(FineSample {
                    label: format!("reserve @ {}", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: vec.capacity(),
                    len: vec.len(),
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  reserve 模式: 初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("Vec<u8> reserve+5 写入增长曲线", &samples);
        drop(vec);
    }
}

// ─── 测试 B: 模拟生产环境：多 Swarm 同时增长 ──────────────────────

fn test_multi_swarm_growth(baseline: usize) {
    println!("═══ 测试 B: 多 Swarm 同时增长 (模拟生产场景) ═══");
    println!();

    let swarm_count = 50_000;
    let peers_per_swarm = 20; // 逐步增长到 20 个 peer/swarm
    let key: [u8; 4] = [1, 2, 3, 4];

    // B1: 当前代码模式 (5-write push，无预分配)
    {
        println!("  ── B1: 当前 push 模式 (5-write, 无预分配) ──");
        let before = rss_now();
        let mut swarms: Vec<Vec<u8>> = (0..swarm_count).map(|_| Vec::new()).collect();
        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();

        for round in 0..peers_per_swarm {
            for swarm in swarms.iter_mut() {
                push_5writes(swarm, &key, (round as u16) % 65535, 1, 0, round as u32);
            }
            let rss = rss_now();
            let delta = rss.saturating_sub(last_rss);
            let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
            let total_len: usize = swarms.iter().map(|v| v.len()).sum();
            samples.push(FineSample {
                label: format!("round {} (+1 peer/swarm)", round + 1),
                rss_delta: delta,
                capacity: total_cap,
                len: total_len,
            });
            last_rss = rss;
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("当前模式: 50K swarms × 20 peers 逐步增长", &samples);
        drop(swarms);
    }

    // B2: 优化模式 (reserve + 5-write)
    {
        println!("  ── B2: 优化模式 (reserve + 5-write) ──");
        let before = rss_now();
        let mut swarms: Vec<Vec<u8>> = (0..swarm_count).map(|_| Vec::new()).collect();
        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();

        for round in 0..peers_per_swarm {
            for swarm in swarms.iter_mut() {
                push_5writes_reserved(swarm, &key, (round as u16) % 65535, 1, 0, round as u32);
            }
            let rss = rss_now();
            let delta = rss.saturating_sub(last_rss);
            let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
            let total_len: usize = swarms.iter().map(|v| v.len()).sum();
            samples.push(FineSample {
                label: format!("round {} (+1 peer/swarm)", round + 1),
                rss_delta: delta,
                capacity: total_cap,
                len: total_len,
            });
            last_rss = rss;
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("优化模式: 50K swarms × 20 peers 逐步增长", &samples);
        drop(swarms);
    }
}

// ─── 测试 C: 伸缩振荡 (模拟 expire → shrink → regrow) ─────────────

fn test_shrink_regrow(baseline: usize) {
    println!("═══ 测试 C: 伸缩振荡 (expire → shrink → regrow) ═══");
    println!();

    let key: [u8; 4] = [1, 2, 3, 4];
    let swarm_count = 10_000;

    // 先增长到 1000 peers/swarm
    let before = rss_now();
    let mut swarms: Vec<Vec<u8>> = (0..swarm_count).map(|_| Vec::new()).collect();
    for _ in 0..1000 {
        for swarm in swarms.iter_mut() {
            push_5writes(swarm, &key, 6881, 1, 0, 1000);
        }
    }
    let after_grow = rss_now();
    println!("  增长到 1000 peers/swarm: {} → {} (+{})",
        fmt_mb(before), fmt_mb(after_grow), fmt_mb(after_grow.saturating_sub(before)));

    let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
    let total_len: usize = swarms.iter().map(|v| v.len()).sum();
    println!("  总容量: {}  总数据: {}", fmt_mb(total_cap), fmt_mb(total_len));

    // 模拟 expire: 删到只剩 10% (模拟 shrink_if_idle 条件)
    println!();
    println!("  ── 模拟 expire: 删除 90% peer ──");
    for swarm in swarms.iter_mut() {
        let keep = swarm.len() / 10;
        swarm.truncate(keep);
    }
    let after_expire = rss_now();
    println!("  删除后 RSS: {} (变化: {})",
        fmt_mb(after_expire), fmt_mb(after_expire.saturating_sub(after_grow)));

    // 模拟 shrink_to_fit (当前 shrink_if_idle 行为)
    println!();
    println!("  ── 模拟 shrink_to_fit ──");
    let before_shrink = rss_now();
    for swarm in swarms.iter_mut() {
        swarm.shrink_to_fit();
    }
    let after_shrink = rss_now();
    println!("  shrink 后 RSS: {} (净变化: {})",
        fmt_mb(after_shrink), fmt_mb(after_shrink.saturating_sub(before_shrink)));

    // 模拟 regrow: 重新增长到 500 peers/swarm
    println!();
    println!("  ── 模拟 regrow: 重新增长到 500 peers/swarm ──");
    let before_regrow = rss_now();
    let mut samples: Vec<FineSample> = Vec::new();
    let mut last_rss = before_regrow;

    for round in 0..450 {
        for swarm in swarms.iter_mut() {
            push_5writes(swarm, &key, 6881, 1, 0, (1000 + round) as u32);
        }
        if (round + 1) % 50 == 0 {
            let rss = rss_now();
            samples.push(FineSample {
                label: format!("regrow +{} peers", round + 1),
                rss_delta: rss.saturating_sub(last_rss),
                capacity: swarms.iter().map(|v| v.capacity()).sum(),
                len: swarms.iter().map(|v| v.len()).sum(),
            });
            last_rss = rss;
        }
    }
    let after_regrow = rss_now();
    println!("  regrow 后 RSS: {} (净增: {})",
        fmt_mb(after_regrow), fmt_mb(after_regrow.saturating_sub(before_regrow)));
    print_fine_samples("伸缩振荡 regrow 阶段", &samples);

    drop(swarms);
    let final_rss = rss_now();
    println!("  释放后 RSS: {}", fmt_mb(final_rss));
    println!();
}

// ─── 测试 D: 模拟生产场景——Zipf 分布 + 批量 announce ─────────────

fn test_production_simulation(baseline: usize) {
    println!("═══ 测试 D: 生产场景模拟 (Zipf 热门分布) ═══");
    println!();

    // 参数：模拟 50000 torrents，总 peer 数据 80-120 MB
    let torrents = 50_000;
    let total_peers = 8_000_000; // 800 万 peer ≈ 96 MB 数据

    // 用简单幂律模拟: top 1% 种子拥有 50% peer
    {
        println!("  ── D1: 当前 push 模式 (5-write) ──");
        let before = rss_now();

        // 计算每个种子的 peer 数 (幂律)
        let mut peer_counts: Vec<usize> = Vec::with_capacity(torrents);
        let mut remaining = total_peers;
        for i in 0..torrents {
            // 排名越高 peer 越多
            let rank = i + 1;
            let count = if rank <= 100 {
                // top 100: 各 20000~5000 peer
                (total_peers / 10 / rank).max(1000)
            } else if rank <= 1000 {
                (total_peers / 50 / rank).max(100)
            } else {
                (total_peers / 500 / rank).max(1)
            };
            let count = count.min(remaining);
            peer_counts.push(count);
            remaining = remaining.saturating_sub(count);
        }

        let key: [u8; 4] = [1, 2, 3, 4];
        let mut swarms: Vec<Vec<u8>> = Vec::with_capacity(torrents);
        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();
        let sample_interval = torrents / 20;

        for i in 0..torrents {
            let mut vec = Vec::new();
            let count = peer_counts[i];
            // 预分配以减少首次分配的噪声
            // vec.reserve(count * 12); // 先不预分配，看原始行为
            for j in 0..count {
                push_5writes(&mut vec, &key, (j as u16) % 65535, 1, 0, j as u32);
            }
            swarms.push(vec);

            if (i + 1) % sample_interval == 0 {
                let rss = rss_now();
                let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
                let total_len: usize = swarms.iter().map(|v| v.len()).sum();
                samples.push(FineSample {
                    label: format!("{} swarms", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: total_cap,
                    len: total_len,
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("生产模拟 (5-write): 50K swarms, 8M peers Zipf 分布", &samples);
        drop(swarms);
    }

    // D2: 1-write 模式
    {
        println!("  ── D2: 优化 push 模式 (1-write) ──");
        let before = rss_now();

        let torrents = 50_000;
        let total_peers = 8_000_000;
        let mut peer_counts: Vec<usize> = Vec::with_capacity(torrents);
        let mut remaining = total_peers;
        for i in 0..torrents {
            let rank = i + 1;
            let count = if rank <= 100 {
                (total_peers / 10 / rank).max(1000)
            } else if rank <= 1000 {
                (total_peers / 50 / rank).max(100)
            } else {
                (total_peers / 500 / rank).max(1)
            };
            let count = count.min(remaining);
            peer_counts.push(count);
            remaining = remaining.saturating_sub(count);
        }

        let key: [u8; 4] = [1, 2, 3, 4];
        let mut swarms: Vec<Vec<u8>> = Vec::with_capacity(torrents);
        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();
        let sample_interval = torrents / 20;

        for i in 0..torrents {
            let mut vec = Vec::new();
            let count = peer_counts[i];
            for j in 0..count {
                push_1write(&mut vec, &key, (j as u16) % 65535, 1, 0, j as u32);
            }
            swarms.push(vec);

            if (i + 1) % sample_interval == 0 {
                let rss = rss_now();
                let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
                let total_len: usize = swarms.iter().map(|v| v.len()).sum();
                samples.push(FineSample {
                    label: format!("{} swarms", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: total_cap,
                    len: total_len,
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("生产模拟 (1-write): 50K swarms, 8M peers Zipf 分布", &samples);
        drop(swarms);
    }
}

// ─── 测试 E: 精确匹配生产参数 ──────────────────────────────────────

fn test_production_exact() {
    println!("═══ 测试 E: 精确匹配生产参数 ═══");
    println!("  Torrents: 428,493  Peers: 666,384  Max peers/torrent: 366");
    println!();

    let torrents: usize = 428_493;
    let total_peers: usize = 666_384;
    let max_peers: usize = 366;

    // 生成符合生产数据的分布
    // 思路：top N 个种子按几何递减分配 peer，其余每个 1 peer
    let mut peer_counts: Vec<usize> = vec![1; torrents];

    // 热门种子数：估算需要多少种子来分配超额 peer
    // 如果全是 1 peer → 428,493 peers，实际有 666,384 → 多了 237,891
    let extra = total_peers.saturating_sub(torrents); // 237,891

    // 从 max_peers=366 递减到 1，看多少个热门种子能吸收超额
    // 用递减序列: 365, 364, 363, ... (每个种子多得 extra_i = peers_i - 1)
    let mut remaining = extra;
    let mut hot_count = 0;
    let mut extra_val = max_peers.min(remaining + 1) - 1; // 从 365 开始
    while remaining > 0 && hot_count < torrents {
        let give = extra_val.min(remaining);
        peer_counts[hot_count] = 1 + give;
        remaining -= give;
        hot_count += 1;
        if extra_val > 1 {
            extra_val -= 1;
        }
    }

    // 把剩余的 extra 均匀撒到前面的种子
    if remaining > 0 {
        for i in 0..hot_count.min(torrents) {
            if remaining == 0 { break; }
            let add = remaining.min(max_peers - peer_counts[i]);
            peer_counts[i] += add;
            remaining -= add;
        }
    }

    // 统计
    let actual_total: usize = peer_counts.iter().sum();
    let actual_max = peer_counts.iter().max().copied().unwrap_or(0);
    let non_zero = peer_counts.iter().filter(|&&c| c > 0).count();
    println!("  生成分布: total={} max={} non_zero={} hot={}",
        format_with_commas(actual_total), actual_max, format_with_commas(non_zero), format_with_commas(hot_count));
    println!();

    let key: [u8; 4] = [1, 2, 3, 4];

    // ── E1: 当前 push 模式 ──
    {
        println!("  ── E1: 当前 push (5-write, 无预分配) ──");
        let before = rss_now();
        let mut swarms: Vec<Vec<u8>> = Vec::with_capacity(torrents);
        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();
        let sample_interval = torrents / 25; // ~17K per sample

        for i in 0..torrents {
            let mut vec = Vec::new();
            let count = peer_counts[i];
            for j in 0..count {
                push_5writes(&mut vec, &key, (j as u16) % 65535, 1, 0, j as u32);
            }
            swarms.push(vec);

            if (i + 1) % sample_interval == 0 || i == torrents - 1 {
                let rss = rss_now();
                let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
                let total_len: usize = swarms.iter().map(|v| v.len()).sum();
                samples.push(FineSample {
                    label: format!("{} swarms", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: total_cap,
                    len: total_len,
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("当前 5-write 模式", &samples);
        drop(swarms);
    }

    // ── E2: 1-write 模式 ──
    {
        println!("  ── E2: 优化 push (1-write) ──");
        let before = rss_now();
        let mut swarms: Vec<Vec<u8>> = Vec::with_capacity(torrents);
        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();
        let sample_interval = torrents / 25;

        for i in 0..torrents {
            let mut vec = Vec::new();
            let count = peer_counts[i];
            for j in 0..count {
                push_1write(&mut vec, &key, (j as u16) % 65535, 1, 0, j as u32);
            }
            swarms.push(vec);

            if (i + 1) % sample_interval == 0 || i == torrents - 1 {
                let rss = rss_now();
                let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
                let total_len: usize = swarms.iter().map(|v| v.len()).sum();
                samples.push(FineSample {
                    label: format!("{} swarms", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: total_cap,
                    len: total_len,
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("优化 1-write 模式", &samples);
        drop(swarms);
    }

    // ── E3: 模拟 shrink + regrow (产线核心场景) ──
    {
        println!("  ── E3: 模拟 shrink→regrow 振荡 (产线核心场景) ──");
        // 重建生产状态
        let mut swarms: Vec<Vec<u8>> = Vec::with_capacity(torrents);
        for i in 0..torrents {
            let mut vec = Vec::new();
            let count = peer_counts[i];
            for j in 0..count {
                push_5writes(&mut vec, &key, (j as u16) % 65535, 1, 0, j as u32);
            }
            swarms.push(vec);
        }

        let after_build = rss_now();
        println!("  构建完成 RSS: {}", fmt_mb(after_build));

        // 模拟 expire 删除 50% 数据
        for swarm in swarms.iter_mut() {
            let keep = swarm.len() / 2;
            swarm.truncate(keep);
        }
        let after_expire = rss_now();
        println!("  expire 50% RSS: {} (变化: {})",
            fmt_mb(after_expire), fmt_mb(after_expire.saturating_sub(after_build)));

        // shrink_to_fit
        for swarm in swarms.iter_mut() {
            swarm.shrink_to_fit();
        }
        let after_shrink = rss_now();
        println!("  shrink_to_fit RSS: {} (变化: {})",
            fmt_mb(after_shrink), fmt_mb(after_shrink.saturating_sub(after_expire)));

        // regrow 回原数量
        println!();
        println!("  regrow 阶段 RSS 采样:");
        let mut last_rss = after_shrink;
        let mut samples: Vec<FineSample> = Vec::new();

        for round in 0..max_peers {
            let mut any_grown = false;
            for (i, swarm) in swarms.iter_mut().enumerate() {
                if swarm.len() / 12 < peer_counts[i] {
                    push_5writes(swarm, &key, (round as u16) % 65535, 1, 0, round as u32);
                    any_grown = true;
                }
            }
            if !any_grown { break; }

            if round % 30 == 0 || round == max_peers - 1 {
                let rss = rss_now();
                let total_cap: usize = swarms.iter().map(|v| v.capacity()).sum();
                let total_len: usize = swarms.iter().map(|v| v.len()).sum();
                samples.push(FineSample {
                    label: format!("regrow round {}", round + 1),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: total_cap,
                    len: total_len,
                });
                last_rss = rss;
            }
        }
        let final_rss = rss_now();
        println!("  regrow 完成 RSS: {} (净增: {})",
            fmt_mb(final_rss), fmt_mb(final_rss.saturating_sub(after_shrink)));
        print_fine_samples("regrow 详细", &samples);
        drop(swarms);
    }

    let released = rss_now();
    println!("  释放后 RSS: {}", fmt_mb(released));
    println!();
}

// ─── 测试 F: 真实 Zipf 热度分布 + 64 Shard BTreeMap ──────────────

fn test_shard_vs_single() {
    println!("═══ 测试 F: 真实 Zipf 分布 × 64 Shard BTreeMap ═══");
    println!();

    let torrents: usize = 428_493;
    let total_peers: usize = 666_384;
    let max_peers: usize = 366;
    let shards: usize = 64;

    // ── 生成真实 Zipf 热度分布 ──────────────────────────
    // peers(rank) = min(max_peers, 1 + floor(C / rank^α))
    // 二分搜索 α 使 sum(peers) = total_peers
    let c = (max_peers - 1) as f64; // 365

    let mut lo = 0.01f64;
    let mut hi = 2.0f64;
    let mut alpha = 0.5;
    let mut peer_counts: Vec<usize> = Vec::new();

    for _ in 0..50 {
        alpha = (lo + hi) / 2.0;
        let mut sum: usize = 0;
        peer_counts.clear();
        for rank in 1..=torrents {
            let extra = (c / (rank as f64).powf(alpha)) as usize;
            let count = 1 + extra.min(max_peers - 1);
            peer_counts.push(count);
            sum += count;
            if sum > total_peers + 5000 { break; } // early exit
        }
        if sum < total_peers {
            hi = alpha;
        } else if sum > total_peers + 100 {
            lo = alpha;
        } else {
            break;
        }
    }

    // 微调到精确总数
    let mut sum: usize = peer_counts.iter().sum();
    if sum > total_peers {
        let mut excess = sum - total_peers;
        for i in (0..peer_counts.len()).rev() {
            if excess == 0 { break; }
            if peer_counts[i] > 1 {
                let reduce = excess.min(peer_counts[i] - 1);
                peer_counts[i] -= reduce;
                excess -= reduce;
                sum -= reduce;
            }
        }
    } else if sum < total_peers {
        let mut deficit = total_peers - sum;
        for i in 0..peer_counts.len() {
            if deficit == 0 { break; }
            if peer_counts[i] < max_peers {
                let add = deficit.min(max_peers - peer_counts[i]);
                peer_counts[i] += add;
                deficit -= add;
                sum += add;
            }
        }
    }

    // 确保长度匹配
    while peer_counts.len() < torrents {
        peer_counts.push(1);
    }
    peer_counts.truncate(torrents);

    let actual_total: usize = peer_counts.iter().sum();
    let actual_max = peer_counts.iter().max().copied().unwrap_or(0);
    let hot = peer_counts.iter().filter(|&&c| c > 1).count();

    // 分布统计
    let top_10 = peer_counts.iter().take(10).copied().collect::<Vec<_>>();
    let top_100_sum: usize = peer_counts.iter().take(100).sum();
    let top_1000_sum: usize = peer_counts.iter().take(1000).sum();
    let top_10000_sum: usize = peer_counts.iter().take(10000).sum();

    println!("  Zipf α≈{:.3}  C={}", alpha, c as usize);
    println!("  total={} max={} hot(>1)={}", format_with_commas(actual_total), actual_max, format_with_commas(hot));
    println!("  top 10: {:?}", top_10);
    println!("  top 100 sum={} ({:.1}%)  top 1K sum={} ({:.1}%)  top 10K sum={} ({:.1}%)",
        format_with_commas(top_100_sum),
        top_100_sum as f64 / actual_total as f64 * 100.0,
        format_with_commas(top_1000_sum),
        top_1000_sum as f64 / actual_total as f64 * 100.0,
        format_with_commas(top_10000_sum),
        top_10000_sum as f64 / actual_total as f64 * 100.0,
    );
    println!();

    // 预生成所有 InfoHash
    let hashes: Vec<InfoHash> = (0..torrents).map(|i| InfoHash::from_u64(i as u64)).collect();

    // ── F1: 单 BTreeMap ──────────────────────────────
    {
        println!("  ── F1: 单 BTreeMap (真实 Zipf 分布) ──");
        let before = rss_now();
        let mut map: BTreeMap<InfoHash, Swarm> = BTreeMap::new();
        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();
        let sample_interval = torrents / 25;

        for i in 0..torrents {
            map.insert(hashes[i], Swarm::with_peers(peer_counts[i]));

            if (i + 1) % sample_interval == 0 || i == torrents - 1 {
                let rss = rss_now();
                let total_in_map: usize = map.values().map(|s| s.peers.len() / 12).sum();
                samples.push(FineSample {
                    label: format!("{} entries", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: map.len(),
                    len: total_in_map,
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("单 BTreeMap (Zipf)", &samples);
        drop(map);
    }

    // ── F2: 64 个 BTreeMap (shard) ────────────────────
    {
        println!("  ── F2: 64 Shard BTreeMap (真实 Zipf 分布) ──");
        let before = rss_now();
        let mut shard_maps: Vec<BTreeMap<InfoHash, Swarm>> = (0..shards)
            .map(|_| BTreeMap::new())
            .collect();

        let mut last_rss = before;
        let mut samples: Vec<FineSample> = Vec::new();
        let sample_interval = torrents / 25;

        for i in 0..torrents {
            let shard = i % shards;
            shard_maps[shard].insert(hashes[i], Swarm::with_peers(peer_counts[i]));

            if (i + 1) % sample_interval == 0 || i == torrents - 1 {
                let rss = rss_now();
                let total_in_map: usize = shard_maps.iter()
                    .flat_map(|m| m.values())
                    .map(|s| s.peers.len() / 12)
                    .sum();
                samples.push(FineSample {
                    label: format!("{} total", format_with_commas(i + 1)),
                    rss_delta: rss.saturating_sub(last_rss),
                    capacity: i + 1,
                    len: total_in_map,
                });
                last_rss = rss;
            }
        }
        let after = rss_now();
        println!("  初始 {} → 最终 {} (净增 {})", fmt_mb(before), fmt_mb(after), fmt_mb(after.saturating_sub(before)));
        print_fine_samples("64 Shard BTreeMap (Zipf)", &samples);
        drop(shard_maps);
    }

    let released = rss_now();
    println!("  释放后 RSS: {}", fmt_mb(released));
    println!();
}

// ─── 测试 G: DashMap vs BTreeMap 多规模内存实测 ────────────────────

fn test_dashmap_vs_btreemap_scaling() {
    println!("═══ 测试 G: DashMap vs BTreeMap 多规模内存对比 ═══");
    println!();

    let scales = [1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000, 6_000_000, 7_000_000, 8_000_000, 9_000_000, 10_000_000];
    let peers_per = 1; // 1 peer/torrent 省内存，突出容器开销

    println!("  {:>9}  {:>10}  {:>10}  {:>10}  {:>10}", "torrents", "BTreeMap", "DashMap", "差额", "比例");
    println!("  {}  {}  {}  {}  {}", "─".repeat(9), "─".repeat(10), "─".repeat(10), "─".repeat(10), "─".repeat(10));

    for &n in &scales {
        let hashes: Vec<InfoHash> = (0..n).map(|i| InfoHash::from_u64(i as u64)).collect();
        let hashes_ref = &hashes;

        // ═══ BTreeMap ═══
        let before = rss_now();
        let mut bt: BTreeMap<InfoHash, Swarm> = BTreeMap::new();
        for i in 0..n {
            bt.insert(hashes_ref[i], Swarm::with_peers(peers_per));
        }
        let bt_rss = rss_now().saturating_sub(before);
        drop(bt);

        // ═══ DashMap ═══
        let before = rss_now();
        let dm: dashmap::DashMap<InfoHash, Swarm> = dashmap::DashMap::with_shard_amount(256);
        for i in 0..n {
            dm.insert(hashes_ref[i], Swarm::with_peers(peers_per));
        }
        let dm_rss = rss_now().saturating_sub(before);
        drop(dm);

        let diff = dm_rss as i64 - bt_rss as i64;
        let pct = if bt_rss > 0 { diff as f64 / bt_rss as f64 * 100.0 } else { 0.0 };
        let abs_diff = diff.unsigned_abs() as usize;
        let sign = if diff >= 0 { "+" } else { "-" };
        println!("  {:>9}  {:>10}  {:>10}  {}{:>9}  {:>+9.0}%",
            format_with_commas(n),
            fmt_mb(bt_rss),
            fmt_mb(dm_rss),
            sign,
            fmt_mb(abs_diff),
            pct,
        );
    }
    println!();
}
