use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use rustracker::server::{router, AppState, DEFAULT_TRACKER_SHARDS};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().collect();
    args.remove(0); // drop binary name

    // Mode A: --duration-secs N --concurrency M --torrents T
    // Mode B (legacy positional): total concurrency torrents
    let (duration_mode, total, concurrency, torrents) =
        if let Some(pos) = args.iter().position(|a| a == "--duration-secs") {
            let dur: u64 = args.get(pos + 1).and_then(|s| s.parse().ok()).unwrap_or(30);
            let conc = named_arg(&args, "--concurrency").unwrap_or(200).max(1);
            let torr = named_arg(&args, "--torrents").unwrap_or(100).max(1);
            (Some(Duration::from_secs(dur)), 0usize, conc, torr)
        } else {
            let total = args.first().and_then(|s| s.parse().ok()).unwrap_or(2_000);
            let conc = args
                .get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(200)
                .max(1);
            let torr = args
                .get(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(100)
                .max(1);
            (None, total, conc, torr)
        };

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind load-test server")?;
    let addr = listener.local_addr().context("failed to read local addr")?;
    let app = router(AppState::sharded(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        DEFAULT_TRACKER_SHARDS,
    ));

    let server = tokio::spawn(async move { axum::serve(listener, app.into_make_service()).await });
    wait_until_ready(addr).await?;

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(concurrency)
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")?;

    let ok = Arc::new(AtomicUsize::new(0));
    let non_200 = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let started = Instant::now();

    if let Some(dur_limit) = duration_mode {
        // Duration-based mode: keep spawning requests until time runs out.
        let stop = Arc::new(AtomicBool::new(false));
        let ok2 = ok.clone();
        let non2 = non_200.clone();
        let err2 = errors.clone();
        let sem = semaphore.clone();
        let client2 = client.clone();

        let producer_handle = {
            let stop = stop.clone();
            tokio::spawn(async move {
                let mut index: usize = 0;
                while !stop.load(Ordering::Relaxed) {
                    let permit = sem.clone().acquire_owned().await?;
                    if stop.load(Ordering::Relaxed) {
                        drop(permit);
                        break;
                    }
                    let client = client2.clone();
                    let ok = ok2.clone();
                    let non_200 = non2.clone();
                    let errors = err2.clone();
                    let url = announce_url(addr, index, torrents);
                    index += 1;

                    tokio::spawn(async move {
                        let _permit = permit;
                        match client.get(url).send().await {
                            Ok(r) if r.status().is_success() => {
                                ok.fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(_) => {
                                non_200.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    });
                }
                anyhow::Ok(())
            })
        };

        tokio::time::sleep(dur_limit).await;
        stop.store(true, Ordering::Relaxed);
        // Give in-flight tasks a moment to finish.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = producer_handle.await;
    } else {
        // Count-based mode: fire exactly `total` requests.
        let mut tasks = Vec::with_capacity(total);
        for index in 0..total {
            let permit = semaphore.clone().acquire_owned().await?;
            let client = client.clone();
            let ok = ok.clone();
            let non_200 = non_200.clone();
            let errors = errors.clone();
            let url = announce_url(addr, index, torrents);

            tasks.push(tokio::spawn(async move {
                let _permit = permit;
                match client.get(url).send().await {
                    Ok(r) if r.status().is_success() => {
                        ok.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(_) => {
                        non_200.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }
        for task in tasks {
            task.await?;
        }
    }

    let elapsed = started.elapsed();
    let seconds = elapsed.as_secs_f64().max(0.001);
    let ok_val = ok.load(Ordering::Relaxed);
    let non_val = non_200.load(Ordering::Relaxed);
    let err_val = errors.load(Ordering::Relaxed);
    let completed = ok_val + non_val + err_val;

    println!(
        "mode={}",
        if duration_mode.is_some() {
            "duration"
        } else {
            "count"
        }
    );
    println!("completed={completed}");
    println!("concurrency={concurrency}");
    println!("torrents={torrents}");
    println!("ok={ok_val}");
    println!("non_200={non_val}");
    println!("errors={err_val}");
    println!("seconds={seconds:.3}");
    println!("rps={:.2}", completed as f64 / seconds);

    let stats: serde_json::Value = client
        .get(format!("http://{addr}/api/stats"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!(
        "stats_peers={}",
        stats["peers"].as_u64().unwrap_or_default()
    );
    println!(
        "stats_torrents={}",
        stats["torrents"].as_u64().unwrap_or_default()
    );

    server.abort();
    Ok(())
}

fn named_arg(args: &[String], name: &str) -> Option<usize> {
    let pos = args.iter().position(|a| a == name)?;
    args.get(pos + 1)?.parse().ok()
}

fn announce_url(addr: SocketAddr, index: usize, torrents: usize) -> String {
    let info_hash = fixed_len_id(b'h', index % torrents);
    let peer_id = fixed_len_id(b'p', index);
    let port = 10_000 + (index % 50_000);

    format!(
        "http://{addr}/announce?info_hash={info_hash}&peer_id={peer_id}&port={port}&left=0&event=started&compact=1"
    )
}

fn fixed_len_id(prefix: u8, value: usize) -> String {
    let mut id = [prefix; 20];
    let encoded = format!("{value:018}");
    id[2..].copy_from_slice(encoded.as_bytes());
    String::from_utf8_lossy(&id).into_owned()
}

async fn wait_until_ready(addr: SocketAddr) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(5);

    while Instant::now() < deadline {
        if let Ok(response) = client.get(format!("http://{addr}/healthz")).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    anyhow::bail!("load-test server did not become ready")
}
