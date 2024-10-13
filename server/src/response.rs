use std::str::FromStr;

use http_body_util::{combinators::BoxBody, BodyExt, Full};
use httparse::{Header, Status};
use hyper::{
    body::Bytes,
    header::{HeaderName, HeaderValue},
    Response, StatusCode,
};

use crate::service;

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, service::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

pub fn parse_status(header: &Header<'_>) -> Option<StatusCode> {
    if let Some(status) = header.value.iter().position(|c| *c == b' ') {
        if let Ok(status) = StatusCode::from_bytes(&header.value[..status]) {
            return Some(status);
        }
    }

    None
}

pub fn translate(
    input: fastcgi_client::Response,
) -> Result<Response<BoxBody<Bytes, service::Error>>, crate::service::Error> {
    let mut response = Response::new(BoxBody::default());
    let mut stdout = input.stdout.unwrap_or_default();
    let mut headers = [httparse::EMPTY_HEADER; 64];

    if let Status::Complete((offset, headers)) = httparse::parse_headers(&stdout, &mut headers)? {
        for header in headers {
            match header.name {
                "Status" => {
                    if let Some(status) = parse_status(&header) {
                        *response.status_mut() = status;
                    }
                }
                _ => {
                    response.headers_mut().append(
                        HeaderName::from_str(header.name)?,
                        HeaderValue::from_bytes(header.value)?,
                    );
                }
            }
        }

        *response.body_mut() = full(stdout.drain(offset..).collect::<Bytes>());
    }

    Ok(response)
}
