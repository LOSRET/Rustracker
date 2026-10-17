# Rustracker Linux 生产安装指南（Agent）

> 适用场景：用户给出仓库地址 `https://github.com/LOSRET/Rustracker`，并说“帮我安装”。
>
> 范围：Linux 生产部署（x86_64 优先；ARM 走源码构建）。Windows、开发环境、负载测试不在本文范围。
>
> 开发规范见 [CLAUDE.md](./CLAUDE.md)；人类安装说明见 [README.md](./README.md) / [README-zh.md](./README-zh.md)。

**TL;DR：** 先按第 1 节向用户确认配置，按第 3 节安装，按第 4 节验证；覆盖安装、卸载、改防火墙前必须问用户。

人类可以直接对 agent 说：`请阅读 https://github.com/LOSRET/Rustracker/blob/main/agent-install.md 并按它安装。`

## 0. 执行原则

- **先问配置，再安装。** 按第 1 节一次性问清楚监听、反向代理、面板、MCP、趋势、间隔等；用户没回答的项才用默认值。
- **先探测，再安装。** 不要在不了解系统状态的情况下直接覆盖已有服务。
- **不要臆造安装方式。** 本项目没有官方 Docker 镜像、没有 apt/yum 包、没有 arm64 预编译包。
- **破坏性操作必须先问用户**：覆盖已有安装、卸载、停止非 rustracker 进程、修改防火墙、把服务暴露到公网。
- **不要打印 Admin Token** 到聊天、日志或提交记录。
- **不要用 `curl | sudo sh`。** 先把文件下载到本地，再执行。
- **每一步检查退出码。** 验证失败就停止，不要继续叠加修改。

## 1. 安装前向用户确认配置

**不要直接使用默认值安装。** 先完成第 2 节探测，再把下面这组问题一次性发给用户；不要逐条打断。用户回复“全部默认”时，才使用括号中的默认值。

```text
安装前请确认以下配置（可直接回复“全部默认”）：

1. 监听地址/端口：默认 [::]:8080。是否需要改成 0.0.0.0:8080 或其他端口？（填 IP:端口，不是域名）
2. 访问方式：仅内网 / 直连公网 / 经过 Nginx / Caddy / Cloudflare 等反向代理？（未回答按无代理处理）
3. 是否启用 Web 面板：默认启用。不需要可安装 nodashboard 版本。
4. 是否启用 MCP 管理端点 /mcp：需要 / 不需要（写操作，需要 Admin Token；未回答按不需要处理）。
5. 趋势数据：默认持久化到 /var/lib/rustracker/trends.jsonl；可改路径或禁用。
6. Announce 间隔 / Peer 超时：默认 1800 / 3000 秒；是否调整？
7. 如果服务器上已有 rustracker，是否升级现有服务？（全新安装请忽略）
8. 是否需要开放防火墙端口：是 / 否（只给建议，不擅自修改）。
```

把回答映射到安装参数：

| 用户答案 | 安装参数 |
|---|---|
| 监听地址/端口 | `RUSTRACKER_LISTEN=<值>` |
| 反向代理后 | `RUSTRACKER_TRUST_PROXY_HEADERS=true` |
| 仅内网 / 直连公网 / 无代理 | `RUSTRACKER_TRUST_PROXY_HEADERS=false` |
| 需要 Web 面板（默认） | 下载 `rustracker-linux` |
| 不需要 Web 面板 | 下载 `rustracker-linux-nodashboard` |
| 需要 MCP | 不传该变量，或 `RUSTRACKER_NO_MCP=false` |
| 不需要 MCP | `RUSTRACKER_NO_MCP=true` |
| 趋势数据路径 | `RUSTRACKER_TRENDS_FILE=<路径>` |
| 禁用趋势数据 | `RUSTRACKER_TRENDS_FILE=none` |
| 自定义间隔/超时 | `RUSTRACKER_INTERVAL_SECS=<秒>` / `RUSTRACKER_PEER_TIMEOUT_SECS=<秒>` |
| 升级现有安装 | 先按第 7 节备份，再执行第 3 节 |

示例：用户回答“监听 `0.0.0.0:8080`、Cloudflare 反代、启用面板、不需要 MCP、趋势和间隔都用默认、全新安装、不开放防火墙”，则第 3 节执行：

