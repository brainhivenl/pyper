use std::str::FromStr;

use http_body_util::{combinators::BoxBody, BodyExt, Full};
use httparse::Status;
use hyper::{
    body::Bytes,
    header::{HeaderName, HeaderValue},
    Response, StatusCode,
};

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

pub fn translate(input: fastcgi_client::Response) -> Response<BoxBody<Bytes, hyper::Error>> {
    let mut response = Response::new(BoxBody::default());
    let stdout = input.stdout.unwrap_or_default();

    let mut headers = [httparse::EMPTY_HEADER; 64];
    let status = httparse::parse_headers(&stdout, &mut headers).expect("failed to parse headers");

    if let Status::Complete((offset, headers)) = status {
        for header in headers {
            if header.name == "Status" {
                if let Some(status) = header.value.iter().position(|c| *c == b' ') {
                    let status = StatusCode::from_bytes(&header.value[..status]).unwrap();
                    *response.status_mut() = status;
                }

                continue;
            }

            response.headers_mut().insert(
                HeaderName::from_str(&header.name).unwrap(),
                HeaderValue::from_bytes(header.value).unwrap(),
            );
        }

        *response.body_mut() = full(Bytes::from(stdout[offset..].to_vec()));
    }

    response
}
