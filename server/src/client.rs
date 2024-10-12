use std::{convert::Infallible, path::PathBuf};

use bb8::Pool;
use http::StatusCode;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Bytes, Request, Response};
use tracing::instrument;

use crate::manager::{self, Manager};

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
    pool: Pool<Manager>,
}

impl PhpClient {
    pub fn new(root: PathBuf, pool: Pool<Manager>) -> Self {
        Self { root, pool }
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

            Ok::<_, Error>(crate::response::translate(response))
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
