# Rustracker Code Wiki

> 版本：0.2.6 | 语言：Rust 2021 Edition | 许可证：MIT

---

## 目录

1. [项目概述](#1-项目概述)
2. [整体架构](#2-整体架构)
3. [目录结构](#3-目录结构)
4. [模块详解](#4-模块详解)
   - 4.1 [core — 核心追踪引擎](#41-core--核心追踪引擎)
   - 4.2 [protocol — BitTorrent 协议层](#42-protocol--bittorrent-协议层)
   - 4.3 [server — HTTP 服务层](#43-server--http-服务层)
5. [关键数据结构](#5-关键数据结构)
6. [关键函数与算法](#6-关键函数与算法)
7. [模块依赖关系](#7-模块依赖关系)
8. [数据流分析](#8-数据流分析)
9. [并发模型与分片策略](#9-并发模型与分片策略)
10. [依赖清单](#10-依赖清单)
11. [构建与运行](#11-构建与运行)
12. [测试体系](#12-测试体系)
13. [Feature Flags](#13-feature-flags)
14. [部署与运维](#14-部署与运维)

---

## 1. 项目概述

**Rustracker** 是一个用 Rust 编写的轻量级、高性能 HTTP BitTorrent Tracker。它实现了 BEP 3 规范的 `announce` 和 `scrape` 端点，同时内置实时 Web 监控面板，支持 102 种 BitTorrent 客户端识别、种子黑名单热重载、趋势数据持久化等运维特性。

核心设计目标：
- **高吞吐**：64 分片并发 Tracker 池，每分片独立 `RwLock`，最小化锁争用
- **低内存**：Peer 采用紧凑二进制存储（IPv4 6 字节/peer，IPv6 18 字节/peer），无逐 peer 堆分配
- **零外部依赖运行**：单一二进制文件，无需数据库或其他服务

---

## 2. 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                       Axum HTTP Server                          │
│  /announce  /scrape  /healthz  /  /style.css  /app.js  /api/*  │
└──────────────────────────┬──────────────────────────────────────┘
                           │
              ┌────────────┼────────────────┐
              ▼            ▼                ▼
┌──────────────────┐ ┌──────────────┐ ┌──────────────────────┐
│   TrackerPool    │ │  TrendStore  │ │  Blacklist Watcher   │
│ (64 sharded      │ │ (7天JSONL    │ │ (5秒文件轮询热重载)   │
│  RwLock<Tracker>)│ │  历史数据)    │ │  HashSet<InfoHash>   │
│                  │ │              │ └──────────────────────┘
│  每个分片:        │ │ 10分钟采样    │
│  BTreeMap<       │ │ 内存环形缓冲  │
│   InfoHash,      │ │ +可选JSONL    │
│   Swarm          │ │  持久化       │
│  >               │ │              │
└──────┬───────────┘ └──────────────┘
       │
       ▼
┌──────────────────────────────────────────────────┐
│                    Swarm                          │
│  ┌─────────────────┐  ┌────────────────────┐     │
│  │ PackedIpv4Peers │  │ PackedIpv6Peers    │     │
│  │ (12B/entry)     │  │ (24B/entry)        │     │
│  │ IP+Port+Flag+   │  │ IP+Port+Flag+      │     │
│  │ Tag+LastSeen    │  │ Tag+LastSeen       │     │
│  └─────────────────┘  └────────────────────┘     │
│  complete: u32   downloaded: u32                  │
└──────────────────────────────────────────────────┘
```

**三层架构**：

| 层 | 模块 | 职责 | 依赖方向 |
|---|---|---|---|
| 核心层 | `core` | Tracker 逻辑、Swarm 管理、Peer 存储、计数器 | 无 I/O 依赖 |
| 协议层 | `protocol` | Bencode 编码、Announce/Scrape 解析、客户端识别 | 依赖 `core` 类型 |
| 服务层 | `server` | HTTP 路由、请求处理、黑名单、趋势数据 | 依赖 `core` + `protocol` |

---

## 3. 目录结构

```
rustracker/
├── Cargo.toml                          # 项目清单与依赖声明
├── LICENSE                             # MIT 许可证
├── README.md / README-zh.md            # 项目文档
├── install-linux.sh                    # Linux systemd 安装脚本
│
├── src/                                # Rust 源代码
│   ├── main.rs                         # 入口：CLI 解析、Tokio 运行时、优雅关闭
│   ├── lib.rs                          # 库根：重新导出 core、protocol、server 模块
│   │
│   ├── core.rs                         # 核心模块声明
│   ├── core/                           # 核心追踪引擎（纯逻辑，无 I/O）
│   │   ├── types.rs                    # 核心类型：InfoHash、PeerId、PeerState 等
│   │   ├── tracker.rs                  # Tracker、AnnounceInput/Output、TrackerSnapshot
│   │   ├── swarm.rs                    # Swarm：紧凑二进制 Peer 存储、随机选择
│   │   ├── topk.rs                     # 四维 Top-K 排名算法
│   │   └── counters.rs                 # 增量计数器，O(1) 快照
│   │
│   ├── protocol.rs                     # 协议模块声明
│   ├── protocol/                       # BitTorrent 协议编解码（无网络依赖）
│   │   ├── bencode.rs                  # 轻量 Bencode 编码器
│   │   ├── announce.rs                 # Announce/Scrape 查询解析与响应构建
│   │   └── client_id.rs                # 102 种客户端 Peer ID 识别
│   │
│   ├── server.rs                       # 服务层模块声明 + TrackerPool + AppState
│   └── server/                         # HTTP 服务层
│       ├── handlers.rs                 # 请求处理器：announce、scrape、healthz、API
│       ├── blacklist.rs                # 种子黑名单文件解析
│       └── trends.rs                   # 趋势数据采集、缓存、JSONL 持久化
│
├── assets/                             # Web 监控面板静态文件
│   ├── index.html                      # 生产版 HTML（内联 CSS/JS）
│   ├── style.css                       # 面板样式
│   ├── app.js                          # 面板逻辑：ECharts 图表、i18n、API 调用
│   ├── contact.html                    # 联系信息 HTML（personal-contact feature 注入）
│   └── translate.svg                   # 翻译图标
│
├── examples/                           # 性能测试与基准工具
│   ├── announce_load.rs                # Announce 负载测试
│   ├── load_test.rs                    # 通用负载测试
│   ├── memory_ci_compare.rs            # CI 内存对比测试
│   ├── memory_staircase_test.rs        # 内存阶梯测试
│   ├── memory_tracker_bench.rs         # Tracker 内存基准测试
│   ├── memory_tracker_btree.rs         # BTree 内存基准测试
│   ├── rps_bench.rs                    # RPS 基准测试
│   └── unified_bench.rs               # 统一基准测试套件
│
├── tests/                              # 集成测试
│   └── tracker_http.rs                 # HTTP 端点集成测试
│
└── .cargo/
    └── config.toml                     # 交叉编译配置（zig-cc musl 静态链接）
```

---

## 4. 模块详解

### 4.1 `core` — 核心追踪引擎

纯逻辑层，不涉及任何 I/O 操作，可独立测试。

#### 4.1.1 `core/types.rs` — 核心类型定义

| 类型 | 说明 |
|------|------|
| `InfoHash([u8; 20])` | 种子信息哈希，20 字节，实现 `Hash`/`Ord`/`Display`（十六进制输出） |
| `PeerId([u8; 20])` | Peer 标识符，20 字节 |
| `AnnounceEvent` | 枚举：`Started` / `Completed` / `Stopped` / `Empty` |
| `Ipv4PeerKey` | IPv4 Peer 键：`ip: [u8; 4]` + `port: u16`，支持 `compact()` 输出 6 字节 |
| `Ipv6PeerKey` | IPv6 Peer 键：`ip: [u8; 16]` + `port: u16`，支持 `compact()` 输出 18 字节 |
| `PeerState` | Peer 状态：`complete: bool` + `last_seen_secs: u32` + `client_tag: u8` |
| `TorrentStats` | 种子统计：`complete` / `downloaded` / `incomplete` |
| `PeerContact` | Peer 联系方式：`ip: IpAddr` + `port: u16`，支持紧凑编码 |

关键方法：
- `InfoHash::from_hex(str) -> Option<InfoHash>`：从 40 字符十六进制字符串解析
- `Ipv4PeerKey::compact() -> [u8; 6]`：4 字节 IP + 2 字节端口（大端序）
- `Ipv6PeerKey::compact() -> [u8; 18]`：16 字节 IP + 2 字节端口（大端序）
- `PeerContact::localhost(peer_id, port) -> PeerContact`：构造本地回环地址

#### 4.1.2 `core/tracker.rs` — Tracker 核心逻辑

`Tracker` 是单分片的核心追踪器，管理一组 `BTreeMap<InfoHash, Swarm>`。

**核心结构体**：

```rust
struct Tracker {
    interval: Duration,          // Announce 间隔
    peer_timeout: Duration,      // Peer 过期超时
    started_at: Instant,         // 启动时间
    next_expire_at: Instant,     // 下次过期清扫时间
    swarms: BTreeMap<InfoHash, Swarm>,  // 种子 -> Swarm 映射
    client_counts: Vec<(u8, u64)>,      // 客户端标签 -> 计数
    counters: TrackerCounters,          // 增量计数器
}
```

**输入/输出结构体**：

| 结构体 | 用途 |
|--------|------|
| `AnnounceInput` | Announce 请求输入：info_hash、peer_id、ip、port、uploaded/downloaded/left、event、numwant、client_tag |
| `AnnounceOutput` | Announce 响应输出：interval、complete、incomplete、downloaded、peers（Vec<PeerContact>） |
| `TrackerSnapshot` | Tracker 全局快照：interval、peer_timeout、totals、clients |
| `TrackerTotals` | 全局统计：torrents、peers、seeders、leechers、downloaded |

**核心方法**：

| 方法 | 说明 |
|------|------|
| `Tracker::new(interval, peer_timeout)` | 创建新 Tracker |
| `Tracker::announce(input, now)` | 处理 Announce 请求，更新 Swarm 状态，返回 Peer 列表 |
| `Tracker::scrape(info_hashes)` | 查询多个种子的统计信息 |
| `Tracker::snapshot()` | 获取 O(1) 全局快照 |
| `Tracker::expire_due(now)` | 检查并执行过期 Peer 清扫 |
| `Tracker::top_torrents_all(limit)` | 获取四维 Top-K 排名 |
| `Tracker::client_distribution()` | 获取客户端分布数据 |

**Announce 处理流程**：
1. 根据 `info_hash` 获取或创建 Swarm
2. 根据 `event` 类型处理：
   - `Stopped`：移除 Peer，递减计数器
   - `Completed`：更新 Peer 状态，递增 downloaded 计数
   - `Started`/`Empty`：插入或更新 Peer
3. 从 Swarm 中随机选择 Peer（排除请求者自身）
4. 返回带抖动的 interval 和 Peer 列表

**Interval 抖动机制**：
- 基础 interval ±10% 随机偏移
- 使用 FNV-1a 变体哈希（info_hash + peer_id + 时间窗口）作为种子
- 目的：避免大量 Peer 同时重新 Announce 造成惊群效应

#### 4.1.3 `core/swarm.rs` — Swarm 与紧凑 Peer 存储

**Swarm** 是单个种子的 Peer 集合，采用紧凑二进制格式存储 Peer 信息。

**存储格式**：

| 类型 | 每条目字节数 | 布局 |
|------|------------|------|
| IPv4 | 12 字节 | `IP(4B) + Port(2B) + Flags(1B) + ClientTag(1B) + LastSeen(4B)` |
| IPv6 | 24 字节 | `IP(16B) + Port(2B) + Flags(1B) + ClientTag(1B) + LastSeen(4B)` |

**核心结构体**：

| 结构体 | 说明 |
|--------|------|
| `PackedIpv4Peers` | IPv4 Peer 紧凑存储，底层为 `Vec<u8>` |
| `PackedIpv6Peers` | IPv6 Peer 紧凑存储，底层为 `Vec<u8>` |
| `PeerEndpoint` | Peer 端点枚举：`V4(Ipv4PeerKey)` / `V6(Ipv6PeerKey)` |
| `Swarm` | 单个种子的 Peer 集合：ipv4_peers + ipv6_peers + complete + downloaded |
| `Rng` | XorShift 伪随机数生成器 |

**Swarm 核心方法**：

| 方法 | 说明 |
|------|------|
| `Swarm::upsert_peer(endpoint, peer)` | 插入或更新 Peer，返回 `PeerUpsert` 增量信息 |
| `Swarm::remove_peer_tag(endpoint)` | 移除 Peer，返回 `PeerRemoval` 增量信息 |
| `Swarm::expire(now_secs, timeout_secs)` | 清扫过期 Peer，返回 `ExpireResult` |
| `Swarm::contacts_excluding(endpoint, limit, seed)` | 随机选择 Peer 列表（排除请求者） |
| `Swarm::stats()` | 返回 `TorrentStats` |
| `PackedIpv4Peers::select_random(count, rng, exclude, contacts)` | 定点均匀间距随机选择（OpenTracker 风格） |

**Peer 选择算法**（OpenTracker 风格定点均匀间距随机选择）：
1. 将总数左移至 62 位以上（定点化）
2. 计算步长 `shifted_step = shifted_total / count`
3. 从随机起始位置开始，按步长等间距跳跃选择
4. 每步添加小随机偏移，避免完全确定性
5. 时间窗口变化时种子变化，实现 Peer 轮换

**IPv4/IPv6 分配策略**（`allocate_v4_v6`）：
- 保证每种地址族至少 1/4 的名额（如果可用）
- 剩余名额按两种地址族的 Peer 数量比例分配
- 使用 1024 倍定点数计算百分比，避免浮点运算

#### 4.1.4 `core/topk.rs` — 四维 Top-K 排名

单次遍历所有 Swarm，同时计算四个维度的 Top-K 排名。

**维度**：peers（做种+下载）、seeders（做种数）、leechers（下载数）、downloaded（完成数）

**算法**：
- 使用 4 个最小堆（`BinaryHeap<Reverse<...>>`）
- 遍历每个 Swarm 时，快速判断是否所有堆都已满且当前种子低于所有阈值（跳过优化）
- 每个堆独立维护 `min_key` 阈值，实现 O(N log K) 复杂度

**核心函数**：

| 函数 | 说明 |
|------|------|
| `top_torrents_all(swarms, limit)` | 单次遍历计算四维 Top-K |
| `try_heap_insert(heap, min_key, limit, key, ...)` | 尝试向最小堆插入元素 |
| `drain_heap_by(heap, sort_field)` | 从堆中提取并按指定维度降序排序 |

#### 4.1.5 `core/counters.rs` — 增量计数器

维护 Tracker 级别的运行总计，使 `snapshot()` 操作为 O(1)。

**核心结构体**：

| 结构体 | 说明 |
|--------|------|
| `TrackerCounters` | 增量计数器：torrents、peers、seeders、downloaded |
| `PeerUpsert` | Peer 更新增量：is_new_peer、was_complete、now_complete、old_tag |
| `PeerRemoval` | Peer 移除增量：tag、was_complete |
| `ExpireResult` | 过期清扫增量：tags、removed_peers、removed_complete |

**计数器更新规则**：
- `apply_upsert`：新 Peer 递增 peers；状态变化调整 seeders
- `apply_removal`：递减 peers；如果是 seeder 则递减 seeders
- `apply_expire`：递减 peers/seeders/torrents/downloaded
- Debug 模式下 `verify()` 方法会全量遍历验证计数器一致性

---

### 4.2 `protocol` — BitTorrent 协议层

纯协议编解码层，不依赖网络 I/O。

#### 4.2.1 `protocol/bencode.rs` — Bencode 编码器

轻量级 Bencode（BitTorrent 编码格式）序列化器，零外部依赖。

**`Value` 枚举**：

| 变体 | 编码格式 | 示例 |
|------|---------|------|
| `Bytes(Vec<u8>)` | `<len>:<bytes>` | `4:spam` |
| `Integer(i64)` | `i<value>e` | `i42e` |
| `List(Vec<Value>)` | `l<items>e` | `l4:spam4:eggse` |
| `Dictionary(BTreeMap<Vec<u8>, Value>)` | `d<key><value>...e` | `d3:cow3:mooe` |

**辅助函数**：
- `Value::bytes()` / `Value::string()` / `Value::integer()` / `Value::dictionary()`：构造器
- `Value::encode() -> Vec<u8>`：编码为字节流
- `failure(message) -> Vec<u8>`：构造 Bencode 失败响应

**注意**：Dictionary 使用 `BTreeMap`，键自动按字典序排序，符合 Bencode 规范。

#### 4.2.2 `protocol/announce.rs` — Announce/Scrape 协议处理

**核心结构体**：

| 结构体 | 说明 |
|--------|------|
| `AnnounceQuery` | 解析后的 Announce 请求参数 |
| `ScrapeQuery` | 解析后的 Scrape 请求参数（支持多个 info_hash） |
| `ProtocolError` | 协议错误：`Missing(&str)` / `Invalid(&str)` |

**核心函数**：

| 函数 | 说明 |
|------|------|
| `parse_announce_query(raw_query)` | 解析 Announce URL 查询字符串 |
| `parse_scrape_query(raw_query)` | 解析 Scrape URL 查询字符串 |
| `announce_response(output, compact)` | 构建 Bencode Announce 响应 |
| `scrape_response(stats)` | 构建 Bencode Scrape 响应 |
| `peer_ip(query_ip, remote_addr)` | 确定 Peer IP（优先使用查询参数，其次远程地址） |
| `compact_peers(peers)` | 编码 IPv4 紧凑 Peer 列表 |
| `compact_peers6(peers)` | 编码 IPv6 紧凑 Peer 列表 |

**Announce 参数解析**：
- 必填：`info_hash`（20 字节 URL 编码）、`peer_id`（20 字节 URL 编码）、`port`
- 可选：`uploaded`、`downloaded`、`left`、`event`、`numwant`（默认 100，最大 400）、`compact`（默认 1）、`ip`

**响应格式**：
- Compact 模式：`peers` 为二进制紧凑格式，`peers6` 为 IPv6 紧凑格式
- Dictionary 模式：`peers` 为字典列表（ip + port）

#### 4.2.3 `protocol/client_id.rs` — 客户端识别

从 Peer ID 前缀识别 BitTorrent 客户端类型，支持 102 种客户端。

**识别策略**：
1. **Azureus 风格**（`-XX####-` 格式）：使用编译时生成的 256×256 查找表 `AZUREUS_TABLE`
   - 索引方式：`AZUREUS_TABLE[peer_id[1]][peer_id[2]]`
   - 64 KB 只读内存，零运行时开销
2. **非 Azureus 前缀**：线性扫描 `NON_AZUREUS` 静态数组
   - 包含 Aria2（`A2-`）、FDM（`FD6`）、Mainline（`M6-`/`M7-`/`M8-`）、Tixati（`TIX`）等

**核心函数**：
- `identify(peer_id: &[u8; 20]) -> u8`：返回客户端标签字节（0 = 未知）
- `client_name(tag: u8) -> &'static str`：将标签字节转换为可读名称

**客户端常量**：`UNKNOWN(0)` 到 `QVOD(101)`，共 102 种客户端标签。

---

### 4.3 `server` — HTTP 服务层

基于 Axum + Tokio 的 HTTP 服务层，整合所有组件。

#### 4.3.1 `server.rs` — 应用状态与路由

**核心结构体**：

| 结构体 | 说明 |
|--------|------|
| `AppState` | 应用共享状态：tracker（Arc<TrackerPool>）、trends（Arc<RwLock<TrendStore>>）、blacklist（Arc<RwLock<Arc<HashSet<InfoHash>>>>）、versioned_index（dashboard feature） |
| `TrackerPool` | 64 分片 Tracker 池：`Vec<RwLock<Tracker>>` |

**AppState 构造方法**：

| 方法 | 说明 |
|------|------|
| `AppState::new(tracker, trends_file)` | 单分片模式 |
| `AppState::sharded(interval, peer_timeout, shards)` | 多分片模式 |
| `AppState::sharded_with_blacklist_file(...)` | 完整构造：分片 + 黑名单文件 + 趋势文件 |

**TrackerPool 核心方法**：

| 方法 | 说明 |
|------|------|
| `TrackerPool::announce(info_hash, input, now)` | 根据 info_hash 哈希到分片，获取写锁执行 announce |
| `TrackerPool::scrape(info_hashes)` | 按 info_hash 分组到分片，并行读取各分片统计 |
| `TrackerPool::snapshot()` | 遍历所有分片，聚合快照数据 |
| `TrackerPool::top_torrents_all(limit)` | 遍历所有分片，合并 Top-K 堆 |
| `TrackerPool::expire_due(now)` | 尝试获取各分片写锁执行过期清扫（非阻塞） |

**分片路由**：使用 `DefaultHasher` 对 `InfoHash` 哈希后取模确定分片索引。

**后台任务**：
- **过期清扫**：每 1 秒执行一次 `expire_due`
- **趋势采样**：每 10 分钟采样一次快照数据
- **黑名单监视**：每 5 秒检查文件修改时间，变更时重新加载

**路由定义**（`router` 函数）：

| 路径 | 处理器 | 说明 |
|------|--------|------|
| `/announce` | `handlers::announce` | BitTorrent Announce |
| `/scrape` | `handlers::scrape` | BitTorrent Scrape |
| `/healthz` | `handlers::healthz` | 健康检查 |
| `/api/stats` | `handlers::stats` | JSON 统计数据 |
| `/api/trends` | `handlers::trends` | JSON 趋势历史 |
| `/api/clients` | `handlers::clients` | JSON 客户端分布 |
| `/api/top100` | `handlers::top100` | JSON Top 100 种子 |
| `/` | `handlers::index` | Dashboard HTML（需 `dashboard` feature） |
| `/style.css` | `handlers::style` | CSS（需 `dashboard` feature） |
| `/app.js` | `handlers::app_js` | JavaScript（需 `dashboard` feature） |

#### 4.3.2 `server/handlers.rs` — HTTP 请求处理器

**Announce 处理流程**：
1. 解析 URL 查询参数为 `AnnounceQuery`
2. 检查黑名单（`blacklist.read().await.contains()`）
3. 确定 Peer IP：优先 `CF-Connecting-IP` 头 → 查询参数 `ip` → 远程地址
4. 识别客户端类型（`client_id::identify()`）
5. 构造 `AnnounceInput`，调用 `tracker.announce()`
6. 构建 Bencode 响应

**Scrape 处理流程**：
1. 解析 URL 查询参数为 `ScrapeQuery`
2. 过滤掉黑名单中的 info_hash
3. 调用 `tracker.scrape()` 获取统计
4. 构建 Bencode 响应

**Dashboard 处理**（`dashboard` feature）：
- 静态资源通过 `include_str!` 编译时嵌入
- CSS/JS 设置 1 小时缓存（`Cache-Control: public, max-age=3600`）
- HTML 中的资源引用添加 FNV-1a 哈希版本号，实现缓存失效

**Top100 处理**：
- 支持 `sort` 查询参数：`peers`/`seeders`/`leechers`/`downloaded`
- 支持 `limit` 查询参数（默认 100，最大 500）
- 返回四个维度的排序结果

#### 4.3.3 `server/blacklist.rs` — 黑名单管理

**功能**：
- 解析黑名单文本文件：每行一个 40 字符十六进制 info_hash
- 支持 `#` 开头的注释行和空行
- 无效行记录警告日志并跳过

**核心函数**：
- `load_blacklist(path: &Path) -> anyhow::Result<HashSet<InfoHash>>`

**热重载机制**：
- 每 5 秒检查文件修改时间（`mtime`）
- 修改时间变化时重新加载
- 使用 `Arc<HashSet<InfoHash>>` 实现读无锁、写原子替换

#### 4.3.4 `server/trends.rs` — 趋势数据管理

**核心结构体**：

| 结构体 | 说明 |
|--------|------|
| `TrendStore` | 趋势数据存储：points（趋势点）、client_points（客户端分布点）、缓存 |
| `StatsResponse` | `/api/stats` JSON 响应 |
| `TrendsResponse` | `/api/trends` JSON 响应 |
| `ClientsResponse` | `/api/clients` JSON 响应 |
| `TrendPointResponse` | 单个趋势数据点 |
| `ClientTrendPoint` | 单个客户端分布数据点 |
| `ClientTrendData` | 客户端趋势缓存数据 |

**TrendStore 常量**：
- `RETENTION_SECS = 7 * 24 * 60 * 60`（7 天保留期）
- `SAMPLE_SECS = 10 * 60`（10 分钟采样间隔）
- `TOP_CLIENT_COUNT = 15`（Top 15 客户端）

**核心方法**：
- `TrendStore::record(now, snapshot)`：记录趋势数据点，自动填充缺失时间窗口
- `TrendStore::record_clients(now, clients)`：记录客户端分布，维护 Top-N 缓存

**数据填充策略**：
- 采样点按 10 分钟对齐（`bucket = now - (now % SAMPLE_SECS)`）
- 缺失的时间窗口填充零值
- 写入即冻结：同一 bucket 不覆盖

**持久化**：
- 两个 JSONL 文件：`trends.jsonl`（趋势数据）和 `top_clients.jsonl`（客户端分布）
- 每次采样追加写入一行 JSON
- 启动时自动从磁盘加载历史数据

---

## 5. 关键数据结构

### 5.1 数据结构关系图

```
AppState
├── tracker: Arc<TrackerPool>
│   └── shards: Vec<RwLock<Tracker>>
│       └── Tracker
│           ├── swarms: BTreeMap<InfoHash, Swarm>
│           │   └── Swarm
│           │       ├── ipv4_peers: PackedIpv4Peers (Vec<u8>, 12B/entry)
│           │       ├── ipv6_peers: PackedIpv6Peers (Vec<u8>, 24B/entry)
│           │       ├── complete: u32
│           │       └── downloaded: u32
│           ├── client_counts: Vec<(u8, u64)>
│           └── counters: TrackerCounters
├── trends: Arc<RwLock<TrendStore>>
│   └── TrendStore
│       ├── points: Vec<TrendPointResponse>
│       ├── client_points: Vec<(u64, Vec<(u8, u32)>)>
│       └── caches...
└── blacklist: Arc<RwLock<Arc<HashSet<InfoHash>>>>
```

### 5.2 紧凑 Peer 存储内存布局

**IPv4 条目（12 字节）**：
```
┌───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┐
│ IP[0] │ IP[1] │ IP[2] │ IP[3] │Port[0]│Port[1]│ Flags │  Tag  │ Seen[0]│Seen[1]│Seen[2]│Seen[3]│
└───────┴───────┴───────┴───────┴───────┴───────┴───────┴───────┴───────┴───────┴───────┴───────┘
 ←──────── 4 bytes ────────→ ← 2B → ← 1B → ← 1B → ←────────── 4 bytes ──────────→
```

**IPv6 条目（24 字节）**：
```
┌──────────────────── 16 bytes ────────────────────┬── 2B ──┬── 1B ──┬── 1B ──┬──────── 4 bytes ────────┐
│                    IPv6 Address                    │  Port  │ Flags  │  Tag   │     Last Seen Secs      │
└───────────────────────────────────────────────────┴────────┴────────┴────────┴─────────────────────────┘
```

- `Flags` 字段：bit 0 = `FLAG_COMPLETE`（是否做种）
- `Tag` 字段：客户端标签字节（`client_id::identify()` 返回值）
- `Last Seen Secs`：相对于 Tracker 启动时间的秒数（大端序 u32）

---

## 6. 关键函数与算法

### 6.1 Announce Interval 抖动

```rust
fn jittered_interval_secs(interval, info_hash, peer_id, now_secs) -> u64
```

- 基础值 ±10% 范围内随机偏移
- 种子 = FNV-1a 变体（info_hash + peer_id + now_secs）
- 同一 Peer 在不同时间窗口获得不同 interval
- 不同 Peer 在同一时间窗口获得不同 interval

### 6.2 Peer 随机选择（OpenTracker 风格）

```rust
PackedIpv4Peers::select_random(count, rng, exclude, contacts)
```

1. 定点化：将 total 左移至 ≥ 2^62
2. 计算步长：`shifted_step = shifted_total / count`
3. 从随机位置开始，按步长等间距跳跃
4. 每步添加小随机偏移（1 ~ diff）
5. 排除请求者自身的 Peer

### 6.3 四维 Top-K 排名

```rust
top_torrents_all(swarms, limit) -> Top100All
```

- 单次遍历，4 个最小堆并行维护
- 快速路径：若所有堆已满且当前种子低于所有阈值，直接跳过
- 时间复杂度：O(N log K)，N = 种子数，K = limit

### 6.4 客户端识别

```rust
identify(peer_id: &[u8; 20]) -> u8
```

1. 若 `peer_id[0] == b'-'`：Azureus 风格，查 `AZUREUS_TABLE[peer_id[1]][peer_id[2]]`
2. 否则：线性扫描 `NON_AZUREUS` 前缀数组
3. 未匹配返回 `UNKNOWN(0)`

### 6.5 过期清扫

```rust
Tracker::expire(now)
```

- 遍历所有 Swarm，调用 `Swarm::expire()`
- 移除 `last_seen_secs` 超时的 Peer
- 清空空 Swarm（`swarms.retain()`）
- 更新增量计数器和客户端计数
- Debug 模式下验证计数器一致性

---

## 7. 模块依赖关系

```
                    ┌─────────┐
                    │ main.rs │
                    └────┬────┘
                         │
                    ┌────▼────┐
                    │ server  │ ← Axum, Tokio, Tower
                    └┬───────┘
                     │
              ┌──────┼──────┐
              ▼      ▼      ▼
         ┌────────┐ ┌────────────┐ ┌───────────┐
         │  core  │ │ protocol   │ │ server/   │
         │        │ │            │ │ handlers  │
         └────────┘ │ announce   │ │ blacklist │
              ▲     │ bencode    │ │ trends    │
              │     │ client_id  │ └───────────┘
              │     └─────┬──────┘
              │           │
              └───────────┘
           core/types 被 protocol 依赖
```

**依赖规则**：
- `core` 不依赖 `protocol` 和 `server`（纯逻辑层）
- `protocol` 依赖 `core::types` 和 `core::tracker`（类型引用）
- `server` 依赖 `core` 和 `protocol`（整合层）
- `main.rs` 仅依赖 `server`（入口点）

---

## 8. 数据流分析

### 8.1 Announce 请求流

```
BitTorrent Client
       │
       │  GET /announce?info_hash=...&peer_id=...&port=...
       ▼
  Axum Router
       │
       ▼
  handlers::announce()
       │
       ├── parse_announce_query()  →  AnnounceQuery
       ├── blacklist.read().contains()  →  检查黑名单
       ├── cloudflare_connecting_ip()  →  确定 IP
       ├── client_id::identify()  →  识别客户端
       │
       ▼
  TrackerPool::announce(info_hash, input, now)
       │
       ├── shard_index(info_hash)  →  确定分片
       ├── shard.write().await  →  获取写锁
       │
       ▼
  Tracker::announce(input, now)
       │
       ├── swarms.entry(info_hash)  →  获取/创建 Swarm
       ├── swarm.upsert_peer()  →  更新 Peer 状态
       ├── counters.apply_upsert()  →  更新增量计数器
       ├── swarm.contacts_excluding()  →  随机选择 Peer
       ├── jittered_interval_secs()  →  计算抖动间隔
       │
       ▼
  announce_response(output, compact)  →  Bencode 编码
       │
       ▼
  HTTP 200 Response (text/plain; charset=ISO-8859-1)
```

### 8.2 趋势数据流

```
Tokio 定时器 (每 10 分钟)
       │
       ▼
  tracker.snapshot()  →  TrackerSnapshot
       │
       ├── trends.write().await.record(now, snapshot)
       │   └── 记录趋势点 + 填充缺失窗口 + 清理过期数据
       │
       ├── save_trend_point(path, now, snapshot)  →  追加 JSONL
       │
       ├── trends.write().await.record_clients(now, clients)
       │   └── 记录客户端分布 + 维护 Top-15 缓存
       │
       └── save_client_point(path, now, clients)  →  追加 JSONL
```

### 8.3 黑名单热重载流

```
Tokio 定时器 (每 5 秒)
       │
       ▼
  file_mtime(path)  →  检查修改时间
       │
       │  (mtime 变化)
       ▼
  load_blacklist(path)  →  HashSet<InfoHash>
       │
       ▼
  *blacklist.write().await = Arc::new(new_set)  →  原子替换
```

---

## 9. 并发模型与分片策略

### 9.1 分片策略

- **分片数**：默认 64（`DEFAULT_TRACKER_SHARDS`）
- **分片路由**：`DefaultHasher` 哈希 InfoHash 后取模
- **锁粒度**：每分片独立 `RwLock<Tracker>`
- **Announce**：获取目标分片的**写锁**
- **Scrape**：按分片分组，获取各分片的**读锁**
- **Snapshot**：顺序获取各分片的读锁
- **Expire**：非阻塞尝试获取写锁（`try_write()`），获取失败则跳过

### 9.2 锁层级

```
AppState (Clone, 共享引用)
├── tracker: Arc<TrackerPool>
│   └── shards[i]: RwLock<Tracker>     ← 分片级锁
│       └── swarms: BTreeMap<...>      ← 锁内数据，无需额外同步
├── trends: Arc<RwLock<TrendStore>>    ← 全局读写锁
└── blacklist: Arc<RwLock<Arc<HashSet>>>  ← 全局读写锁，Arc 原子替换
```

### 9.3 后台任务

| 任务 | 间隔 | 作用 |
|------|------|------|
| 过期清扫 | 1 秒 | 清除超时 Peer，回收空 Swarm |
| 趋势采样 | 10 分钟 | 记录快照数据，持久化到 JSONL |
| 黑名单监视 | 5 秒 | 检查文件变更，热重载黑名单 |

---

## 10. 依赖清单

### 10.1 运行时依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `anyhow` | 1.0 | 错误处理 |
| `axum` | 0.8 (features: tokio, http1) | HTTP 框架 |
| `clap` | 4.5 (features: derive, env) | CLI 参数解析 |
| `http-body-util` | 0.1 | HTTP Body 工具 |
| `percent-encoding` | 2.3 | URL 百分比编码解码 |
| `serde` | 1.0 (features: derive) | 序列化框架 |
| `serde_json` | 1.0 | JSON 序列化 |
| `thiserror` | 2.0 | 派生错误类型 |
| `tokio` | 1.38 (features: macros, net, rt-multi-thread, signal, time) | 异步运行时 |
| `tower` | 0.5 | 服务抽象层 |
| `tracing` | 0.1 | 结构化日志 |
| `tracing-subscriber` | 0.3 (features: env-filter) | 日志订阅器 |

### 10.2 开发时依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `hyper` | 1.4 | HTTP 库（测试用） |
| `rand` | 0.10 | 随机数（测试/基准用） |
| `reqwest` | 0.13 (features: json, rustls) | HTTP 客户端（测试用） |
| `serde_json` | 1.0 | JSON 序列化（测试用） |

---

## 11. 构建与运行

### 11.1 环境要求

- Rust 1.85+（Edition 2021）

### 11.2 构建命令

```bash
# Release 构建
cargo build --release

# 无 Dashboard 构建（更小二进制）
cargo build --release --no-default-features

# 交叉编译 Linux musl（从 Windows）
cargo build --release --target x86_64-unknown-linux-musl
```

### 11.3 运行命令

```bash
# 默认运行
cargo run --release -- --listen 127.0.0.1:8080

# 自定义参数
cargo run --release -- \
  --listen 0.0.0.0:6969 \
  --interval-secs 900 \
  --peer-timeout-secs 3000 \
  --blacklist /path/to/blacklist.txt \
  --trends-file /path/to/trends.jsonl
```

### 11.4 CLI 参数

| 参数 | 环境变量 | 默认值 | 说明 |
|------|---------|--------|------|
| `--listen` | `RUSTRACKER_LISTEN` | `0.0.0.0:8080` | 监听地址 |
| `--interval-secs` | `RUSTRACKER_INTERVAL_SECS` | `1800` | Announce 间隔（秒） |
| `--peer-timeout-secs` | `RUSTRACKER_PEER_TIMEOUT_SECS` | `3000` | Peer 过期超时（秒） |
| `--blacklist` | `RUSTRACKER_BLACKLIST` | — | 黑名单文件路径 |
| `--trends-file` | `RUSTRACKER_TRENDS_FILE` | — | 趋势数据 JSONL 路径 |

命令行参数优先于环境变量。

### 11.5 日志控制

```bash
RUST_LOG=info cargo run --release          # 默认级别
RUST_LOG=debug cargo run --release         # 调试级别
RUST_LOG=rustracker=trace cargo run --release  # 仅 rustracker 追踪
```

### 11.6 优雅关闭

- 监听 `Ctrl+C`（SIGINT）和 `SIGTERM`（Unix）
- 收到信号后 Axum 优雅关闭，等待现有请求完成

---

## 12. 测试体系

### 12.1 单元测试

各模块内嵌 `#[cfg(test)] mod tests`：

| 模块 | 测试内容 |
|------|---------|
| `core/tracker.rs` | Peer 追踪、停止/完成事件、过期清扫、快照、Interval 抖动、大 Swarm、Peer 轮换 |
| `core/counters.rs` | 增量计数器更新（新 seeder/leecher、状态转换、移除、过期、下溢保护） |
| `protocol/bencode.rs` | Bencode 字典编码（键排序） |
| `protocol/announce.rs` | Announce/Scrape 解析、紧凑 Peer 编码 |

### 12.2 集成测试

`tests/tracker_http.rs`：通过 Axum `oneshot` 测试完整 HTTP 请求流程。

| 测试 | 说明 |
|------|------|
| `healthz_returns_ok` | 健康检查端点 |
| `index_returns_dashboard_html` | Dashboard HTML 返回 |
| `stats_api_returns_json_totals` | Stats API 统计正确性 |
| `trends_api_returns_history` | Trends API 历史数据 |
| `handles_concurrent_announces_across_shards` | 多分片并发 Announce |
| `announce_then_scrape_reports_peer` | Announce 后 Scrape 数据一致 |
| `compact_announce_includes_ipv6_peers6` | IPv6 紧凑编码 |
| `announce_uses_cloudflare_connecting_ip` | CF-Connecting-IP 头处理 |
| `invalid_announce_returns_bencoded_failure` | 无效请求返回 Bencode 错误 |
| `blacklisted_announce_returns_403` | 黑名单拒绝 |
| `non_blacklisted_announce_works` | 非黑名单正常处理 |
| `scrape_excludes_blacklisted_torrents` | Scrape 排除黑名单 |
| `info_hash_from_hex_parses_correctly` | InfoHash 十六进制解析 |

### 12.3 基准测试

`examples/unified_bench.rs`：统一基准测试套件。

- 并发 HTTP 请求通过 Axum Router
- 跟踪 RPS、RSS 内存、CPU 使用率、延迟百分位（p50/p99/max）
- 动态调整新 Peer 加入 vs 重新 Announce 的比例
- 限制每个种子最大 Peer 数

运行方式：
```bash
cargo run --release --example unified_bench
```

---

## 13. Feature Flags

| Feature | 默认 | 说明 |
|---------|------|------|
| `dashboard` | 启用 | 嵌入 Web 监控面板（HTML/CSS/JS） |
| `personal-contact` | 禁用 | 向 Dashboard HTML 注入联系信息（`contact.html`） |

禁用 `dashboard`：
- 编译时排除 `/`、`/style.css`、`/app.js` 路由
- 不嵌入静态资源，二进制体积更小
- 所有 Tracker 协议端点（`/announce`、`/scrape`、`/healthz`、`/api/*`）功能完整

```bash
cargo build --release --no-default-features
```

---

## 14. 部署与运维

### 14.1 Linux systemd 部署

使用 `install-linux.sh` 脚本安装为 systemd 服务：

```bash
sudo sh install-linux.sh
```

安装后文件布局：

| 路径 | 说明 |
|------|------|
| `/opt/rustracker/rustracker` | 二进制文件 |
| `/etc/rustracker.env` | 环境变量配置 |
| `/etc/rustracker/blacklist.txt` | 种子黑名单 |
| `/var/lib/rustracker/trends.jsonl` | 趋势数据 |
| `/etc/systemd/system/rustracker.service` | systemd 服务单元 |

### 14.2 CI/CD

GitHub Actions 工作流：

| 工作流 | 文件 | 说明 |
|--------|------|------|
| Release | `.github/workflows/release.yml` | 版本号变更时自动构建并发布 |
| Memory Benchmark | `.github/workflows/memory-benchmark.yml` | 内存基准测试 |
| Sync Deploy | `.github/workflows/sync-deploy.yml` | 同步部署 |

### 14.3 交叉编译

`.cargo/config.toml` 配置了 zig-cc 工具链用于 musl 静态链接：

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

生成完全静态链接的二进制文件，无 glibc 依赖。

### 14.4 Cloudflare 支持

Announce 处理器优先读取 `CF-Connecting-IP` 请求头获取客户端真实 IP，适用于通过 Cloudflare 代理部署的场景。

---

> 本文档基于 Rustracker v0.2.5 源码自动分析生成。
