#!/usr/bin/env sh
set -eu

APP_NAME="rustracker"
INSTALL_DIR="/opt/rustracker"
BIN_PATH="$INSTALL_DIR/rustracker"
ENV_PATH="/etc/rustracker.env"
SERVICE_PATH="/etc/systemd/system/rustracker.service"
DEFAULT_LISTEN="0.0.0.0:8080"
DEFAULT_INTERVAL="1800"
DEFAULT_TIMEOUT="3000"
DEFAULT_MALLOC_CONF="background_thread:true,dirty_decay_ms:5000,muzzy_decay_ms:5000,narenas:4"
BLACKLIST_PATH="/etc/rustracker/blacklist.txt"
TRENDS_FILE_PATH="/var/lib/rustracker/trends.jsonl"

load_existing_config() {
    LISTEN_DEFAULT=$DEFAULT_LISTEN
    INTERVAL_DEFAULT=$DEFAULT_INTERVAL
    TIMEOUT_DEFAULT=$DEFAULT_TIMEOUT
    MALLOC_CONF_DEFAULT=$DEFAULT_MALLOC_CONF

    if [ -f "$ENV_PATH" ]; then
        LISTEN_DEFAULT=$(grep '^RUSTRACKER_LISTEN=' "$ENV_PATH" | tail -n 1 | cut -d= -f2- || true)
        INTERVAL_DEFAULT=$(grep '^RUSTRACKER_INTERVAL_SECS=' "$ENV_PATH" | tail -n 1 | cut -d= -f2- || true)
        TIMEOUT_DEFAULT=$(grep '^RUSTRACKER_PEER_TIMEOUT_SECS=' "$ENV_PATH" | tail -n 1 | cut -d= -f2- || true)
        TRENDS_DEFAULT=$(grep '^RUSTRACKER_TRENDS_FILE=' "$ENV_PATH" | tail -n 1 | cut -d= -f2- || true)
        ADMIN_TOKEN_DEFAULT=$(grep '^RUSTRACKER_ADMIN_TOKEN=' "$ENV_PATH" | tail -n 1 | cut -d= -f2- || true)
        MALLOC_CONF_DEFAULT=$(grep '^MALLOC_CONF=' "$ENV_PATH" | tail -n 1 | cut -d= -f2- || true)

        LISTEN_DEFAULT=${LISTEN_DEFAULT:-$DEFAULT_LISTEN}
        INTERVAL_DEFAULT=${INTERVAL_DEFAULT:-$DEFAULT_INTERVAL}
        TIMEOUT_DEFAULT=${TIMEOUT_DEFAULT:-$DEFAULT_TIMEOUT}
        TRENDS_DEFAULT=${TRENDS_DEFAULT:-$TRENDS_FILE_PATH}
        MALLOC_CONF_DEFAULT=${MALLOC_CONF_DEFAULT:-$DEFAULT_MALLOC_CONF}
    else
        TRENDS_DEFAULT=$TRENDS_FILE_PATH
        ADMIN_TOKEN_DEFAULT=""
    fi
}

normalize_listen() {
    case "$1" in
        *:*) printf '%s\n' "$1" ;;
        *[!0-9]*|'') printf '%s\n' "$1" ;;
        *) printf '0.0.0.0:%s\n' "$1" ;;
    esac
}

generate_admin_token() {
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -hex 32
    else
        od -An -N32 -tx1 /dev/urandom | tr -d ' \n'
    fi
}

need_root() {
    if [ "$(id -u)" != "0" ]; then
        echo "请使用 root 权限运行，例如：sudo sh install-linux.sh"
        exit 1
    fi
}

pause() {
    printf "\n按 Enter 返回菜单..."
    # shellcheck disable=SC2162
    read _
}

