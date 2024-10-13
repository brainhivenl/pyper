use std::{convert::Infallible, future::Future, path::PathBuf, pin::Pin};

use bb8::Pool;
use futures::TryStreamExt;
use http::StatusCode;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Bytes, Frame, Incoming},
    service::Service,
    Request, Response,
};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{
    manager::{self, Manager},
    request, service,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to get connection: {0}")]
    Pool(#[from] bb8::RunError<manager::Error>),
    #[error("fastcgi error: {0}")]
    FastCgi(#[from] fastcgi_client::ClientError),
    #[error("failed to join task: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("failed to parse headers: {0}")]
    Headers(#[from] httparse::Error),
    #[error("failed to parse header name: {0}")]
    HeaderName(#[from] http::header::InvalidHeaderName),
    #[error("failed to parse header value: {0}")]
    HeaderValue(#[from] http::header::InvalidHeaderValue),
}

async fn serve_file(
    path: PathBuf,
) -> Result<Response<BoxBody<Bytes, Error>>, crate::service::Error> {
    let file = File::open(path).await?;
    let stream = ReaderStream::new(file);
    let stream_body = StreamBody::new(stream.map_ok(Frame::data).map_err(|e| e.into()));
    let body = stream_body.boxed();

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;

    Ok(response)
}

fn handle_result(
    result: Result<Response<BoxBody<Bytes, Error>>, Error>,
) -> Result<Response<BoxBody<Bytes, Error>>, Infallible> {
    match result {
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

#[derive(Clone)]
pub struct PhpService {
    root: PathBuf,
    pool: Pool<Manager>,
}

impl PhpService {
    pub fn new(pool: Pool<Manager>, root: PathBuf) -> Self {
        Self { pool, root }
    }
}

impl Service<Request<Incoming>> for PhpService {
    type Response = Response<BoxBody<Bytes, Error>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let root = self.root.clone();
        let pool = self.pool.clone();
        let future = async move {
            let (parts, body) = request.into_parts();
            let file = request::find_file(&root, parts.uri.path());

            if file.extension() != Some("php".as_ref()) {
                return serve_file(file).await;
            }

            // Make sure the connection is not dropped when the future is dropped
            let handle = tokio::spawn(async move {
                let mut conn = pool.get().await?;

                tracing::debug!({ ?file, path = parts.uri.path() }, "calling script for request");

                let request = request::translate(&root, &file, &parts, body).await;
                Ok::<_, service::Error>(conn.send(request).await?)
            });

            let response = handle.await??;
            crate::response::translate(response)
        };

        Box::pin(async move { handle_result(future.await) })
    }
}
