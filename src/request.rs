use std::path::Path;

use fastcgi_client::Params;
use hyper::Request;
use tokio::io::Empty;

fn try_get_header<'a, T>(request: &'a Request<T>, name: &str) -> Option<&'a str> {
    request.headers().get(name).and_then(|v| v.to_str().ok())
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

pub fn translate<'a, T>(
    root: &'a Path,
    request: &'a Request<T>,
) -> fastcgi_client::Request<'a, Empty> {
    let mut params = Params::default()
        .document_root(path_to_str(root))
        .request_method(request.method().as_str())
        .script_name("index.php")
        .script_filename(join(root, "index.php"));

    if let Some(header) = try_get_header(request, "host") {
        params = params.server_name(header);
    }

    if let Some(header) = try_get_header(request, "content-type") {
        params = params.content_type(header);
    }

    fastcgi_client::Request::new(params, tokio::io::empty())
}
