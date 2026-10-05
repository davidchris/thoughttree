//! Bounded HTTP transport for GPUI image resources.
//!
//! Answer text is untrusted, so image requests may only reach public
//! addresses. Every hop, including redirects and IP literals, resolves through
//! [`PublicResolver`].

use std::{
    io::{self, Read},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use anyhow::{bail, Result};
use futures::{future::BoxFuture, AsyncReadExt};
use gpui::http_client::{http, AsyncBody, HttpClient, RedirectPolicy};
use ureq::unversioned::{
    resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
    transport::{DefaultConnector, NextTimeout},
};

const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_REDIRECTS: u32 = 5;

pub struct NativeHttpClient {
    agent: ureq::Agent,
    user_agent: http::HeaderValue,
}

impl NativeHttpClient {
    pub fn new() -> Self {
        Self::with_resolver(PublicResolver::default())
    }

    fn with_resolver(resolver: PublicResolver) -> Self {
        let settings = ureq::Agent::config_builder()
            // A proxy would resolve the destination out of the resolver's view.
            .proxy(None)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .max_redirects(MAX_REDIRECTS)
            .user_agent("ThoughtTree/0.5 (GPUI)")
            .build();
        Self {
            agent: ureq::Agent::with_parts(settings, DefaultConnector::new(), resolver),
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

/// Resolves like the system resolver, then drops every non-public address.
#[derive(Debug, Default)]
struct PublicResolver {
    inner: DefaultResolver,
    #[cfg(test)]
    allow_loopback: bool,
}

impl PublicResolver {
    fn allows(&self, ip: IpAddr) -> bool {
        #[cfg(test)]
        if self.allow_loopback && ip.is_loopback() {
            return true;
        }
        is_public(ip)
    }
}

impl Resolver for PublicResolver {
    fn resolve(
        &self,
        uri: &http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let mut allowed = self.empty();
        for address in self.inner.resolve(uri, config, timeout)?.iter() {
            if self.allows(address.ip()) {
                allowed.push(*address);
            }
        }
        if allowed.is_empty() {
            return Err(ureq::Error::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Images from local or private network addresses are blocked.",
            )));
        }
        Ok(allowed)
    }
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => match ip.to_ipv4_mapped().or_else(|| embedded_v4(ip)) {
            Some(ip) => is_public_v4(ip),
            None => is_public_v6(ip),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || a >= 240
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..20).contains(&b)))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let [first, second, ..] = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || (first == 0x2001 && second == 0x0db8))
}

// NAT64 (64:ff9b::/96) and 6to4 (2002::/16) addresses reach an IPv4 host.
fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let segments = ip.segments();
    let octets = ip.octets();
    match segments {
        [0x64, 0xff9b, 0, 0, 0, 0, ..] => Some(Ipv4Addr::new(
            octets[12], octets[13], octets[14], octets[15],
        )),
        [0x2002, ..] => Some(Ipv4Addr::new(octets[2], octets[3], octets[4], octets[5])),
        _ => None,
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
impl NativeHttpClient {
    /// Tests serve fixtures from 127.0.0.1; every other private range stays
    /// blocked.
    pub(crate) fn for_loopback_fixture() -> Self {
        Self::with_resolver(PublicResolver {
            allow_loopback: true,
            ..Default::default()
        })
    }
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
    fn only_public_addresses_are_reachable() {
        for blocked in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:c0a8:0101::",
        ] {
            assert!(!is_public(blocked.parse().unwrap()), "{blocked}");
        }
        for public in ["93.184.215.14", "2606:4700::6810:85e5"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
    }

    #[test]
    fn loopback_image_requests_never_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let error = smol::block_on(NativeHttpClient::new().get(
            &format!("http://{address}/pixel.png"),
            ().into(),
            true,
        ))
        .err()
        .unwrap();
        assert!(
            format!("{error:#}").contains("private network"),
            "{error:#}"
        );
        assert!(listener.accept().is_err());
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
            let mut response = NativeHttpClient::for_loopback_fixture()
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
