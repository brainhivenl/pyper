use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use client::PhpClient;
use hyper::{server::conn::http1::Builder, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use manager::Manager;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

mod client;
mod manager;
mod request;
mod response;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let addr = SocketAddr::from((IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000));
    let manager = Manager::new(addr);
    let pool = bb8::Builder::new().max_size(5).build(manager).await?;

    let client = PhpClient::new("./example".into(), pool);

    let addr: SocketAddr = ([127, 0, 0, 1], 3000).into();
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (tcp, _) = listener.accept().await?;
        let io = TokioIo::new(tcp);
        let client = client.clone();

        tracing::debug!("incoming connection");

        tokio::task::spawn(async move {
            if let Err(e) = Builder::new()
                .timer(TokioTimer::new())
                .serve_connection(io, service_fn(|r| client.handle(r)))
                .await
            {
                tracing::error!({ error = ?e }, "failed to serve connection");
            }

            tracing::debug!("finished connection");
        });
    }
}
