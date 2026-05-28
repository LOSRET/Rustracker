use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::pin;
use std::time::Duration;

use axum::body::Body;
use axum::extract::connect_info::IntoMakeServiceWithConnectInfo;
use axum::Router;
use hyper::body::Incoming;
use hyper::server::conn::http1::Builder;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower::{Service, ServiceExt};
use tracing::trace;

pub async fn serve<F>(
    listener: TcpListener,
    app: Router,
    keepalive_timeout: Duration,
    shutdown: F,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let mut make_service = app.into_make_service_with_connect_info::<SocketAddr>();
    let (signal_tx, signal_rx) = watch::channel(());
    tokio::spawn(async move {
        shutdown.await;
        trace!("received graceful shutdown signal. Telling tasks to shutdown");
        drop(signal_rx);
    });

    let (close_tx, close_rx) = watch::channel(());

    loop {
        let (stream, remote_addr) = tokio::select! {
            conn = listener.accept() => conn?,
            _ = signal_tx.closed() => {
                trace!("signal received, not accepting new connections");
                break;
            }
        };

        <IntoMakeServiceWithConnectInfo<Router, SocketAddr> as ServiceExt<SocketAddr>>::ready(
            &mut make_service,
        )
        .await
        .unwrap_or_else(|err| match err {});

        let io = TokioIo::new(stream);
        let tower_service = make_service
            .call(remote_addr)
            .await
            .unwrap_or_else(|err| match err {})
            .map_request(|req: axum::extract::Request<Incoming>| req.map(Body::new));
        let hyper_service = TowerToHyperService::new(tower_service);
        let signal_tx = signal_tx.clone();
        let close_rx = close_rx.clone();

        tokio::spawn(async move {
            let mut builder = Builder::new();
            builder
                .keep_alive(true)
                .timer(TokioTimer::new())
                .header_read_timeout(keepalive_timeout);

            let conn = builder.serve_connection(io, hyper_service);
            let mut conn = pin!(conn);
            let mut shutdown_started = false;

            loop {
                tokio::select! {
                    result = conn.as_mut() => {
                        if let Err(err) = result {
                            trace!("failed to serve connection: {err:#}");
                        }
                        break;
                    }
                    _ = signal_tx.closed(), if !shutdown_started => {
                        shutdown_started = true;
                        trace!("signal received in task, starting graceful shutdown");
                        conn.as_mut().graceful_shutdown();
                    }
                }
            }

            drop(close_rx);
        });
    }

    drop(close_rx);
    drop(listener);

    trace!(
        "waiting for {} task(s) to finish",
        close_tx.receiver_count()
    );
    close_tx.closed().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::future;

    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    async fn slow() -> &'static str {
        tokio::time::sleep(Duration::from_millis(300)).await;
        "slow"
    }

    async fn start_server(app: Router, keepalive_timeout: Duration) -> SocketAddr {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            serve(listener, app, keepalive_timeout, future::pending())
                .await
                .unwrap();
        });
        addr
    }

    async fn send_request(stream: &mut TcpStream, path: &str) {
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
    }

    async fn read_response(stream: &mut TcpStream, expected_body: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 128];

        loop {
            let read = stream.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0, "connection closed before full response");
            buf.extend_from_slice(&chunk[..read]);
            if buf.windows(expected_body.len()).any(|w| w == expected_body) {
                return buf;
            }
        }
    }

    #[tokio::test]
    async fn keepalive_connection_accepts_second_request_before_timeout() {
        let app = Router::new().route("/healthz", get(|| async { "ok" }));
        let addr = start_server(app, Duration::from_millis(500)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        send_request(&mut stream, "/healthz").await;
        let first = read_response(&mut stream, b"ok").await;
        assert!(first.windows(b"200 OK".len()).any(|w| w == b"200 OK"));

        send_request(&mut stream, "/healthz").await;
        let second = read_response(&mut stream, b"ok").await;
        assert!(second.windows(b"200 OK".len()).any(|w| w == b"200 OK"));
    }

    #[tokio::test]
    async fn keepalive_connection_closes_after_idle_timeout() {
        let app = Router::new().route("/healthz", get(|| async { "ok" }));
        let addr = start_server(app, Duration::from_millis(100)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        send_request(&mut stream, "/healthz").await;
        let response = read_response(&mut stream, b"ok").await;
        assert!(response.windows(b"200 OK".len()).any(|w| w == b"200 OK"));

        tokio::time::sleep(Duration::from_millis(300)).await;

        let mut buf = [0_u8; 1];
        let read = stream.read(&mut buf).await.unwrap();
        assert_eq!(read, 0);
    }

    #[tokio::test]
    async fn keepalive_timeout_does_not_abort_active_request() {
        let app = Router::new().route("/slow", get(slow));
        let addr = start_server(app, Duration::from_millis(100)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        send_request(&mut stream, "/slow").await;
        let response = read_response(&mut stream, b"slow").await;
        assert!(response.windows(b"200 OK".len()).any(|w| w == b"200 OK"));
    }
}
