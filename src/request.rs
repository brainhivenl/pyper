use std::path::Path;

use fastcgi_client::Params;
use futures::TryStreamExt;
use http::request::Parts;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use tokio::io::AsyncRead;
use tokio_util::compat::FuturesAsyncReadCompatExt;

fn try_get_header<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts.headers.get(name).and_then(|v| v.to_str().ok())
}

fn join(root: impl AsRef<Path>, path: &str) -> String {
    root.as_ref()
        .join(path)
        .into_os_string()
        .into_string()
        .unwrap_or_default()
}

fn path_to_str(path: &Path) -> &str {
    path.as_os_str().to_str().unwrap_or_default()
}

pub async fn translate<'a>(
    root: &'a Path,
    parts: &'a Parts,
    body: Incoming,
) -> fastcgi_client::Request<'a, impl AsyncRead + Unpin> {
    let file_name = "index.php";
    let mut params = Params::default()
        .document_root(path_to_str(root))
        .request_method(parts.method.as_str())
        .script_name(file_name)
        .script_filename(join(root, &file_name));

    if let Some(header) = try_get_header(&parts, "host") {
        params = params.server_name(header);
    }

    if let Some(header) = try_get_header(&parts, "content-type") {
        params = params.content_type(header);
    }

    if let Some(header) = try_get_header(&parts, "content-length") {
        params.insert("CONTENT_LENGTH".into(), header.into());
    }

    if let Some(path_query) = parts.uri.path_and_query() {
        if let Some(query) = path_query.query() {
            params = params
                .request_uri(format!("{}?{query}", path_query.path()))
                .query_string(query);
        } else {
            params = params.request_uri(path_query.path());
        }
    }

    let stream = body.into_data_stream();
    let read = TryStreamExt::map_err(stream, std::io::Error::other).into_async_read();

    fastcgi_client::Request::new(params, read.compat())
}
