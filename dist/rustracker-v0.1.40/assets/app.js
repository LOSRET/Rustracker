        const state = { data: null, range: "24h", lang: navigator.language.startsWith("zh") ? "zh" : "en" };
        const $ = (id) => document.getElementById(id);
        const chart = echarts.init($("trendChart"), null, { renderer: "canvas" });

        const T = {
            zh: {
                monitoring: "监控", overview: "Tracker 概览", running: "运行中",
                error: "异常", paused_state: "已暂停", disc_link: "Tracker 免责说明",
                view: "查看", side_note: "HTTP tracker 的连接、做种、下载和完成统计。",
                title: "Tracker 控制台", subtitle: "查看当前端口上的 peer、做种和下载状态。",
                loading: "正在加载...", last_update: "最后更新", read_error: "读取失败",
                chart_title: "Tracker 趋势",
                chart_note: "Torrents、Peers、Seeders 和 Leechers 随时间变化",
                range_24h: "24小时", range_3d: "3天", range_7d: "7天",
                config_fmt: (i, t) => `上报间隔：${i} 秒 | 超时：${t} 秒`,
                disc_title: "Tracker 免责说明",
                disc_p1: "本站 Tracker 仅提供连接协调、状态记录与统计展示，不存储、不托管、不分发任何实际资源内容。",
                disc_p2: "页面中的 torrents、peers、seeders、leechers、客户端类型及趋势数据，来源于客户端上报与系统采样，可能存在延迟、缺失、偏差或伪造，不代表资源真实状态。",
                disc_p3: "本页面信息不代表任何资源的真实性、完整性、可用性、安全性或合法性，也不构成任何服务承诺或结果保证。",
                disc_p4: "对于第三方客户端行为、资源内容、传输结果及由此产生的任何直接或间接后果，本站不承担责任，使用者应自行判断并承担相关风险。",
                disc_p5: "受 Tracker 工作机制限制，本站不保留可用于长期识别、追踪或还原单个连接历史的完整日志，也无法对既往连接行为提供持续、完整或可验证的回溯记录。",
                blog_label: "Blog", contact_label: "如有问题请联系",
            },
            en: {
                monitoring: "Monitoring", overview: "Tracker Overview", running: "Running",
                error: "Error", paused_state: "Paused", disc_link: "Disclaimer",
                view: "View", side_note: "HTTP tracker connection, seeding, downloading and completion statistics.",
                title: "Tracker Console", subtitle: "View peer, seeding and download status on the current port.",
                loading: "Loading...", last_update: "Last updated", read_error: "Read failed",
                chart_title: "Tracker Trends",
                chart_note: "Torrents, Peers, Seeders and Leechers over time",
                range_24h: "24h", range_3d: "3D", range_7d: "7D",
                config_fmt: (i, t) => `Interval: ${i}s | Timeout: ${t}s`,
                disc_title: "Disclaimer",
                disc_p1: "This tracker only provides connection coordination, status recording and statistical display. It does not store, host or distribute any actual resource content.",
                disc_p2: "Torrents, peers, seeders, leechers, client types and trend data displayed on this page are derived from client reports and system sampling. They may contain delays, omissions, deviations or falsification, and do not represent the true state of resources.",
                disc_p3: "The information on this page does not represent the authenticity, completeness, availability, security or legality of any resource, nor does it constitute any service commitment or result guarantee.",
                disc_p4: "This site assumes no responsibility for third-party client behavior, resource content, transmission results, or any direct or indirect consequences arising therefrom. Users should exercise their own judgment and bear associated risks.",
                disc_p5: "Due to tracker operational limitations, this site does not retain complete logs that could be used for long-term identification, tracking or reconstruction of individual connection histories, and cannot provide continuous, complete or verifiable retrospective records of past connection activity.",
                blog_label: "Blog", contact_label: "Contact",
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
            if (state.data) render();
        }

        function number(value) {
            return new Intl.NumberFormat(state.lang === "zh" ? "zh-CN" : "en-US").format(value || 0);
        }

        function setStatus(text, error = false) {
            $("statusText").textContent = text;
            $("statusText").className = error ? "status-line error" : "status-line";
            $("navState").textContent = error ? t("error") : t("running");
        }

        async function loadStats() {
            try {
                const response = await fetch("/api/stats", { cache: "no-store" });
                if (!response.ok) throw new Error(`HTTP ${response.status}`);
                state.data = await response.json();
                render();
                setStatus(`${t("last_update")} ${new Date().toLocaleTimeString(state.lang === "zh" ? "zh-CN" : "en-US")}`);
            } catch (error) {
                setStatus(`${t("read_error")}: ${error.message}`, true);
            }
        }

        function render() {
            const data = state.data || {};
            $("metricPeers").textContent = number(data.peers);
            $("metricSeeders").textContent = number(data.seeders);
            $("metricLeechers").textContent = number(data.leechers);
            $("metricCompleted").textContent = number(data.completed);
            $("configText").textContent = tf("config_fmt", data.interval || "-", data.peer_timeout || "-");
            renderChart();
        }

        function filterHistory() {
            const history = state.data?.history || [];
            if (!history.length) return history;
            const ranges = { "24h": 86400, "3d": 259200, "7d": 604800 };
            const secs = ranges[state.range] || 86400;
            const cutoff = Math.floor(Date.now() / 1000) - secs;
            return history.filter((item) => item.timestamp >= cutoff);
        }

        function renderChart() {
            const history = filterHistory();
            const labels = history.map((item) => new Date(item.timestamp * 1000).toLocaleString("zh-CN", {
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
                    top: 0,
                    right: 0,
                    data: ["Torrents", "Peers", "Seeders", "Leechers"]
                },
                grid: { left: 44, right: 20, top: 52, bottom: 36, containLabel: true },
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

        $("rangeGroup").addEventListener("click", (e) => {
            const btn = e.target.closest(".range-btn");
            if (!btn || btn.classList.contains("active")) return;
            $("rangeGroup").querySelectorAll(".range-btn").forEach((b) => b.classList.remove("active"));
            btn.classList.add("active");
            state.range = btn.dataset.range;
            renderChart();
        });

        $("langSelect").addEventListener("change", (e) => setLang(e.target.value));

        $("langSelect").value = state.lang;
        setLang(state.lang);
        loadStats();
        window.addEventListener("resize", () => chart.resize());
        setInterval(() => { loadStats(); }, 5000);