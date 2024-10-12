use std::{error::Error, net::SocketAddr, path::PathBuf};

use clap::Parser;
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

#[derive(Parser)]
struct Opts {
    #[clap(long, default_value = "127.0.0.1:9000")]
    bind: SocketAddr,

    #[clap(long)]
    ping_path: Option<String>,

    #[clap(long, default_value = "127.0.0.1:3000")]
    listen: SocketAddr,

    #[clap(long, default_value = "./example")]
    root_dir: PathBuf,

    #[clap(long, default_value = "5")]
    max_conn: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opts = Opts::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let mut manager = Manager::new(opts.bind);

    if let Some(path) = opts.ping_path {
        manager = manager.with_ping(path);
    }

    let pool = bb8::Builder::new()
        .max_size(opts.max_conn)
        .build(manager)
        .await?;

    let client = PhpClient::new(pool, opts.root_dir);
    let listener = TcpListener::bind(opts.listen).await?;

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
