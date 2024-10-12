use std::{convert::Infallible, path::PathBuf};

use bb8::Pool;
use http::StatusCode;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Bytes, Request, Response};
use tracing::instrument;

use crate::manager::{self, Manager};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to get connection: {0}")]
    Pool(#[from] bb8::RunError<manager::Error>),
    #[error("fastcgi error: {0}")]
    FastCgi(#[from] fastcgi_client::ClientError),
    #[error("failed to parse headers: {0}")]
    Headers(#[from] httparse::Error),
    #[error("invalid header name: {0}")]
    HeaderName(#[from] http::header::InvalidHeaderName),
    #[error("invalid header value: {0}")]
    HeaderValue(#[from] http::header::InvalidHeaderValue),
}

#[derive(Clone)]
pub struct PhpClient {
    root: PathBuf,
    pool: Pool<Manager>,
}

impl PhpClient {
    pub fn new(pool: Pool<Manager>, root: PathBuf) -> Self {
        Self { pool, root }
    }

    #[instrument(skip(self, request), name = "handle_request")]
    pub async fn handle(
        &self,
        request: Request<hyper::body::Incoming>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Infallible> {
        let result = async move {
            let mut conn = self.pool.get().await?;
            let (parts, body) = request.into_parts();
            let request = crate::request::translate(&self.root, &parts, body).await;
            let response = conn.send(request).await?;

            crate::response::translate(response)
        };

        match result.await {
            Ok(response) => Ok(response),
            Err(e) => {
                tracing::error!({ error = ?e }, "failed to handle request");

                let mut response = Response::new(
                    Full::new(Bytes::from("internal server error"))
                        .map_err(|_| unreachable!())
                        .boxed(),
                );
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;

                Ok(response)
            }
        }
    }
}