```bash
sudo env \
  RUSTRACKER_LISTEN='0.0.0.0:8080' \
  RUSTRACKER_TRUST_PROXY_HEADERS='true' \
  RUSTRACKER_NO_MCP='true' \
  sh /tmp/rustracker-agent-install/install-linux.sh install --yes
```

注意：

- **不要问用户 Admin Token 明文。** 安装器会在服务器上自动生成；需要自定义 token 时，让用户自己在服务器上写入 `/etc/rustracker.env`。
- **不要问 sudo 密码、SSH 私钥或其他敏感凭据。** 如果 sudo 需要密码，让用户手动执行命令。
- “全部默认”表示接受安装脚本默认值（包括信任代理头和启用 MCP）。如果服务器直连公网或不需要 MCP，不要让用户选“全部默认”。
- 普通项（监听、间隔、超时、趋势）未回答时使用默认值；安全相关项（访问方式、MCP、防火墙）必须得到明确回答。用户不回答时按保守值处理：`RUSTRACKER_TRUST_PROXY_HEADERS=false`、`RUSTRACKER_NO_MCP=true`、不修改防火墙。
- 在第 5 节汇报中明确列出实际使用的值，包括用户没有回答、由 agent 采用默认值的项。
- 如果用户要求公网访问，先确认防火墙/TLS 方案；不要自行开放端口。

## 2. 安装前探测

按顺序检查并记录结果：

```bash
uname -srm
cat /etc/os-release || true
command -v systemctl || true
command -v curl || true
id -u
sudo -n true && echo "sudo=passwordless" || echo "sudo=needs-password-or-unavailable"
ss -ltnp | grep ':8080 ' || true
test -f /etc/systemd/system/rustracker.service && echo "existing-service" || true
test -x /opt/rustracker/rustracker && echo "existing-binary" || true
```

根据结果决策：

| 检测结果 | 动作 |
|---|---|
| `uname -s` 不是 `Linux` | 停止，告知本文只覆盖 Linux |
| `uname -m` 不是 `x86_64` | 走第 6 节源码构建 |
| 没有 `systemctl` | 停止，让用户选择其他进程管理方式 |
| 没有 `curl` | 先用系统包管理器安装 `curl` |
| 非 root，且 sudo 需要密码或不可用 | 停止，让用户提供 root/sudo，或把命令交给用户手动执行 |
| 已存在 rustracker 服务 | 先问用户是否升级；升级前备份，见第 7 节 |
| 8080 被其他进程占用 | 不要 kill；询问用户换端口或处理占用进程 |
| 已存在 `/etc/rustracker.env` | 安装器会读取其中可识别的值（token、监听、趋势等）作为默认值；先确认用户是否要覆盖配置 |

用户确定端口后，再检查一次该端口是否被占用：

```bash
ss -ltnp | grep ':<端口> ' || true
```

## 3. 执行安装

先完成第 2 节探测，并确认第 1 节的问题已得到答复。以下命令整段执行。如果当前用户是 root 且系统没有 `sudo`，去掉命令中的 `sudo`。

### 3.1 下载二进制和安装器

```bash
set -euo pipefail

[ "$(uname -s)" = "Linux" ] || { echo "仅支持 Linux"; exit 1; }
[ "$(uname -m)" = "x86_64" ] || { echo "非 x86_64，请走第 6 节源码构建"; exit 1; }

mkdir -p /tmp/rustracker-agent-install

# 需要 Web 面板（默认）
curl -fsSL -o /tmp/rustracker-agent-install/rustracker-linux \
  https://github.com/LOSRET/Rustracker/releases/latest/download/rustracker-linux

# 不需要 Web 面板时，改用下面这一行（文件名仍叫 rustracker-linux）：
# curl -fsSL -o /tmp/rustracker-agent-install/rustracker-linux \
#   https://github.com/LOSRET/Rustracker/releases/latest/download/rustracker-linux-nodashboard

curl -fsSL -o /tmp/rustracker-agent-install/install-linux.sh \
  https://raw.githubusercontent.com/LOSRET/Rustracker/main/install-linux.sh
chmod +x /tmp/rustracker-agent-install/rustracker-linux
```

### 3.2 执行安装

只有用户明确回复“全部默认”时，才能直接执行：

```bash
sudo sh /tmp/rustracker-agent-install/install-linux.sh install --yes
```

否则按用户回答拼命令。下面是完整模板；实际执行时只保留用户确认要覆盖的变量，其余删掉：

