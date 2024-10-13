use std::{convert::Infallible, future::Future, path::PathBuf, pin::Pin};

use bb8::Pool;
use http::StatusCode;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{
    body::{Bytes, Incoming},
    service::Service,
    Request, Response,
};
use hyper_staticfile::Static;

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
    files: Static,
    pool: Pool<Manager>,
}

impl PhpService {
    pub fn new(pool: Pool<Manager>, root: PathBuf) -> Self {
        Self {
            pool,
            files: Static::new(&root),
            root,
        }
    }
}

impl Service<Request<Incoming>> for PhpService {
    type Response = Response<BoxBody<Bytes, Error>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let root = self.root.clone();
        let pool = self.pool.clone();
        let files = self.files.clone();
        let future = async move {
            let file = request::find_file(&root, request.uri().path());

            if file.extension() != Some("php".as_ref()) {
                let response = files.serve(request).await?;
                return Ok(response.map(|body| body.map_err(Into::into).boxed()));
            }

            // Make sure the connection is not dropped when the future is dropped
            let handle = tokio::spawn(async move {
                let (parts, body) = request.into_parts();
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
