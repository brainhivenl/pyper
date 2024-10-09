use std::{convert::Infallible, path::PathBuf};

use bb8::Pool;
use http_body_util::Full;
use hyper::{body::Bytes, Request, Response, StatusCode};

use crate::manager::{self, ConnManager};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("connection error: {0}")]
    Pool(#[from] bb8::RunError<manager::Error>),
    #[error("fastcgi error: {0}")]
    FastCgi(#[from] fastcgi_client::ClientError),
}

#[derive(Clone)]
pub struct PhpClient {
    root: PathBuf,
    pool: Pool<ConnManager>,
}

impl PhpClient {
    pub fn new(root: PathBuf, pool: Pool<ConnManager>) -> Self {
        Self { root, pool }
    }

    pub async fn handle(
        &self,
        request: Request<impl hyper::body::Body>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let result = async move {
            let mut client = self.pool.get().await?;
            let request = crate::request::translate(&self.root, &request);
            let response = client.execute(request).await?;

            Ok::<_, Error>(crate::response::translate(response))
        };

        match result.await {
            Ok(response) => Ok(response),
            Err(e) => {
                tracing::error!({ error = ?e }, "failed to handle request");

                let mut response = Response::new(Full::new(Bytes::from("internal server error")));
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;

                Ok(response)
            }
        }
    }
}
