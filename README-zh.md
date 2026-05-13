[English](./README.md) | 中文

# rustracker

一个用 Rust 编写的小型 HTTP BitTorrent 跟踪器。

## 功能特性

- `GET /announce` 用于 peer 通告
- `GET /scrape` 用于获取种子统计信息
- Bencode 编码的跟踪器响应
- 默认使用紧凑的 IPv4 peer 响应
- 内存中的 swarm 状态管理，支持 peer 过期
- 同一 HTTP 端口上的仪表盘界面
- 仪表盘数据的 JSON 统计 API
- 支持 Ctrl+C 优雅关闭
- 通过文件实现种子黑名单

## 运行

```powershell
cargo run -- --listen 127.0.0.1:8080
```

也可以通过环境变量配置服务：

```powershell
$env:RUSTRACKER_LISTEN = "127.0.0.1:8080"
$env:RUSTRACKER_INTERVAL_SECS = "1800"
$env:RUSTRACKER_PEER_TIMEOUT_SECS = "3000"
cargo run
```

### 种子黑名单

创建一个文本文件，每行一个 40 字符的十六进制 `info_hash`（空行和 `#` 开头的注释会被忽略），然后通过 `--blacklist` 或 `RUSTRACKER_BLACKLIST` 参数传入：

```text
# 被屏蔽的种子
e09b1c0c4b174ef2b25c8de662941777fb3f2d7a
```

```powershell
cargo run -- --blacklist blacklist.txt
```

被黑名单中的种子在 `announce` 时会被拒绝（HTTP 403），并在 `scrape` 结果中静默排除。该文件每 5 秒检测一次变更——编辑保存后无需重启。无效行会以警告形式记录并跳过。

### 趋势数据持久化

跟踪器趋势数据（种子数、peer 数、做种数、下载数、客户端分布）默认存储在内存中，重启后会丢失。要将趋势数据持久化到磁盘，请使用 `--trends-file` 或 `RUSTRACKER_TRENDS_FILE`：

```powershell
cargo run -- --trends-file trends.jsonl
```

数据每 10 分钟采样一次，保留 7 天。会创建两个 JSONL 文件：
- 您指定的路径（例如 `trends.jsonl`）——每个时间戳的种子数/peer 数/做种数/下载数
- 同目录下的 `top_clients.jsonl`——每个时间戳的客户端分布

重启时会从磁盘加载现有数据。Linux 安装程序默认在 `/var/lib/rustracker/trends.jsonl` 启用此功能。

## Linux 安装

发布包中包含用于 Linux 主机的 `install-linux.sh`。将 Linux 二进制文件和脚本放在同一目录下，然后运行：

```sh
sudo sh install-linux.sh
```

安装程序提供中文菜单，支持安装/更新、卸载、服务启动/停止/重启、状态查看和配置显示。

也可以使用非交互式命令：

```sh
sudo sh install-linux.sh install
sudo sh install-linux.sh status
sudo sh install-linux.sh configure
sudo sh install-linux.sh restart
```

使用 `configure` 可在安装后更改监听地址、通告间隔或 peer 超时时间。对于监听地址，仅输入端口号（如 `6969`）会被接受并保存为 `0.0.0.0:6969`。
