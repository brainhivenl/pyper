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

pub fn parse_headers<const N: usize>(
    input: &[u8],
) -> Result<(usize, [Header<'_>; N]), httparse::Error> {
    let mut headers = [httparse::EMPTY_HEADER; N];

    match httparse::parse_headers(input, &mut headers)? {
        Status::Complete((offset, _)) => Ok((offset, headers)),
        Status::Partial => Ok((0, headers)),
    }
}

pub fn translate(
    input: fastcgi_client::Response,
) -> Result<Response<BoxBody<Bytes, service::Error>>, crate::service::Error> {
    let mut response = Response::new(BoxBody::default());
    let mut stdout = input.stdout.unwrap_or_default();
    let (offset, headers) = parse_headers::<64>(&stdout)?;

    for header in headers {
        match header.name {
            // Invalid header
            "" => continue,
            "Status" => {
                if let Some(status) = parse_status(&header) {
                    *response.status_mut() = status;
                }
            }
            _ => {
                response.headers_mut().insert(
                    HeaderName::from_str(header.name)?,
                    HeaderValue::from_bytes(header.value)?,
                );
            }
        }
    }

    if offset > 0 {
        *response.body_mut() = full(stdout.drain(offset..).collect::<Bytes>());
    }

    Ok(response)
}
