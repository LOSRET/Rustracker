[English](./README.md) | 中文

# Rustracker

[![Version](https://img.shields.io/github/v/release/LOSRET/Rustracker?color=blue&label=version)](https://github.com/LOSRET/Rustracker/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021--edition-orange.svg)](https://www.rust-lang.org)

一个轻量、高性能的 HTTP BitTorrent 跟踪器，内置实时监控面板，使用 Rust 编写，采用 Vibe Coding 开发方式。

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
- 多语言界面（中文 / English / 日本語 / Русский / Deutsch / Українська），自动检测语言

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
- [Node.js](https://nodejs.org/) 20+ 及 npm（仅在使用默认的 `dashboard` feature 构建时需要）

### 从源码构建并运行

```bash
git clone https://github.com/LOSRET/Rustracker.git
cd rustracker
npm install --prefix frontend   # 仅首次需要，安装面板构建依赖
cargo run --release -- --listen 127.0.0.1:8080
```

在浏览器中打开 `http://127.0.0.1:8080` 即可查看监控面板。

> 若使用 `cargo build --release --no-default-features` 关闭面板构建，则无需 Node.js/npm。

### 预编译二进制

从 [GitHub Releases](https://github.com/LOSRET/Rustracker/releases) 下载最新版本：
- `rustracker.exe` — Windows x86_64
- `rustracker-linux` — Linux x86_64
- `rustracker-linux.tar.gz` — Linux 归档包（含安装脚本）

## 命令行参数

| 参数 | 环境变量 | 默认值 | 说明 |
|------|---------|--------|------|
| `--listen` | `RUSTRACKER_LISTEN` | `[::]:8080`（Linux）；`[::]:8080` + `0.0.0.0:8080`（Windows） | 监听地址（可重复指定，平台相关默认双栈） |
| `--interval-secs` | `RUSTRACKER_INTERVAL_SECS` | `1800` | 通告间隔（秒） |
| `--peer-timeout-secs` | `RUSTRACKER_PEER_TIMEOUT_SECS` | `3000` | Peer 过期超时（秒） |
| `--blacklist` | `RUSTRACKER_BLACKLIST` | — | 种子黑名单文件路径 |
| `--trends-file` | `RUSTRACKER_TRENDS_FILE` | — | 趋势数据 JSONL 持久化路径 |
| `--admin-token` | `RUSTRACKER_ADMIN_TOKEN` | — | 管理接口所需的 Bearer Token |

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

### 面板 API

| 端点 | 说明 |
|------|------|
| `GET /api/stats` | 实时计数：peers、seeders、leechers、torrents、completed、rps、version、uptime |
| `GET /api/trends` | 历史趋势数据（7 天保留，10 分钟采样） |
| `GET /api/clients` | Top 15 客户端类型及时序历史 |
| `GET /api/clients/list` | 所有已连接客户端类型，按 peer 数降序（当前快照） |
| `GET /api/top100?limit=100` | Top 100 种子，按 peers / seeders / leechers / downloaded 排名（`limit` 最大 500） |

均返回 JSON。

## Web 监控面板

Tracker 在同一端口的 `/` 路径提供功能完整的监控面板：

- **概览页面** — Peers、Seeders、Leechers、Torrents 和 Completed 实时计数
- **趋势图表** — 交互式 ECharts 图表，支持 24小时 / 3天 / 7天 时间范围切换
- **客户端图表** — Top 15 BitTorrent 客户端的 peer 数量趋势
- **客户端页面** — 所有已连接客户端类型及其当前 peer 数量的可排序表格（由 `/api/clients/list` 提供数据）
- **Top 100 页面** — 最活跃种子的可排序表格
- **免责声明** — 面向公众部署时的内置法律声明
- **国际化** — 自动检测 6 种语言（中文 / English / 日本語 / Русский / Deutsch / Українська），支持手动切换

`/assets/*` 下的哈希资源（Vite 产出的 JS/CSS/字体，文件名按内容寻址）由服务器以 `Cache-Control: public, max-age=31536000, immutable` 缓存；`index.html` 入口随每次发布重建。

## 种子黑名单

创建一个文本文件，每行一个 40 字符的十六进制 `info_hash`：

```
# 被屏蔽的种子
e09b1c0c4b174ef2b25c8de662941777fb3f2d7a
```

通过 `--blacklist blacklist.txt` 或 `RUSTRACKER_BLACKLIST` 传入路径。

- 被黑名单中的种子在 `announce` 时会返回 bencode 失败响应
- `scrape` 结果会静默排除黑名单中的种子
- 文件每 5 秒检测一次变更——编辑保存后无需重启
- 无效行会以警告形式记录并跳过

### 管理接口

配置 `RUSTRACKER_ADMIN_TOKEN` 后，可以查询某个条目是否已在黑名单中：

```bash
curl "http://127.0.0.1:8080/api/blacklist?info_hash=0123456789abcdef0123456789abcdef01234567" \
  -H "Authorization: Bearer $RUSTRACKER_ADMIN_TOKEN"
```

响应包含 `"blacklisted": true` 或 `"blacklisted": false`，并且不会修改黑名单文件。

同时配置 `RUSTRACKER_BLACKLIST` 和 `RUSTRACKER_ADMIN_TOKEN` 后，可以通过带鉴权的管理接口添加黑名单：

```bash
curl -X POST http://127.0.0.1:8080/api/blacklist \
  -H "Authorization: Bearer $RUSTRACKER_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"info_hash":"0123456789abcdef0123456789abcdef01234567"}'
```

接口会先把 40 字符十六进制 `info_hash` 追加写入黑名单文件，再更新内存黑名单。重复添加会返回成功，但 `"added": false`。

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
├── src/
│   ├── core/          # 追踪引擎（无 I/O）：types、tracker、swarm、topk、counters
│   ├── protocol/      # BT 协议：bencode、announce、client_id（102 种客户端）
│   └── server/        # HTTP 层：handlers、admin、pool（64 分片）、blacklist、trends
├── frontend/          # Vue 3 + Vite + Tailwind 面板 SPA（编译时内嵌）
├── examples/          # 负载测试与性能基准
├── tests/             # 集成测试
├── build.rs           # 构建前端，将 dist/ 内嵌进二进制
└── install-linux.sh   # Linux systemd 安装脚本
```

三层设计：`core`（纯引擎）→ `protocol`（BT 编解码）→ `server`（Axum HTTP）。详细模块职责见 `CLAUDE.md`。

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

发布包中包含 `install-linux.sh`。将 Linux 二进制文件和脚本放在同一目录下，运行 `sudo sh install-linux.sh` 即可进入交互菜单（安装/更新、启动/停止/重启、配置、系统优化）。也支持 `install`、`start`、`stop`、`status` 等非交互式命令。

**安装后的默认文件布局：**

| 路径 | 说明 |
|------|------|
| `/opt/rustracker/rustracker` | 二进制文件 |
| `/etc/rustracker.env` | 环境变量配置，包含自动生成的 Admin Token |
| `/etc/rustracker/blacklist.txt` | 种子黑名单 |
| `/var/lib/rustracker/trends.jsonl` | 趋势数据 |
| `/etc/systemd/system/rustracker.service` | systemd 服务单元 |

## 负载测试

内置的压测工具位于 `examples/`：

```bash
cargo run --release --example announce_load -- 2000 200 100   # 简单负载测试
cargo run --release --example load_test -- --duration 60 --concurrency 500  # 高级（Zipf 分布）
cargo run --release --example unified_bench    # RPS、RSS、CPU、延迟
cargo run --release --example shrink_bench     # 内存收缩/再增长周期
```

完整的内存与基准测试列表见 `CLAUDE.md`。

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

此模式关闭 `dashboard` feature——`/` 和 `/assets/{*name}` 路由在编译时排除。所有 Tracker 协议端点（`/announce`、`/scrape`、`/healthz`、`/api/*`）功能完整。

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
