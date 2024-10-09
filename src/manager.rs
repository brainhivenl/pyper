use std::net::SocketAddr;

use bb8::ManageConnection;
use fastcgi_client::{conn::KeepAlive, Client, Params, Request};
use tokio::net::TcpStream;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("fastcgi not responding: {0}")]
    Ping(#[from] fastcgi_client::ClientError),
}

#[derive(Clone)]
pub struct ConnManager {
    addr: SocketAddr,
}

impl ConnManager {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    async fn _connect(&self) -> Result<Client<TcpStream, KeepAlive>, Error> {
        let stream = TcpStream::connect(&self.addr).await?;
        Ok(Client::new_keep_alive(stream))
    }
}

#[async_trait::async_trait]
impl ManageConnection for ConnManager {
    type Connection = Client<TcpStream, KeepAlive>;
    type Error = Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(self._connect().await?)
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        conn.execute(Request::new(
            Params::default()
                .request_method("GET")
                .server_name("localhost")
                .server_port(8000),
            &mut tokio::io::empty(),
        ))
        .await?;

        Ok(())
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}
