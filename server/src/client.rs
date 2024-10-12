use std::{convert::Infallible, path::PathBuf};

use bb8::Pool;
use futures::TryStreamExt;
use http::StatusCode;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Bytes, Frame},
    Request, Response,
};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tracing::instrument;

use crate::{
    manager::{self, Manager},
    request,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
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

async fn serve_file(
    path: PathBuf,
) -> Result<Response<BoxBody<Bytes, Error>>, crate::client::Error> {
    let file = File::open(path).await?;
    let stream = ReaderStream::new(file);
    let stream_body = StreamBody::new(stream.map_ok(Frame::data).map_err(|e| e.into()));
    let body = stream_body.boxed();

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;

    Ok(response)
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
    ) -> Result<Response<BoxBody<Bytes, Error>>, Infallible> {
        let result = async move {
            let (parts, body) = request.into_parts();
            let file = request::find_file(&self.root, parts.uri.path());

            if file.extension() != Some("php".as_ref()) {
                return serve_file(file).await;
            }

            let mut conn = self.pool.get().await?;

            tracing::debug!({ ?file, path = parts.uri.path() }, "found script for request");

            let request = request::translate(&self.root, &file, &parts, body).await;
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
