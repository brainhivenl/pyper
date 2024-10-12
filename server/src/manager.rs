use std::{
    io::ErrorKind,
    net::SocketAddr,
    sync::atomic::{AtomicBool, Ordering},
};

use bb8::ManageConnection;
use fastcgi_client::{conn::KeepAlive, Client, ClientError, ClientResult, Request, Response};
use tokio::{io::AsyncRead, net::TcpStream};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("connection closed")]
    Closed,
}

#[derive(Clone)]
pub struct Manager {
    addr: SocketAddr,
}

impl Manager {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    async fn _connect(&self) -> Result<Conn, Error> {
        let stream = TcpStream::connect(&self.addr).await?;
        let client = Client::new_keep_alive(stream);

        Ok(Conn::new(client))
    }
}

pub struct Conn {
    client: Client<TcpStream, KeepAlive>,
    closed: AtomicBool,
}

impl Conn {
    pub fn new(client: Client<TcpStream, KeepAlive>) -> Self {
        Self {
            client,
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

#[async_trait::async_trait]
impl ManageConnection for Manager {
    type Connection = Conn;
    type Error = Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(self._connect().await?)
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        if !conn.closed.load(Ordering::Relaxed) {
            return Ok(());
        }

        Err(Error::Closed)
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.closed.load(Ordering::Relaxed)
    }
}