find_binary() {
    if [ -n "${RUSTRACKER_BINARY:-}" ] && [ -f "$RUSTRACKER_BINARY" ]; then
        printf '%s\n' "$RUSTRACKER_BINARY"
        return 0
    fi

    SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
    for candidate in \
        "$SCRIPT_DIR/rustracker-linux" \
        "$SCRIPT_DIR/rustracker-linux-nodashboard" \
        "$SCRIPT_DIR/rustracker" \
        "$SCRIPT_DIR/target/x86_64-unknown-linux-gnu/release/rustracker" \
        "$SCRIPT_DIR/target/x86_64-unknown-linux-musl/release/rustracker" \
        "$SCRIPT_DIR/target/release/rustracker"
    do
        if [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    echo "未找到 Linux 二进制文件。请把 rustracker-linux、rustracker-linux-nodashboard 或 rustracker 放在脚本同目录。" >&2
    return 1
}

write_env() {
    load_existing_config

    printf "监听地址或端口 [%s]: " "$LISTEN_DEFAULT"
    read LISTEN || true
    LISTEN=${LISTEN:-$LISTEN_DEFAULT}
    LISTEN=$(normalize_listen "$LISTEN")

    printf "上报间隔秒数 [%s]: " "$INTERVAL_DEFAULT"
    read INTERVAL || true
    INTERVAL=${INTERVAL:-$INTERVAL_DEFAULT}

    printf "Peer 超时秒数 [%s]: " "$TIMEOUT_DEFAULT"
    read TIMEOUT || true
    TIMEOUT=${TIMEOUT:-$TIMEOUT_DEFAULT}

    printf "趋势数据落盘路径（输入 none 禁用）[%s]: " "$TRENDS_DEFAULT"
    read TRENDS || true
    TRENDS=${TRENDS:-$TRENDS_DEFAULT}
    case "$TRENDS" in
        none|NONE|None) TRENDS="" ;;
    esac

    ADMIN_TOKEN=${ADMIN_TOKEN_DEFAULT:-$(generate_admin_token)}

    cat > "$ENV_PATH" <<EOF
RUSTRACKER_LISTEN=$LISTEN
RUSTRACKER_INTERVAL_SECS=$INTERVAL
RUSTRACKER_PEER_TIMEOUT_SECS=$TIMEOUT
RUSTRACKER_BLACKLIST=$BLACKLIST_PATH
RUSTRACKER_TRENDS_FILE=$TRENDS
RUSTRACKER_ADMIN_TOKEN=$ADMIN_TOKEN
MALLOC_CONF=$MALLOC_CONF_DEFAULT
EOF
    chmod 0600 "$ENV_PATH"
}

write_service() {
    cat > "$SERVICE_PATH" <<EOF
[Unit]
Description=Rustracker BitTorrent tracker
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=$ENV_PATH
ExecStart=$BIN_PATH
Restart=on-failure
RestartSec=3
User=root
WorkingDirectory=$INSTALL_DIR

[Install]
WantedBy=multi-user.target
EOF
}

install_app() {
    need_root
    BINARY=$(find_binary)
    TMP_BIN="$INSTALL_DIR/rustracker.new"

    mkdir -p "$INSTALL_DIR"

    if [ -f "$SERVICE_PATH" ] && command -v systemctl >/dev/null 2>&1; then
        systemctl stop "$APP_NAME" >/dev/null 2>&1 || true
    fi

    cp "$BINARY" "$TMP_BIN"
    chmod 0755 "$TMP_BIN"
    mv -f "$TMP_BIN" "$BIN_PATH"

    write_env
    write_service

    if [ ! -f "$BLACKLIST_PATH" ]; then
        mkdir -p "$(dirname "$BLACKLIST_PATH")"
        printf '# rustracker torrent blacklist\n# one 40-char hex info_hash per line\n' > "$BLACKLIST_PATH"
    fi

    mkdir -p "$(dirname "$TRENDS_FILE_PATH")"

    systemctl daemon-reload
    systemctl enable "$APP_NAME" >/dev/null 2>&1 || true
    systemctl restart "$APP_NAME"

    echo "安装完成。"
    echo "服务：$APP_NAME"
    echo "配置：$ENV_PATH"
    echo "黑名单：$BLACKLIST_PATH"
    echo "趋势数据：$TRENDS_FILE_PATH"
    echo "目录：$INSTALL_DIR"
}

configure_app() {
    need_root
    write_env

    if [ -f "$SERVICE_PATH" ]; then
        systemctl daemon-reload
        systemctl restart "$APP_NAME" >/dev/null 2>&1 || true
    fi

    echo "配置已更新。"
    echo "配置：$ENV_PATH"
}