```bash
sudo env \
  RUSTRACKER_LISTEN='0.0.0.0:8080' \
  RUSTRACKER_INTERVAL_SECS='1800' \
  RUSTRACKER_PEER_TIMEOUT_SECS='3000' \
  RUSTRACKER_TRENDS_FILE='/var/lib/rustracker/trends.jsonl' \
  RUSTRACKER_TRUST_PROXY_HEADERS='false' \
  RUSTRACKER_NO_MCP='true' \
  sh /tmp/rustracker-agent-install/install-linux.sh install --yes
```

配置项：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `RUSTRACKER_LISTEN` | `[::]:8080` | 监听地址（IP:端口）；只写端口时自动补成 `[::]:端口` |
| `RUSTRACKER_INTERVAL_SECS` | `1800` | 返回给 peer 的 announce 间隔 |
| `RUSTRACKER_PEER_TIMEOUT_SECS` | `3000` | peer 过期时间 |
| `RUSTRACKER_TRENDS_FILE` | `/var/lib/rustracker/trends.jsonl` | 趋势数据路径；安装器会创建父目录；设为 `none` 禁用持久化 |
| `RUSTRACKER_TRUST_PROXY_HEADERS` | `true` | 仅在可信反向代理后设为 `true`；直连公网应设为 `false` |
| `RUSTRACKER_NO_MCP` | `false` | 设为 `true` 禁用 `/mcp` |

默认结果（用户没有覆盖时）：

| 项目 | 默认值 |
|---|---|
| 监听地址 | `[::]:8080` |
| Announce 间隔 | `1800` 秒 |
| Peer 超时 | `3000` 秒 |
| 趋势数据 | `/var/lib/rustracker/trends.jsonl` |
| 信任代理头 | `true` |
| MCP `/mcp` | 启用 |
| Admin Token | 自动生成 64 位十六进制 |

注意：

- `RUSTRACKER_TRUST_PROXY_HEADERS` 在安装脚本中默认是 `true`，但二进制 CLI 默认是 `false`。直连公网时必须显式传 `false`，否则客户端可以伪造来源 IP 头。
- **不要在命令行传 `RUSTRACKER_ADMIN_TOKEN`**，它会出现在进程列表中。需要自定义 token 时，先写 `/etc/rustracker.env`（权限 `0600`），再执行 `install --yes`；安装器会保留其中已有的 token、监听、趋势等值。
- MCP 是写操作接口。用户没有明确要求时，建议设置 `RUSTRACKER_NO_MCP=true`；需要时保留默认，并确保通过 HTTPS 访问。
- 安装器从仓库 `main` 获取，是为了立即获得 `install --yes`；旧 release 包内的安装器可能还不支持。二进制仍来自最新 release，两者兼容。
- 如果 `raw.githubusercontent.com` 无法访问，可先 `git clone --depth 1 https://github.com/LOSRET/Rustracker.git`，使用仓库内的 `install-linux.sh`；不要从第三方镜像下载安装器。

安装完成后执行第 4 节验证，然后按第 5 节向用户汇报。

## 4. 安装后验证

以下以默认端口 8080 为例；如果用户选了其他端口，替换命令中的 8080。如果监听地址不是本机回环，把 `127.0.0.1` 换成实际地址。

```bash
systemctl is-active rustracker
systemctl is-enabled rustracker
curl -fsS http://127.0.0.1:8080/healthz || curl -g -fsS 'http://[::1]:8080/healthz'
curl -fsS http://127.0.0.1:8080/api/stats
curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/
ss -ltnp | grep ':8080 ' || true
sudo grep -v '^RUSTRACKER_ADMIN_TOKEN=' /etc/rustracker.env
```

预期：

- `systemctl is-active` 输出 `active`
- `systemctl is-enabled` 输出 `enabled`
- `/healthz` 输出 `ok`
- `/api/stats` 返回 JSON，包含 `"version"` 字段
- `/` 返回 `200`（使用 `rustracker-linux-nodashboard` 时返回 `404`，属正常）
- `ss -ltnp | grep ':8080 '` 能看到 rustracker 监听
- `/etc/rustracker.env` 中的监听地址、间隔、超时、趋势路径、代理头、MCP 开关与用户回答一致

任何一项不符合预期，先看第 8 节，不要继续改配置。

## 5. 向用户汇报

安装成功后，按以下格式汇报，不要输出 Admin Token：

