use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

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

trait AsStr {
    fn as_str(&self) -> &str;
}

impl AsStr for OsStr {
    fn as_str(&self) -> &str {
        self.to_str().unwrap_or_default()
    }
}

impl AsStr for Path {
    fn as_str(&self) -> &str {
        self.as_os_str().as_str()
    }
}

pub fn find_file(root: &Path, uri_path: &str) -> PathBuf {
    let path = root.join(uri_path.trim_start_matches('/'));

    if path.is_file() && path.exists() {
        return path;
    }

    if path.is_dir() {
        let path = path.join("index.php");

        if path.exists() {
            return path;
        }
    }

    root.join("index.php")
}

pub async fn translate<'a>(
    root: &'a Path,
    script: &'a Path,
    parts: &'a Parts,
    body: Incoming,
) -> fastcgi_client::Request<'a, impl AsyncRead + Unpin> {
    let mut params = Params::default()
        .document_root(root.as_str())
        .request_method(parts.method.as_str())
        .script_name(script.file_name().unwrap_or_default().as_str())
        .script_filename(script.as_str());

    if let Some(header) = try_get_header(&parts, "host") {
        let (host, port) = header.split_once(':').unwrap_or((header, ""));

        params = params
            .server_name(host)
            .server_addr(header)
            .custom("HTTP_HOST", header);

        if !port.is_empty() {
            params = params.server_port(port);
        }
    }

    if let Some(header) = try_get_header(&parts, "content-type") {
        params = params.content_type(header);
    }

    if let Some(header) = try_get_header(&parts, "content-length") {
        params = params.content_length(header);
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

    for (name, value) in parts.headers.iter() {
        if matches!(name.as_str(), "host" | "content-type" | "content-length") {
            continue;
        }

        if let Some(value) = value.to_str().ok() {
            params = params.custom(format!("HTTP_{}", name.as_str().to_uppercase()), value);
        }
    }

    let stream = body.into_data_stream();
    let read = TryStreamExt::map_err(stream, std::io::Error::other).into_async_read();

    fastcgi_client::Request::new(params, read.compat())
}
