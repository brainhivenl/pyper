use std::{
    io::ErrorKind,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use bb8::ManageConnection;
use fastcgi_client::{
    conn::KeepAlive, Client, ClientError, ClientResult, Params, Request, Response,
};
use tokio::{io::AsyncRead, net::TcpStream};

use crate::response::parse_headers;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("not responding: {0}")]
    Client(#[from] fastcgi_client::ClientError),
    #[error("failed to parse ping response: {0}")]
    Headers(#[from] httparse::Error),
    #[error("ping failed")]
    Ping,
    #[error("connection closed")]
    Closed,
}

#[derive(Clone)]
pub struct Manager {
    addr: SocketAddr,
    ping_path: Option<Arc<String>>,
}

impl Manager {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            ping_path: None,
        }
    }

    pub fn with_ping(mut self, path: String) -> Self {
        self.ping_path = Some(Arc::new(path));
        self
    }

    async fn _connect(&self) -> Result<Conn, Error> {
        let stream = TcpStream::connect(&self.addr).await?;
        let client = Client::new_keep_alive(stream);

        Ok(Conn::new(
            client,
            self.ping_path.as_ref().map(|path| Arc::clone(&path)),
        ))
    }
}

pub struct Conn {
    client: Client<TcpStream, KeepAlive>,
    ping_path: Option<Arc<String>>,
    closed: AtomicBool,
}

impl Conn {
    pub fn new(client: Client<TcpStream, KeepAlive>, ping_path: Option<Arc<String>>) -> Self {
        Self {
            client,
            ping_path,
            closed: AtomicBool::new(false),
        }
    }

    pub async fn send<I: AsyncRead + Unpin>(
        &mut self,
        request: Request<'_, I>,
    ) -> ClientResult<Response> {
        match self.client.execute(request).await {
            Ok(response) => Ok(response),
            Err(e) => {
                if let ClientError::Io(e) = &e {
                    if matches!(e.kind(), ErrorKind::BrokenPipe | ErrorKind::UnexpectedEof) {
                        self.closed.store(true, Ordering::Relaxed);
                    }
                }

                Err(e)
            }
        }
    }
}

fn ping_params(path: &str) -> Params {
    Params::default()
        .request_method("GET")
        .server_name("localhost")
        .server_port(8000)
        .script_name(path)
        .script_filename(path)
}

#[async_trait::async_trait]
impl ManageConnection for Manager {
    type Connection = Conn;
    type Error = Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(self._connect().await?)
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        if conn.closed.load(Ordering::Relaxed) {
            return Err(Error::Closed);
        }

        if let Some(path) = conn.ping_path.clone() {
            let mut empty = tokio::io::empty();
            let request = Request::new(ping_params(&path), &mut empty);
            let response = conn.send(request).await?;
            let stdout = response.stdout.unwrap_or_default();
            let (offset, _) = parse_headers::<64>(&stdout)?;

            if &stdout[offset..] != b"pong" {
                tracing::error!("ping failed");
                return Err(Error::Ping);
            }
        }

        Ok(())
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.closed.load(Ordering::Relaxed)
    }
}