```text
Rustracker 已安装并启动。
- 服务：rustracker（active / enabled）
- 监听：<实际监听地址>
- Web 面板：<启用 / 未启用>
- 面板地址：http://<服务器IP>:<实际端口>/
- Announce 地址：http://<服务器IP>:<实际端口>/announce
- 反向代理信任头：<true / false>
- 趋势数据：<实际路径 / 已禁用>
- MCP：<已启用（/mcp）/ 已禁用>
- 配置：/etc/rustracker.env
- 下一步：如需公网访问，开放 <实际端口>/TCP，并建议在前面加 HTTPS 反向代理。
```

补充说明：

- Admin Token 查看方式：`sudo grep '^RUSTRACKER_ADMIN_TOKEN=' /etc/rustracker.env`。只在用户明确要求时执行，不要把结果发到聊天。
- 如果服务器有公网 IP 或域名，提醒用户配置防火墙和 TLS。除非用户明确同意，不要自行修改防火墙。
- 公开运营前，确认用户了解当地法律和内容投诉流程；面板自带免责声明。

## 6. 源码构建（无 x86_64 发布包或用户要求时）

前置检查：

```bash
rustc --version   # 需要 1.85+
node --version    # 需要 20.19+ 或 22.12+，建议 22/24 LTS
npm --version
```

在有 dashboard 的完整构建中，`build.rs` 会调用前端的 Vite 构建，因此必须先安装前端依赖。整段执行：

```bash
set -euo pipefail

rm -rf /tmp/rustracker-src
git clone --depth 1 https://github.com/LOSRET/Rustracker.git /tmp/rustracker-src

cd /tmp/rustracker-src
npm ci --prefix frontend
cargo build --release

sudo sh /tmp/rustracker-src/install-linux.sh install --yes
```

说明：

- `install-linux.sh` 会在脚本同目录的 `target/release/rustracker` 找到二进制，无需额外指定路径。
- 不需要 Web 面板时：

  ```bash
  cargo build --release --no-default-features
  ```

  此时 `/` 和 `/assets/*` 返回 `404`，`/announce`、`/scrape`、`/healthz`、`/api/*` 正常。
- ARM（`aarch64`）请在 ARM 机器上本机构建；跨架构交叉编译不在本文范围。
- 不要用 `sudo` 执行 `cargo build` 或 `npm ci`，只用 `sudo` 运行安装器。

## 7. 升级

只有用户确认后才能覆盖现有安装。本节适用于 x86_64 发布包安装；源码安装（尤其是 ARM）请重新构建后重跑安装器。

```bash
set -euo pipefail

# 1. 备份二进制和配置
sudo cp -a /opt/rustracker/rustracker "/opt/rustracker/rustracker.bak.$(date +%Y%m%d%H%M%S)"
sudo cp -a /etc/rustracker.env "/etc/rustracker.env.bak.$(date +%Y%m%d%H%M%S)"

# 2. 下载新二进制和安装器
mkdir -p /tmp/rustracker-agent-install
curl -fsSL -o /tmp/rustracker-agent-install/rustracker-linux \
  https://github.com/LOSRET/Rustracker/releases/latest/download/rustracker-linux
curl -fsSL -o /tmp/rustracker-agent-install/install-linux.sh \
  https://raw.githubusercontent.com/LOSRET/Rustracker/main/install-linux.sh

# 3. 重跑安装，保留 /etc/rustracker.env、默认黑名单文件和趋势数据
sudo sh /tmp/rustracker-agent-install/install-linux.sh install --yes

# 4. 验证
systemctl is-active rustracker
curl -fsS http://127.0.0.1:8080/healthz
curl -fsS http://127.0.0.1:8080/api/stats
```

回滚：

```bash
set -euo pipefail
BACKUP=$(ls -1t /opt/rustracker/rustracker.bak.* | head -n 1)
ENV_BACKUP=$(ls -1t /etc/rustracker.env.bak.* | head -n 1)
sudo cp -a "$BACKUP" /opt/rustracker/rustracker
sudo cp -a "$ENV_BACKUP" /etc/rustracker.env
sudo systemctl restart rustracker
```

## 8. 故障排查

