        const state = { data: null, trendsData: null, clientData: null, range: "24h", lang: navigator.language.startsWith("zh") ? "zh" : "en", page: "dashboard", top100Data: null, top100Sort: "peers" };
        const $ = (id) => document.getElementById(id);
        const escapeHtml = (s) => String(s).replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;");
        const localeFor = () => state.lang === "zh" ? "zh-CN" : "en-US";
        const chart = echarts.init($("trendChart"), null, { renderer: "canvas" });
        const clientChart = echarts.init($("clientChart"), null, { renderer: "canvas" });

        const T = {
            zh: {
                monitoring: "监控", overview: "Tracker 概览", running: "运行中",
                error: "异常", paused_state: "已暂停", disc_link: "Tracker 免责说明",
                view: "查看", side_note: "HTTP tracker 的连接、做种、下载和完成统计。",
                title: "Tracker 控制台", subtitle: "查看当前端口上的 peer、做种和下载状态。",
                loading: "正在加载...", last_update: "最后更新", read_error: "读取失败",
                chart_title: "Tracker 趋势",
                chart_note: "Torrents、Peers、Seeders 和 Leechers 随时间变化",
                client_chart_title: "客户端分布",
                client_chart_note: "前 15 客户端的 Peer 数量随时间变化",
                range_24h: "24小时", range_3d: "3天", range_7d: "7天",
                top100_link: "Top 100 Torrents", top100_title: "🏆 Top 100 Torrents",
                top100_subtitle: "按 Peers / Seeders / Leechers 数量排序",
                sort_peers: "Peers", sort_seeders: "Seeders", sort_leechers: "Leechers", sort_downloaded: "Downloaded",
                col_hash: "Info Hash", top100_loading: "加载中...", top100_empty: "暂无数据", top100_error: "读取失败",
                refresh: "刷新",
                tracker_addr_label: "Tracker 地址：",
                config_fmt: () => `${window.location.origin}/announce`,
                copied: "已复制",
                disc_title: "Tracker 免责说明",
                disc_p1: "本站 Tracker 仅提供连接协调、状态记录与统计展示，不存储、不托管、不分发任何实际资源内容。",
                disc_p2: "页面中的 torrents、peers、seeders、leechers、客户端类型及趋势数据，来源于客户端上报与系统采样，可能存在延迟、缺失、偏差或伪造，不代表资源真实状态。",
                disc_p3: "本页面信息不代表任何资源的真实性、完整性、可用性、安全性或合法性，也不构成任何服务承诺或结果保证。",
                disc_p4: "对于第三方客户端行为、资源内容、传输结果及由此产生的任何直接或间接后果，本站不承担责任，使用者应自行判断并承担相关风险。",
                disc_p5: "受 Tracker 工作机制限制，本站不保留可用于长期识别、追踪或还原单个连接历史的完整日志，也无法对既往连接行为提供持续、完整或可验证的回溯记录。",
                blog_label: "Blog", contact_label: "如有问题请联系",
                seo_title: "BitTorrent Tracker – HTTP Tracker 实时监控面板",
                seo_desc: "BitTorrent HTTP Tracker 实时监控面板，查看 Peers、Seeders、Leechers、客户端分布与趋势图表。提供 announce/scrape 接口，支持 IPv4/IPv6。",
            },
            en: {
                monitoring: "Monitoring", overview: "Tracker Overview", running: "Running",
                error: "Error", paused_state: "Paused", disc_link: "Disclaimer",
                view: "View", side_note: "HTTP tracker connection, seeding, downloading and completion statistics.",
                title: "Tracker Console", subtitle: "View peer, seeding and download status on the current port.",
                loading: "Loading...", last_update: "Last updated", read_error: "Read failed",
                chart_title: "Tracker Trends",
                chart_note: "Torrents, Peers, Seeders and Leechers over time",
                client_chart_title: "Client Distribution",
                client_chart_note: "Top 15 clients by peer count over time",
                range_24h: "24h", range_3d: "3D", range_7d: "7D",
                top100_link: "Top 100 Torrents", top100_title: "🏆 Top 100 Torrents",
                top100_subtitle: "Sorted by Peers / Seeders / Leechers count",
                sort_peers: "Peers", sort_seeders: "Seeders", sort_leechers: "Leechers", sort_downloaded: "Downloaded",
                col_hash: "Info Hash", top100_loading: "Loading...", top100_empty: "No data", top100_error: "Read failed",
                refresh: "Refresh",
                tracker_addr_label: "Tracker URL: ",
                config_fmt: () => `${window.location.origin}/announce`,
                copied: "Copied!",
                disc_title: "Disclaimer",
                disc_p1: "This tracker only provides connection coordination, status recording and statistical display. It does not store, host or distribute any actual resource content.",
                disc_p2: "Torrents, peers, seeders, leechers, client types and trend data displayed on this page are derived from client reports and system sampling. They may contain delays, omissions, deviations or falsification, and do not represent the true state of resources.",
                disc_p3: "The information on this page does not represent the authenticity, completeness, availability, security or legality of any resource, nor does it constitute any service commitment or result guarantee.",
                disc_p4: "This site assumes no responsibility for third-party client behavior, resource content, transmission results, or any direct or indirect consequences arising therefrom. Users should exercise their own judgment and bear associated risks.",
                disc_p5: "Due to tracker operational limitations, this site does not retain complete logs that could be used for long-term identification, tracking or reconstruction of individual connection histories, and cannot provide continuous, complete or verifiable retrospective records of past connection activity.",
                blog_label: "Blog", contact_label: "Contact",
                seo_title: "BitTorrent Tracker – Real-time HTTP Tracker Dashboard",
                seo_desc: "BitTorrent HTTP Tracker dashboard. Monitor Peers, Seeders, Leechers, client distribution and trend charts. Supports announce/scrape with IPv4/IPv6.",
            }
        };

        function t(key) { return (T[state.lang] || T.zh)[key] ?? T.zh[key] ?? key; }
        function tf(key, ...a) { const f = (T[state.lang] || T.zh)[key]; return typeof f === "function" ? f(...a) : key; }

        function setLang(lang) {
            state.lang = lang;
            document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
            document.querySelectorAll("[data-i18n]").forEach((el) => {
                const key = el.getAttribute("data-i18n");
                const val = (T[lang] || T.zh)[key];
                if (typeof val === "string") el.textContent = val;
            });
            const seo = T[lang] || T.zh;
            document.title = seo.seo_title;
            const setMeta = (sel, attr, val) => { const el = document.querySelector(sel); if (el) el.setAttribute(attr, val); };
            setMeta("meta[name='description']", "content", seo.seo_desc);
            setMeta("meta[property='og:title']", "content", seo.seo_title);
            setMeta("meta[property='og:description']", "content", seo.seo_desc);
            setMeta("meta[property='og:locale']", "content", lang === "zh" ? "zh_CN" : "en_US");
            setMeta("meta[name='twitter:title']", "content", seo.seo_title);
            setMeta("meta[name='twitter:description']", "content", seo.seo_desc);
            if (state.data) render();
            if (state.top100Data) renderTop100();
        }

        function number(value) {
            return new Intl.NumberFormat(state.lang === "zh" ? "zh-CN" : "en-US").format(value || 0);
        }

        function setStatus(text, error = false) {
            $("statusText").textContent = text;
            $("statusText").className = error ? "status-line error" : "status-line";
            $("navState").textContent = error ? t("error") : t("running");
        }

        async function loadDashboard() {
            try {
                const statsRes = await fetch("/api/stats", { cache: "no-store" });
                if (!statsRes.ok) throw new Error(`HTTP ${statsRes.status}`);
                state.data = await statsRes.json();
                render();
                setStatus(`${t("last_update")} ${new Date().toLocaleTimeString(localeFor())}`);
            } catch (error) {
                setStatus(`${t("read_error")}: ${escapeHtml(error.message)}`, true);
            }
        }

        async function loadCharts() {
            try {
                const [trendsRes, clientsRes] = await Promise.all([
                    fetch("/api/trends", { cache: "no-store" }),
                    fetch("/api/clients", { cache: "no-store" })
                ]);
                if (trendsRes.ok) state.trendsData = await trendsRes.json();
                if (clientsRes.ok) state.clientData = await clientsRes.json();
                renderChart();
                renderClientChart();
            } catch (error) {
                // Chart refresh failures are non-critical
            }
        }

        function formatUptime(secs) {
            const d = Math.floor(secs / 86400);
            const h = Math.floor((secs % 86400) / 3600);
            const m = Math.floor((secs % 3600) / 60);
            if (d > 0) return `${d}d ${h}h ${m}m`;
            if (h > 0) return `${h}h ${m}m`;
            return `${m}m`;
        }

        function render() {
            const data = state.data || {};
            $("metricPeers").textContent = number(data.peers);
            $("metricSeeders").textContent = number(data.seeders);
            $("metricLeechers").textContent = number(data.leechers);
            $("metricTorrents").textContent = number(data.torrents);
            $("metricCompleted").textContent = number(data.completed);
            $("configText").textContent = tf("config_fmt");
            $("footerVersion").textContent = data.version || "-";
            $("footerUptime").textContent = data.uptime_secs != null ? `Uptime: ${formatUptime(data.uptime_secs)}` : "-";
            renderChart();
            renderClientChart();
        }

        function filterHistory() {
            const history = state.trendsData?.history || [];
            if (!history.length) return history;
            const ranges = { "24h": 86400, "3d": 259200, "7d": 604800 };
            const secs = ranges[state.range] || 86400;
            const cutoff = Math.floor(Date.now() / 1000) - secs;
            return history.filter((item) => item.timestamp >= cutoff);
        }

        function renderChart() {
            const history = filterHistory();
            const labels = history.map((item) => new Date(item.timestamp * 1000).toLocaleString(localeFor(), {
                month: "2-digit",
                day: "2-digit",
                hour: "2-digit",
                minute: "2-digit",
                hour12: false
            }));
            chart.setOption({
                color: ["#2563eb", "#475569", "#15803d", "#b45309"],
                tooltip: { trigger: "axis" },
                legend: {
                    type: "scroll",
                    top: 0,
                    left: "center",
                    itemWidth: 16,
                    itemGap: 14,
                    textStyle: { fontSize: 11 },
                    data: ["Torrents", "Peers", "Seeders", "Leechers"]
                },
                grid: chartGrid(),
                xAxis: {
                    type: "category",
                    boundaryGap: false,
                    data: labels,
                    axisLine: { lineStyle: { color: "#d8dee8" } },
                    axisLabel: { color: "#64748b" }
                },
                yAxis: {
                    type: "value",
                    minInterval: 1,
                    axisLabel: { color: "#64748b" },
                    splitLine: { lineStyle: { color: "#e6ebf2" } }
                },
                series: [
                    { name: "Torrents", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.torrents) },
                    { name: "Peers", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.peers) },
                    { name: "Seeders", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.seeders) },
                    { name: "Leechers", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.leechers) }
                ]
            });
        }

        function chartGrid() {
            return { left: 4, right: 4, top: 52, bottom: 36, containLabel: true };
        }

        const CLIENT_BRAND = [
            ["Xunlei", "#1976D2"], ["迅雷", "#1976D2"],
            ["qBittorrent", "#2196F3"],
            ["Transmission", "#D32F2F"],
            ["Deluge", "#388E3C"],
            ["uTorrent", "#7CB342"], ["µTorrent", "#7CB342"],
            ["BitComet", "#FF8F00"],
            ["BiglyBT", "#00897B"],
            ["Vuze", "#1565C0"],
            ["aria2", "#455A64"],
            ["libTorrent", "#7E57C2"],
            ["BitTorrent", "#4CAF50"],
            ["rTorrent", "#C62828"],
            ["Tixati", "#E65100"],
            ["WebTorrent", "#00ACC1"],
            ["FrostWire", "#0097A7"],
            ["ktorrent", "#1E88E5"],
            ["LibreTorrent", "#43A047"],
            ["Flud", "#26A69A"],
            ["Motrix", "#6D28D9"],
            ["Picotorrent", "#66BB6A"]
        ];
        const UNKNOWN_GRAY = "#9E9E9E";

        function hashClientName(str) {
            let h = 0;
            for (let i = 0; i < str.length; i++) {
                h = ((h << 5) - h + str.charCodeAt(i)) | 0;
            }
            return Math.abs(h);
        }

        function resolveClientColor(name) {
            const trimmed = name.trim();
            if (!trimmed || /^unknown$/i.test(trimmed)) return UNKNOWN_GRAY;
            const lower = trimmed.toLowerCase();
            for (const [key, color] of CLIENT_BRAND) {
                if (lower.includes(key.toLowerCase())) return color;
            }
            const h = hashClientName(trimmed) % 360;
            return `hsl(${h}, 65%, 50%)`;
        }

        function renderClientChart() {
            const d = state.clientData || {};
            const names = d.clients || [];
            const topTags = d.tags || [];
            const history = d.history || [];
            if (!history.length || !names.length) {
                clientChart.setOption({
                    title: {
                        text: state.lang === "zh" ? "暂无数据" : "No data",
                        left: "center",
                        top: "center",
                        textStyle: { color: "#94a3b8", fontSize: 14, fontWeight: "normal" }
                    },
                    series: []
                });
                return;
            }
            const ranges = { "24h": 86400, "3d": 259200, "7d": 604800 };
            const secs = ranges[state.range] || 86400;
            const cutoff = Math.floor(Date.now() / 1000) - secs;
            const filtered = history.filter((item) => item.timestamp >= cutoff);
            const labels = filtered.map((item) => new Date(item.timestamp * 1000).toLocaleString(localeFor(), {
                month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false
            }));
            const series = names.map((name, i) => ({
                name,
                type: "line",
                smooth: true,
                showSymbol: false,
                data: filtered.map((item) => {
                    if (!item.tags) return (item.counts && item.counts[i]) || 0;
                    const ti = item.tags.indexOf(topTags[i]);
                    return ti >= 0 ? (item.counts[ti] || 0) : 0;
                })
            }));
            const resolvedColors = names.map((n) => resolveClientColor(n));
            clientChart.setOption({
                title: { text: "" },
                color: resolvedColors,
                tooltip: { trigger: "axis" },
                legend: { type: "scroll", top: 0, left: "center", itemWidth: 16, itemGap: 14, textStyle: { fontSize: 11 }, data: names },
                grid: chartGrid(),
                xAxis: {
                    type: "category", boundaryGap: false, data: labels,
                    axisLine: { lineStyle: { color: "#d8dee8" } },
                    axisLabel: { color: "#64748b" }
                },
                yAxis: {
                    type: "value", minInterval: 1,
                    axisLabel: { color: "#64748b" },
                    splitLine: { lineStyle: { color: "#e6ebf2" } }
                },
                series
            });
        }

        $("rangeGroup").addEventListener("click", (e) => {
            const btn = e.target.closest(".range-btn");
            if (!btn || btn.classList.contains("active")) return;
            $("rangeGroup").querySelectorAll(".range-btn").forEach((b) => b.classList.remove("active"));
            btn.classList.add("active");
            state.range = btn.dataset.range;
            loadCharts();
        });

        $("langSelect").addEventListener("change", (e) => setLang(e.target.value));

        /* ===== Click to copy tracker address ===== */
        $("configText").addEventListener("click", () => {
            const url = $("configText").textContent;
            if (!url || url === "-") return;
            navigator.clipboard.writeText(url).then(() => {
                $("configText").setAttribute("data-tooltip", t("copied"));
                $("configText").classList.add("copied");
                setTimeout(() => $("configText").classList.remove("copied"), 1200);
            });
        });

        /* ===== Top 100 refresh ===== */
        $("top100Refresh").addEventListener("click", () => {
            state.top100Data = null;
            loadTop100();
        });

        /* ===== Page switching ===== */
        window.switchPage = function(page) {
            state.page = page;
            $("pageDashboard").classList.toggle("page-hidden", page !== "dashboard");
            $("pageTop100").classList.toggle("page-hidden", page !== "top100");
            $("navDashboard").classList.toggle("active", page === "dashboard");
            $("navTop100").classList.toggle("active", page === "top100");
            if (page === "top100" && !state.top100Data) loadTop100();
            if (page === "dashboard") { chart.resize(); clientChart.resize(); }
        };

        /* ===== Top 100 ===== */
        async function loadTop100() {
            $("top100Refresh").disabled = true;
            $("top100Status").textContent = state.lang === "zh" ? "加载中..." : "Loading...";
            try {
                const res = await fetch(`/api/top100?limit=100`, { cache: "no-store" });
                if (!res.ok) throw new Error(`HTTP ${res.status}`);
                state.top100Data = await res.json();
                renderTop100();
                $("top100Status").textContent = `${t("last_update")} ${new Date().toLocaleTimeString(localeFor())}`;
            } catch (error) {
                const body = $("top100Body");
                body.innerHTML = `<tr><td colspan="6" class="top100-empty">${escapeHtml(t("top100_error") + ": " + error.message)}</td></tr>`;
                $("top100Status").textContent = t("top100_error");
            } finally {
                $("top100Refresh").disabled = false;
            }
        }

        function renderTop100() {
            const data = state.top100Data;
            const body = $("top100Body");
            const torrents = data ? data[state.top100Sort] : null;
            if (!torrents || !torrents.length) {
                body.innerHTML = `<tr><td colspan="6" class="top100-empty">${t("top100_empty")}</td></tr>`;
                return;
            }
            body.innerHTML = torrents.map((item, i) => {
                const h = escapeHtml(item.info_hash);
                return `<tr>
                    <td class="col-rank">${i + 1}</td>
                    <td class="col-hash" title="${h}"><code>${h}</code></td>
                    <td class="col-num">${number(item.peers)}</td>
                    <td class="col-num text-green">${number(item.seeders)}</td>
                    <td class="col-num text-amber">${number(item.leechers)}</td>
                    <td class="col-num text-violet">${number(item.downloaded)}</td>
                </tr>`;
            }).join("");
        }

        $("sortGroup").addEventListener("click", (e) => {
            const btn = e.target.closest(".sort-btn");
            if (!btn || btn.classList.contains("active")) return;
            $("sortGroup").querySelectorAll(".sort-btn").forEach((b) => b.classList.remove("active"));
            btn.classList.add("active");
            state.top100Sort = btn.dataset.sort;
            renderTop100();
        });

        $("langSelect").value = state.lang;
        setLang(state.lang);
        loadDashboard();
        loadCharts();
        let _resizeTimer;
        window.addEventListener("resize", () => { clearTimeout(_resizeTimer); _resizeTimer = setTimeout(() => { chart.resize(); clientChart.resize(); }, 150); });
        setInterval(loadDashboard, 5000);
        setInterval(loadCharts, 600000);

        /* ===== Mobile sidebar toggle ===== */
        const side = $("side"), overlay = $("overlay"), hamburger = $("hamburger");
        function closeSide() { side.classList.remove("open"); overlay.classList.remove("open"); }
        hamburger.addEventListener("click", () => { side.classList.toggle("open"); overlay.classList.toggle("open"); });
        overlay.addEventListener("click", closeSide);
        document.querySelectorAll(".nav-item").forEach(el => el.addEventListener("click", closeSide));