[English](./README.md) | 中文

# rustracker

[![Version](https://img.shields.io/github/v/release/LOSRET/rustracker?color=blue&label=version)](https://github.com/LOSRET/rustracker/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021--edition-orange.svg)](https://www.rust-lang.org)

一个轻量、高性能的 HTTP BitTorrent 跟踪器，内置实时监控面板，使用 Rust 编写。

> **注意：** 本项目由 Vibe Coding 方式开发。如无特殊需求，建议优先考虑使用其他成熟的 BT Tracker 方案。

## 功能亮点

**核心协议**
- 符合 BEP 3 规范的 `announce` 和 `scrape` 端点
- 紧凑的 IPv4/IPv6 peer 编码
- Bencode 编码的跟踪器响应
- 可配置的通告间隔和 peer 超时时间

**实时监控面板**
- Web 面板与 Tracker 共用同一 HTTP 端口，无需独立前端服务
- ECharts 趋势图表：种子数、Peer 数、做种数、下载数，支持 24小时 / 3天 / 7天 时间范围
- Top 100 种子页面，支持按 Peers / Seeders / Leechers / Downloaded 排序
- Top 15 客户端分布趋势图
- 中英文双语界面，自动检测语言

**运维特性**
- 64 分片并发 Tracker 池，高吞吐低争用
- 102 种 BitTorrent 客户端识别（qBittorrent、Transmission、µTorrent、Aria2、迅雷等）
- 种子黑名单热重载——编辑文件即可生效，无需重启
- 可选的趋势数据持久化到 JSONL（7 天保留，10 分钟采样）
- Ctrl+C / SIGTERM 优雅关闭
- 基于 `tracing` 的结构化日志，支持 `RUST_LOG` 环境变量过滤

**部署**
- 单一二进制文件，零外部依赖
- Linux `systemd` 安装脚本，提供中文交互菜单
- GitHub Actions CI/CD：版本号变更时自动构建并发布

## 快速开始

### 前置要求

- [Rust](https://www.rust-lang.org/tools/install) 1.85+（edition 2021）

### 从源码构建并运行

```bash
git clone https://github.com/LOSRET/rustracker.git
cd rustracker
cargo run --release -- --listen 127.0.0.1:8080
```

在浏览器中打开 `http://127.0.0.1:8080` 即可查看监控面板。

### 预编译二进制

从 [GitHub Releases](https://github.com/LOSRET/rustracker/releases) 下载最新版本：
- `rustracker.exe` — Windows x86_64
- `rustracker-linux` — Linux x86_64
- `rustracker-linux.tar.gz` — Linux 归档包（含安装脚本）

## 命令行参数

| 参数 | 环境变量 | 默认值 | 说明 |
|------|---------|--------|------|
| `--listen` | `RUSTRACKER_LISTEN` | `0.0.0.0:8080` | 监听地址 |
| `--interval-secs` | `RUSTRACKER_INTERVAL_SECS` | `1800` | 通告间隔（秒） |
| `--peer-timeout-secs` | `RUSTRACKER_PEER_TIMEOUT_SECS` | `3000` | Peer 过期超时（秒） |
| `--blacklist` | `RUSTRACKER_BLACKLIST` | — | 种子黑名单文件路径 |
| `--trends-file` | `RUSTRACKER_TRENDS_FILE` | — | 趋势数据 JSONL 持久化路径 |

每个参数均可通过环境变量或命令行参数设置，命令行优先。

```bash
# 示例：使用环境变量
export RUSTRACKER_LISTEN=0.0.0.0:6969
export RUSTRACKER_INTERVAL_SECS=900
cargo run --release
```

## API 端点

### 健康检查

```
GET /healthz
```

响应：`200 OK`，内容为 `ok`。

### Announce（通告）

```
GET /announce?info_hash=<20字节>&peer_id=<20字节>&port=6881&uploaded=0&downloaded=0&left=0&event=started&compact=1
```

| 参数 | 必填 | 说明 |
|------|------|------|
| `info_hash` | 是 | 20 字节的百分比编码种子信息哈希 |
| `peer_id` | 是 | 20 字节的百分比编码 Peer 标识 |
| `port` | 是 | Peer 的监听端口 |
| `uploaded` | 否 | 已上传总字节数 |
| `downloaded` | 否 | 已下载总字节数 |
| `left` | 否 | 剩余需下载字节数 |
| `event` | 否 | `started`、`completed`、`stopped` 或留空 |
| `compact` | 否 | `1` 使用紧凑编码（默认），`0` 使用字典编码 |
| `numwant` | 否 | 返回的 peer 数量（默认 100，最大 400） |
| `ip` | 否 | 覆盖 peer IP 地址（用于反向代理场景） |

响应（Bencode 编码）：

```
d8:completei5e10:downloadedi0e10:incompletei3e8:intervali1800e5:peers60:...(紧凑二进制)...e
```

紧凑 peer 格式：每个 IPv4 peer 占 6 字节（4 字节 IP + 2 字节端口，大端序）。IPv6 peer 占 18 字节，通过 `peers6` 键返回。

### Scrape（统计查询）

```
GET /scrape?info_hash=<20字节>[&info_hash=...]
```

支持多个 `info_hash` 参数。响应（Bencode 编码）：

```
d5:filesd20:<info_hash>d8:completei5e10:downloadedi10e10:incompletei3eeee
```

### 统计 API

```
GET /api/stats
```

返回 JSON：

```json
{
  "interval": 1800,
  "peer_timeout": 3000,
  "torrents": 42,
  "peers": 128,
  "seeders": 85,
  "leechers": 43,
  "completed": 310
}
```

### 趋势历史

```
GET /api/trends
```

返回 JSON，包含历史趋势数据（7 天保留，10 分钟采样）：

```json
{
  "history": [
    {"timestamp": 1715800000, "torrents": 40, "peers": 120, "seeders": 80, "leechers": 40}
  ]
}
```

### 客户端分布

```
GET /api/clients
```

返回 JSON，包含 Top 15 客户端类型及其历史 peer 数量：

```json
{
  "timestamp": 1715800000,
  "tags": [1, 2, 3],
  "clients": ["qBittorrent", "Transmission", "µTorrent"],
  "history": [
    {"timestamp": 1715800000, "tags": [1, 2, 3], "counts": [50, 30, 20]}
  ]
}
```

### Top 100 种子

```
GET /api/top100?limit=100
```

| 查询参数 | 说明 | 默认值 | 最大值 |
|---------|------|--------|--------|
| `limit` | 每个排名返回的条目数 | `100` | `500` |

返回 JSON，包含四种排序的排名：

```json
{
  "peers": [{"info_hash": "...", "seeders": 10, "leechers": 5, "peers": 15, "downloaded": 100}],
  "seeders": [...],
  "leechers": [...],
  "downloaded": [...]
}
```

## Web 监控面板

Tracker 在同一端口的 `/` 路径提供功能完整的监控面板：

- **概览页面** — Peers、Seeders、Leechers、Torrents 和 Completed 实时计数
- **趋势图表** — 交互式 ECharts 图表，支持 24小时 / 3天 / 7天 时间范围切换
- **客户端图表** — Top 15 BitTorrent 客户端的 peer 数量趋势
- **Top 100 页面** — 最活跃种子的可排序表格
- **免责声明** — 面向公众部署时的内置法律声明
- **国际化** — 自动检测中文/英文，支持手动切换

静态资源（`style.css`、`app.js`）由服务器缓存 1 小时。

## 种子黑名单

创建一个文本文件，每行一个 40 字符的十六进制 `info_hash`：

```
# 被屏蔽的种子
e09b1c0c4b174ef2b25c8de662941777fb3f2d7a
```

通过 `--blacklist blacklist.txt` 或 `RUSTRACKER_BLACKLIST` 传入路径。

- 被黑名单中的种子在 `announce` 时会被拒绝（HTTP 403）
- `scrape` 结果会静默排除黑名单中的种子
- 文件每 5 秒检测一次变更——编辑保存后无需重启
- 无效行会以警告形式记录并跳过

## 趋势数据持久化

默认情况下，趋势数据（种子数、peer 数、做种数、下载数、客户端分布）存储在内存中，重启后丢失。要启用持久化：

```bash
cargo run --release -- --trends-file /var/lib/rustracker/trends.jsonl
```

- 数据每 10 分钟采样一次，保留 7 天
- 会创建两个 JSONL 文件：
  - `<路径>`（如 `trends.jsonl`）——每个时间戳的种子数/peer 数/做种数/下载数
  - 同目录下的 `top_clients.jsonl`——每个时间戳的客户端分布
- 重启时自动从磁盘加载历史数据
- Linux 安装脚本默认在 `/var/lib/rustracker/trends.jsonl` 启用此功能

## 项目架构

```
┌──────────────────────────────────────────────────────┐
│                    Axum HTTP 服务器                    │
│  /announce  /scrape  /healthz  /  /api/*             │
└──────────┬───────────────────────────────────────────┘
           │
           ▼
┌──────────────────────┐     ┌─────────────────────────┐
│    TrackerPool        │     │     TrendStore           │
│  (64 分片 RwLock)     │     │  (7 天 JSONL 历史)       │
│                       │     │  10 分钟采样              │
│  ┌─────────────────┐  │     └─────────────────────────┘
│  │ Tracker 分片 0   │  │
│  │  BTreeMap<       │  │     ┌─────────────────────────┐
│  │   InfoHash,      │  │     │   黑名单监视器            │
│  │   Swarm          │  │     │  (5 秒文件重载)           │
│  │  >               │  │     │  HashSet<InfoHash>        │
│  └─────────────────┘  │     └─────────────────────────┘
│  ... (×64 分片)       │
└───────────────────────┘
```

- **分片**：Tracker 池使用 64 个分片，每个分片独立 `RwLock`，在高并发下最小化争用
- **Peer 存储**：紧凑的二进制格式，IPv4 每 peer 6 字节、IPv6 每 peer 18 字节——无逐 peer 堆分配
- **过期清理**：后台任务每 1 秒清扫过期 peer
- **客户端识别**：编译时生成的 256×256 查找表，用于 Azureus 风格的 peer ID 前缀匹配，同时支持非标准格式的前缀匹配

## 仓库结构

```
rustracker/
├── Cargo.toml                     # Rust 项目清单与依赖声明
├── LICENSE                        # MIT 许可证
├── README.md                      # 英文文档
├── README-zh.md                   # 中文文档
├── install-linux.sh               # Linux systemd 安装脚本（交互式菜单）
│
├── src/                           # Rust 源代码
│   ├── main.rs                    # 入口：CLI 解析、Tokio 运行时、优雅关闭
│   ├── lib.rs                     # 库根：重新导出 core、protocol、server 模块
│   │
│   ├── core.rs                    # 核心追踪引擎模块声明
│   ├── core/                      # 核心追踪引擎（无 I/O 依赖）
│   │   ├── types.rs               # 核心类型：InfoHash、PeerId、PeerState、TorrentStats
│   │   ├── tracker.rs             # Tracker、AnnounceInput/Output、TrackerSnapshot
│   │   ├── swarm.rs               # 每种子 Peer 集合，紧凑二进制存储、PeerEndpoint
│   │   ├── topk.rs                # 四维 Top-K 排名（peers/seeders/leechers/downloaded）
│   │   └── counters.rs            # 增量计数器，O(1) 快照
│   │
│   ├── protocol.rs                # BT 协议模块声明
│   ├── protocol/                  # BitTorrent 协议编解码（无网络依赖）
│   │   ├── bencode.rs             # 轻量 Bencode 编码器（零外部依赖）
│   │   ├── announce.rs            # BEP 3 announce/scrape 查询解析与响应构建
│   │   └── client_id.rs           # 102 种客户端 Peer ID 识别（编译时查找表）
│   │
│   ├── server.rs                  # HTTP 服务层模块声明
│   └── server/                    # HTTP 服务层（axum + tokio）
│       ├── handlers.rs            # HTTP 处理器：announce、scrape、healthz、面板、API
│       ├── blacklist.rs           # 种子黑名单，5 秒热重载文件监视
│       └── trends.rs              # 趋势数据采集、7 天 JSONL 持久化、历史 API
│
├── assets/                        # Web 监控面板静态文件
│   ├── index.html                 # 面板 HTML（生产版，内联 CSS/JS）
│   ├── index-build.html           # 面板 HTML（开发版，外部引用资源）
│   ├── style.css                  # 面板样式
│   └── app.js                     # 面板逻辑：ECharts 图表、国际化、API 调用
│
├── examples/                      # 负载测试与性能基准
│   ├── announce_load.rs           # 简单并发通告负载测试
│   ├── load_test.rs               # 高级负载测试（Zipf 分布、Peer 生命周期）
│   ├── rps_bench.rs               # 每秒请求数基准
│   ├── unified_bench.rs           # 统一基准测试套件
│   ├── memory_tracker_bench.rs    # 内存占用基准
│   ├── memory_tracker_btree.rs    # BTree 内存布局分析
│   ├── memory_staircase_test.rs   # 内存阶梯增长测试
│   └── memory_ci_compare.rs       # CI 内存回归对比
│
└── tests/                         # 集成测试
    └── tracker_http.rs            # HTTP 端点集成测试
```

**源码模块概览：**

| 层级 | 模块 | 职责 |
|------|------|------|
| `core` | `types.rs` | `InfoHash`（20 字节）、`PeerId`（20 字节）、`PeerState`、`TorrentStats` |
| `core` | `tracker.rs` | 64 分片 `TrackerPool` — 每分片 `RwLock<BTreeMap<InfoHash, Swarm>>` |
| `core` | `swarm.rs` | 每种子 Peer 集合 — IPv4 每 peer 6 字节、IPv6 每 peer 18 字节紧凑存储 |
| `core` | `counters.rs` | 增量计数器，O(1) 快照（无需全量遍历） |
| `protocol` | `bencode.rs` | 最小化 Bencode 序列化器 — 无外部 crate 依赖 |
| `protocol` | `announce.rs` | BEP 3 规范 Bencode 响应构建、紧凑 Peer 编码 |
| `protocol` | `client_id.rs` | 编译时 256×256 查找表，匹配 `-XX####-` Azureus 风格 Peer ID 前缀 |
| `server` | `handlers.rs` | `/announce`、`/scrape`、`/healthz`、`/api/*`、`/` 请求处理 |
| `server` | `blacklist.rs` | `HashSet<InfoHash>` 通过 5 秒轮询实现热重载 |
| `server` | `trends.rs` | 内存环形缓冲 + 可选 JSONL 持久化，7 天保留 |

## 客户端识别

Tracker 可从 peer ID 前缀识别 **102 种 BitTorrent 客户端**，包括：

| 类别 | 客户端 |
|------|--------|
| 主流 | qBittorrent、Transmission、µTorrent、BitTorrent、Deluge、Vuze、BiglyBT |
| 轻量 | Aria2、libtorrent、rTorrent、KTorrent、FrostWire |
| Web 端 | WebTorrent、Brave |
| 经典 | FlashGet、GetRight、LimeWire、Shareaza |
| 国产 | 迅雷（Thunder）、QQ旋风、百度网盘 |
| 其他 | Tixati、Halite、BitComet、BitSpirit、MLDonkey |

客户端标签通过 `/api/clients` 端点暴露，并展示在面板的客户端分布图表中。

## Linux 安装

发布包中包含 `install-linux.sh`。将 Linux 二进制文件和脚本放在同一目录下，然后运行：

```bash
sudo sh install-linux.sh
```

安装程序提供中文交互菜单，支持：

1. 安装或更新
2. 卸载
3. 启动 / 停止 / 重启服务
4. 查看状态
5. 查看配置
6. 修改配置（监听地址、通告间隔、超时时间）

非交互式命令：

```bash
sudo sh install-linux.sh install
sudo sh install-linux.sh status
sudo sh install-linux.sh configure
sudo sh install-linux.sh restart
sudo sh install-linux.sh uninstall
```

**安装后的默认文件布局：**

| 路径 | 说明 |
|------|------|
| `/opt/rustracker/rustracker` | 二进制文件 |
| `/etc/rustracker.env` | 环境变量配置 |
| `/etc/rustracker/blacklist.txt` | 种子黑名单 |
| `/var/lib/rustracker/trends.jsonl` | 趋势数据 |
| `/etc/systemd/system/rustracker.service` | systemd 服务单元 |

对于监听地址，仅输入端口号（如 `6969`）会被接受并保存为 `0.0.0.0:6969`。

## 负载测试

内置的压测示例：

### 简单负载测试

```bash
cargo run --release --example announce_load -- 2000 200 100
#                                       总请求数 并发数 种子数
```

按持续时间模式：

```bash
cargo run --release --example announce_load -- \
  --duration-secs 30 --concurrency 200 --torrents 100
```

### 高级负载测试

```bash
cargo run --release --example load_test -- \
  --duration 60 \
  --concurrency 500 \
  --torrents 1000 \
  --peers 50000 \
  --scrape-weight 1 \
  --announce-weight 5 \
  --keep-alive \
  --progress-interval 5
```

特性：Zipf 分布模拟真实种子热度、peer 生命周期事件（started/completed/stopped）、40% 做种比例、延迟百分位统计（p50/p95/p99）。

### RPS 基准测试

```bash
cargo run --release --example rps_bench
```

单任务混合流量基准，模拟真实 tracker 生命周期（初期以新 peer 为主，后期以重新通告为主）。报告累计 RPS、窗口 RPS 和 RSS。

### 统一基准测试

```bash
cargo run --release --example unified_bench
```

并发 HTTP 基准，追踪 RPS、RSS、CPU 使用率和每请求延迟（avg/p50/p99/max）。

## 开发指南

### 构建

```bash
cargo build --release
```

### 构建无面板版本

编译纯 Tracker 二进制，不嵌入 Web 前端（体积更小，不含 HTML/CSS/JS）：

```bash
cargo build --release --no-default-features
```

此模式关闭 `dashboard` feature——`/`、`/style.css`、`/app.js` 路由在编译时排除。所有 Tracker 协议端点（`/announce`、`/scrape`、`/healthz`、`/api/*`）功能完整。

### 测试

```bash
cargo test
```

### 交叉编译 Linux（从 Windows）

项目包含 `.cargo/config.toml`，配置了 zig-cc 工具链用于 musl 静态链接：

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

### 日志

通过 `RUST_LOG` 环境变量控制日志详细程度：

```bash
RUST_LOG=debug cargo run --release
RUST_LOG=rustracker=trace cargo run --release
```

## 许可证

[MIT](./LICENSE)

## 致谢

- [opentracker](https://erdgeist.org/arts/software/opentracker/)（作者 Dirk Engling）— peer 选择算法设计受 opentracker 的定步长均匀随机选择策略启发。采用 [Beerware](https://erdgeist.org/beerware.html) 许可证。
- [PBH-BTN/quick-references](https://github.com/PBH-BTN/quick-references) — BitTorrent 客户端 peer_id 识别参考表。