uninstall_app() {
    need_root
    systemctl stop "$APP_NAME" >/dev/null 2>&1 || true
    systemctl disable "$APP_NAME" >/dev/null 2>&1 || true
    rm -f "$SERVICE_PATH"
    systemctl daemon-reload

    printf "是否删除程序目录 %s？[y/N]: " "$INSTALL_DIR"
    read CONFIRM || true
    case "$CONFIRM" in
        y|Y|yes|YES) rm -rf "$INSTALL_DIR" ;;
    esac

    printf "是否删除配置文件 %s？[y/N]: " "$ENV_PATH"
    read CONFIRM || true
    case "$CONFIRM" in
        y|Y|yes|YES) rm -f "$ENV_PATH" ;;
    esac

    printf "是否删除黑名单文件 %s？[y/N]: " "$BLACKLIST_PATH"
    read CONFIRM || true
    case "$CONFIRM" in
        y|Y|yes|YES) rm -f "$BLACKLIST_PATH" ;;
    esac

    echo "卸载完成。"
}

service_action() {
    need_root
    ACTION=$1
    systemctl "$ACTION" "$APP_NAME"
}

show_status() {
    if command -v systemctl >/dev/null 2>&1; then
        systemctl status "$APP_NAME" --no-pager || true
    else
        echo "当前系统没有 systemctl。"
    fi
}

show_config() {
    if [ -f "$ENV_PATH" ]; then
        echo "当前配置："
        grep -v '^RUSTRACKER_ADMIN_TOKEN=' "$ENV_PATH" || true
        if grep -q '^RUSTRACKER_ADMIN_TOKEN=' "$ENV_PATH"; then
            echo "RUSTRACKER_ADMIN_TOKEN=<hidden>"
        fi
    else
        echo "未找到配置文件：$ENV_PATH"
    fi
    if [ -f "$BLACKLIST_PATH" ]; then
        COUNT=$(grep -cvE '^#|^$' "$BLACKLIST_PATH" 2>/dev/null || echo 0)
        echo ""
        echo "黑名单文件：$BLACKLIST_PATH（$COUNT 条）"
    fi
    if [ -f "$TRENDS_FILE_PATH" ]; then
        LINES=$(wc -l < "$TRENDS_FILE_PATH" 2>/dev/null || echo 0)
        SIZE=$(du -h "$TRENDS_FILE_PATH" 2>/dev/null | cut -f1)
        echo ""
        echo "趋势数据：$TRENDS_FILE_PATH（$LINES 行，$SIZE）"
    fi
}

show_admin_token() {
    need_root

    if [ ! -f "$ENV_PATH" ]; then
        echo "未找到配置文件：$ENV_PATH"
        return
    fi

    TOKEN=$(grep '^RUSTRACKER_ADMIN_TOKEN=' "$ENV_PATH" | tail -n 1 | cut -d= -f2- || true)
    if [ -z "$TOKEN" ]; then
        echo "未配置 RUSTRACKER_ADMIN_TOKEN。"
    else
        echo "$TOKEN"
    fi
}

menu() {
    while :; do
        clear 2>/dev/null || true
        echo "===================================="
        echo " rustracker Linux 安装菜单"
        echo "===================================="
        echo "1) 安装或更新"
        echo "2) 卸载"
        echo "3) 启动服务"
        echo "4) 停止服务"
        echo "5) 重启服务"
        echo "6) 查看状态"
        echo "7) 查看配置"
        echo "8) 修改配置"
        echo "9) 查看 Admin Token"
        echo "0) 退出"
        printf "请选择："
        read CHOICE || exit 0

        case "$CHOICE" in
            1) install_app; pause ;;
            2) uninstall_app; pause ;;
            3) service_action start; pause ;;
            4) service_action stop; pause ;;
            5) service_action restart; pause ;;
            6) show_status; pause ;;
            7) show_config; pause ;;
            8) configure_app; pause ;;
            9) show_admin_token; pause ;;
            0) exit 0 ;;
            *) echo "无效选择。"; pause ;;
        esac
    done
}

case "${1:-menu}" in
    install) install_app ;;
    uninstall) uninstall_app ;;
    start) service_action start ;;
    stop) service_action stop ;;
    restart) service_action restart ;;
    status) show_status ;;
    config) show_config ;;
    configure) configure_app ;;
    token) show_admin_token ;;
    menu) menu ;;
    *)
        echo "用法：sh install-linux.sh [menu|install|uninstall|start|stop|restart|status|config|configure|token]"
        exit 1
        ;;
esac
