//! Bounded HTTP transport for GPUI image resources.

use std::{io::Read, time::Duration};

use anyhow::{bail, Result};
use futures::{future::BoxFuture, AsyncReadExt};
use gpui::http_client::{http, AsyncBody, HttpClient, RedirectPolicy};

const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_REDIRECTS: u32 = 5;

pub struct NativeHttpClient {
    agent: ureq::Agent,
    user_agent: http::HeaderValue,
}

impl NativeHttpClient {
    pub fn new() -> Self {
        let settings = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .max_redirects(MAX_REDIRECTS)
            .user_agent("ThoughtTree/0.5 (GPUI)")
            .build();
        Self {
            agent: ureq::Agent::new_with_config(settings),
            user_agent: http::HeaderValue::from_static("ThoughtTree/0.5 (GPUI)"),
        }
    }
}

impl HttpClient for NativeHttpClient {
    fn user_agent(&self) -> Option<&http::HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&gpui::http_client::Url> {
        None
    }

    fn send(
        &self,
        request: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<http::Response<AsyncBody>>> {
        let agent = self.agent.clone();
        Box::pin(async move {
            if !matches!(request.uri().scheme_str(), Some("http" | "https")) {
                bail!("Only HTTP and HTTPS image requests are supported.");
            }
            let redirects = match request.extensions().get::<RedirectPolicy>() {
                Some(RedirectPolicy::FollowAll) => MAX_REDIRECTS,
                Some(RedirectPolicy::FollowLimit(limit)) => (*limit).min(MAX_REDIRECTS),
                _ => 0,
            };
            let (parts, body) = request.into_parts();
            let mut bytes = Vec::new();
            body.take((MAX_BODY_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() > MAX_BODY_BYTES {
                bail!("The HTTP request exceeds the 16 MiB limit.");
            }
            smol::unblock(move || {
                let request = http::Request::from_parts(parts, bytes);
                let request = agent
                    .configure_request(request)
                    .max_redirects(redirects)
                    .build();
                let response = agent.run(request)?;
                let (parts, body) = response.into_parts();
                let bytes = read_bounded(body.into_reader(), MAX_BODY_BYTES)?;
                Ok(http::Response::from_parts(parts, bytes.into()))
            })
            .await
        })
    }
}

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        bail!("The HTTP response exceeds the {} byte limit.", limit);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn bounded_reader_detects_growth_without_consuming_the_whole_body() {
        let mut reader = std::io::Cursor::new(b"0123456789");
        assert!(read_bounded(&mut reader, 4).is_err());
        assert_eq!(reader.position(), 5);
        assert_eq!(
            read_bounded(std::io::Cursor::new(b"0123"), 4).unwrap(),
            b"0123"
        );
    }

    #[test]
    fn local_file_scheme_never_reaches_the_network_client() {
        let request = http::Request::builder()
            .uri("file://localhost/etc/passwd")
            .body(AsyncBody::empty())
            .unwrap();
        let result = smol::block_on(NativeHttpClient::new().send(request));
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("Only HTTP and HTTPS"));
    }

    #[test]
    fn http_preserves_error_status_and_body_for_the_image_loader() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            assert!(request_line.starts_with("GET /missing "));
            for header in reader.lines() {
                if header.unwrap().is_empty() {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope",
                )
                .unwrap();
        });
        let (status, body) = smol::block_on(async {
            let mut response = NativeHttpClient::new()
                .get(&format!("http://{address}/missing"), ().into(), false)
                .await
                .unwrap();
            let status = response.status();
            let mut body = String::new();
            response.body_mut().read_to_string(&mut body).await.unwrap();
            (status, body)
        });
        server.join().unwrap();
        assert_eq!(status, http::StatusCode::NOT_FOUND);
        assert_eq!(body, "nope");
    }
}