| 现象 | 检查 | 处理 |
|---|---|---|
| `systemctl` 不存在 | `command -v systemctl` | 停止；本文只支持 systemd |
| 安装器找不到二进制 | 脚本同目录是否有 `rustracker-linux` | 用 `sudo env RUSTRACKER_BINARY=/绝对路径 sh /tmp/rustracker-agent-install/install-linux.sh install --yes` 指定 |
| `Permission denied` | 是否使用 root/sudo | 用 `sudo` 重跑安装器 |
| `curl: (22)` 或 404 | 版本/资产名是否正确 | 确认 release 中存在 `rustracker-linux` |
| `curl: (60)` SSL 错误 | CA 证书 | 安装 `ca-certificates`；不要关闭证书校验 |
| 8080 被占用 | `ss -ltnp \| grep ':8080 '` | 换端口或让用户处理，不要 kill 进程 |
| 服务启动后立即退出 | `journalctl -u rustracker -n 100 --no-pager` | 常见为端口占用、权限、配置错误 |
| 启动日志出现 `Illegal instruction` | CPU 是否支持 x86-64-v2 | 极老 x86_64 CPU 不支持发布包，改用第 6 节源码构建 |
| `/healthz` 不通 | `ss -ltnp`、防火墙、监听地址 | 检查监听地址和防火墙规则 |
| 面板 `/` 返回 404 | 是否用了 `rustracker-linux-nodashboard` | 换完整版 `rustracker-linux` |
| `/mcp` 返回 404 | 是否配置 token、是否 `RUSTRACKER_NO_MCP=true` | 检查 `/etc/rustracker.env`，重启服务 |
| `/mcp` 返回 401 | Bearer token 是否正确 | 从 `/etc/rustracker.env` 读取，不要猜 |
| `npm ci` 失败 | Node 版本、`frontend/package-lock.json` | 升级 Node 到 20.19+ 或 22.12+，不要删锁文件 |
| `cargo build` 失败 | Rust 版本、磁盘空间 | 升级 Rust 到 1.85+，清理磁盘 |
| ARM 构建失败 | 目标架构 | 在 ARM 机器上本机构建，或使用 `aarch64-unknown-linux-gnu` 工具链 |

查看日志：

```bash
sudo journalctl -u rustracker -n 100 --no-pager
sudo journalctl -u rustracker -f
```

## 9. 文件布局与运维命令

| 路径 | 说明 |
|---|---|
| `/opt/rustracker/rustracker` | 二进制 |
| `/etc/rustracker.env` | 环境配置（含 Admin Token，权限 `0600`） |
| `/etc/systemd/system/rustracker.service` | systemd 单元 |
| `/etc/rustracker/blacklist.txt` | 种子黑名单 |
| `/etc/rustracker/blacklist.audit.jsonl` | 黑名单变更审计日志（首次写入时创建） |
| `/var/lib/rustracker/trends.jsonl` | 趋势数据 |
| `/etc/sysctl.d/99-rustracker.conf` | 安装器写入的内核网络参数 |
| `/etc/security/limits.d/rustracker.conf` | 文件描述符限制 |

常用命令：

```bash
sudo systemctl status rustracker --no-pager
sudo systemctl restart rustracker
sudo systemctl stop rustracker
sudo journalctl -u rustracker -f
```

## 10. 卸载

卸载是破坏性操作，**必须先得到用户明确确认**。安装器的 `uninstall` 是交互式的，会逐项询问是否删除程序目录、配置和黑名单；不要替用户回答。

让用户手动执行（路径换成实际安装时使用的脚本；源码安装是 `/tmp/rustracker-src/install-linux.sh`）：

```bash
sudo sh /tmp/rustracker-agent-install/install-linux.sh uninstall
```

如果用户明确要求 agent 执行，也要逐项向用户确认后再继续。删除后检查：

```bash
systemctl status rustracker --no-pager || true
test -f /etc/systemd/system/rustracker.service && echo "service-file-remains"
```

## 11. 不要做的事

- 不要在未向用户确认配置的情况下直接执行默认安装。
- 不要 `apt install rustracker`、`yum install rustracker` 或 `docker run`：没有官方包/镜像。
- 不要在用 `rustracker-linux-nodashboard` 安装后期待 Web 面板。
- 不要 kill 占用 8080 的进程。
- 不要修改 systemd 单元里的 `User=root`，除非用户明确要求并理解文件权限影响。
- 不要把 `RUSTRACKER_ADMIN_TOKEN` 写进命令行、聊天记录或日志。
- 不要在未确认的情况下修改防火墙或把服务暴露到公网。
- 不要把 `RUSTRACKER_TRUST_PROXY_HEADERS` 设为 `true` 后直接暴露公网。
