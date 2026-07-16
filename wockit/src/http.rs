//! HTTP boundary: the transport seam plus the shared fetch-and-decode hardening.
//!
//! Port of Networking/HTTPClient.swift.
//!
//! Cache parity: macOS pins `URLRequest.CachePolicy.reloadIgnoringLocalCacheData` so
//! every poll hits the network (these are live, fast-changing feeds and neither API
//! documents conditional-request support). reqwest has no cache layer at all, so that
//! always-hit-the-network policy holds by construction here — no extra request headers
//! are needed or sent.
//!
//! Proxy parity: reqwest's `system-proxy` feature honors `HTTP_PROXY`/`HTTPS_PROXY`/
//! `NO_PROXY`, the Linux analogue of URLSession following the macOS system proxy
//! settings. No other environment is read.

use crate::config;
use crate::error::{FetchError, TransportErrorKind};

/// A completed HTTP exchange: status code plus the (capped) body bytes.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Body bytes. The live transport never buffers more than
    /// [`config::api::MAXIMUM_RESPONSE_BYTES`] + 1 bytes.
    pub body: Vec<u8>,
}

/// Minimal transport seam so the services can be unit-tested with a fake. The live
/// default is [`ReqwestTransport`]. Swift parity: `protocol HTTPClient`.
// Callers are generic over the transport (`impl HttpTransport`) rather than boxing trait
// objects, so plain async-fn-in-trait is sufficient; call sites that spawn can add Send
// bounds where needed.
#[allow(async_fn_in_trait)]
pub trait HttpTransport {
    /// Perform a GET and return the status + body, or a classified transport failure.
    async fn get(&self, url: &url::Url) -> Result<HttpResponse, TransportErrorKind>;
}

/// Perform the request, enforce HTTP 200 and the response cap, and decode — mapping
/// every failure onto a typed [`FetchError`]. Decode failures are logged in debug builds
/// only (Swift logs under `#if DEBUG`).
/// Swift parity: `HTTPClient.fetchDecoded(_:from:)`.
pub async fn fetch_decoded<T: serde::de::DeserializeOwned>(
    transport: &impl HttpTransport,
    url: &url::Url,
) -> Result<T, FetchError> {
    let response = transport.get(url).await.map_err(FetchError::Transport)?;

    // Strictly 200, not 2xx: the Swift guard is `http.statusCode == 200`.
    if response.status != 200 {
        return Err(FetchError::Http(response.status));
    }
    if response.body.len() > config::api::MAXIMUM_RESPONSE_BYTES {
        return Err(FetchError::ResponseTooLarge {
            bytes: response.body.len(),
            maximum: config::api::MAXIMUM_RESPONSE_BYTES,
        });
    }

    match serde_json::from_slice(&response.body) {
        Ok(value) => Ok(value),
        Err(_error) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[wockit] decode {} failed: {_error}",
                std::any::type_name::<T>()
            );
            Err(FetchError::Decode)
        }
    }
}

/// The live transport: a reqwest client with the spec timeout applied to every request.
/// GET only, no extra headers. Redirects follow only while the target stays on HTTPS and
/// the original origin. The scheme rule preserves macOS App Transport Security behavior;
/// the origin rule preserves the Linux port's stricter privacy contract that requests
/// never leave the three configured API hosts.
#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// A transport with the spec timeout ([`config::api::REQUEST_TIMEOUT`], 12 s).
    pub fn new() -> Self {
        Self::with_timeout(config::api::REQUEST_TIMEOUT)
    }

    /// A transport with a caller-chosen total request timeout (tests use a short one).
    pub fn with_timeout(timeout: std::time::Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                // Keep reqwest's default hop limit while refusing both cleartext and
                // cross-origin redirects. Services start from config-owned endpoints,
                // so same-origin redirects cannot introduce a fourth contacted host.
                if attempt.previous().len() > 10 {
                    attempt.error("too many redirects")
                } else if attempt
                    .previous()
                    .first()
                    .is_some_and(|initial| redirect_target_allowed(initial, attempt.url()))
                {
                    attempt.follow()
                } else {
                    attempt.error("refused redirect outside the original HTTPS origin")
                }
            }))
            .build()
            .expect("reqwest client construction only fails on invalid static TLS config");
        Self { client }
    }
}

fn redirect_target_allowed(initial: &url::Url, target: &url::Url) -> bool {
    target.scheme() == "https"
        && initial.scheme() == "https"
        && target.host_str() == initial.host_str()
        && target.port_or_known_default() == initial.port_or_known_default()
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport for ReqwestTransport {
    async fn get(&self, url: &url::Url) -> Result<HttpResponse, TransportErrorKind> {
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(classify_reqwest_error)?;
        let status = response.status().as_u16();
        let body = read_capped_body(response)
            .await
            .map_err(classify_reqwest_error)?;
        Ok(HttpResponse { status, body })
    }
}

/// Stream the body, accumulating at most `MAXIMUM_RESPONSE_BYTES + 1` bytes, then stop —
/// a hostile 100 MB response must never reach memory in full. `fetch_decoded` then
/// rejects anything over the cap.
///
/// Parity nuance: Swift measures the fully buffered body, so its `bytes` count is the
/// true response size; ours reports at most cap + 1.
async fn read_capped_body(mut response: reqwest::Response) -> Result<Vec<u8>, reqwest::Error> {
    let cap = config::api::MAXIMUM_RESPONSE_BYTES + 1;
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = cap - body.len();
        if chunk.len() >= remaining {
            body.extend_from_slice(&chunk[..remaining]);
            break;
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Map a reqwest error onto the closed [`TransportErrorKind`] set. What matters for spec
/// parity is the classification OUTCOME (which `StatusFailureKind` each failure reaches),
/// not a 1:1 URLError correspondence:
///
/// - timeouts map to `TimedOut` (outage-eligible, like `URLError.timedOut`);
/// - connect failures classify from the error chain via [`classify_connect_chain`];
/// - everything else is `Other` (classifies as `Unknown`, never outage-eligible).
fn classify_reqwest_error(error: reqwest::Error) -> TransportErrorKind {
    if error.is_timeout() {
        return TransportErrorKind::TimedOut;
    }
    if error.is_connect() {
        return classify_connect_chain(&error);
    }
    TransportErrorKind::Other
}

/// Classify a connect failure from its error chain. Only failures with POSITIVE evidence
/// classify as outage-eligible, mirroring the Swift switch whose `default:` arm makes
/// unrecognized transport failures `.unknown` (never outage-eligible):
///
/// - io `NetworkDown`/`NetworkUnreachable` — Linux's closest analogue of
///   `URLError.notConnectedToInternet` (there is no OS-level "no internet" signal);
/// - io refused/reset/host-unreachable — `CannotConnectToHost`;
/// - a "dns" message in the chain (hyper-util wraps getaddrinfo failures in a
///   `ConnectError` that displays as "dns error") — `DnsLookupFailed`, keeping genuine
///   DNS outages outage-eligible per spec;
/// - anything else — `Other`. In particular a TLS handshake/certificate failure (io
///   `InvalidData` under rustls, or no io error in the chain at all) lands here, exactly
///   where the macOS app's `URLError` default branch puts it.
fn classify_connect_chain(error: &(dyn std::error::Error + 'static)) -> TransportErrorKind {
    let mut source = error.source();
    while let Some(current) = source {
        if let Some(io_error) = current.downcast_ref::<std::io::Error>() {
            match io_error.kind() {
                std::io::ErrorKind::NetworkDown | std::io::ErrorKind::NetworkUnreachable => {
                    return TransportErrorKind::NotConnectedToInternet;
                }
                std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::HostUnreachable => {
                    return TransportErrorKind::CannotConnectToHost;
                }
                std::io::ErrorKind::InvalidData => return TransportErrorKind::Other,
                _ => break,
            }
        }
        source = current.source();
    }

    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if current.to_string().to_ascii_lowercase().contains("dns") {
            return TransportErrorKind::DnsLookupFailed;
        }
        source = current.source();
    }
    TransportErrorKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StatusFailureKind;
    use std::time::Duration;

    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct Probe {
        value: i64,
    }

    /// Mirror of the Swift test double `FakeHTTP { body, status }`.
    struct FakeHttp {
        body: Vec<u8>,
        status: u16,
    }

    impl HttpTransport for FakeHttp {
        async fn get(&self, _url: &url::Url) -> Result<HttpResponse, TransportErrorKind> {
            Ok(HttpResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }
    }

    struct FailingHttp {
        kind: TransportErrorKind,
    }

    impl HttpTransport for FailingHttp {
        async fn get(&self, _url: &url::Url) -> Result<HttpResponse, TransportErrorKind> {
            Err(self.kind)
        }
    }

    fn test_url() -> url::Url {
        url::Url::parse("https://example.invalid/").unwrap()
    }

    #[tokio::test]
    async fn decodes_valid_json_on_200() {
        let transport = FakeHttp {
            body: br#"{"value": 7}"#.to_vec(),
            status: 200,
        };
        let decoded: Probe = fetch_decoded(&transport, &test_url()).await.unwrap();
        assert_eq!(decoded, Probe { value: 7 });
    }

    // Swift parity: DecodingErrorTests.swift > HTTPClientBoundaryTests >
    // rejectsOversizedSuccessfulResponsesBeforeDecoding
    #[tokio::test]
    async fn rejects_oversized_successful_responses_before_decoding() {
        let maximum = config::api::MAXIMUM_RESPONSE_BYTES;
        let transport = FakeHttp {
            body: vec![0x20; maximum + 1],
            status: 200,
        };
        let result: Result<Probe, FetchError> = fetch_decoded(&transport, &test_url()).await;
        assert_eq!(
            result.unwrap_err(),
            FetchError::ResponseTooLarge {
                bytes: maximum + 1,
                maximum,
            }
        );
    }

    // The Swift guard is `data.count <= maximumResponseBytes`: the boundary itself must
    // still decode; only strictly larger bodies are rejected.
    #[tokio::test]
    async fn accepts_a_body_of_exactly_the_maximum_size() {
        let maximum = config::api::MAXIMUM_RESPONSE_BYTES;
        let mut body = vec![b'a'; maximum];
        body[0] = b'"';
        body[maximum - 1] = b'"';
        let transport = FakeHttp { body, status: 200 };
        let decoded: String = fetch_decoded(&transport, &test_url()).await.unwrap();
        assert_eq!(decoded.len(), maximum - 2);
    }

    // Swift parity: DecodingErrorTests.swift > CryptoServiceTests > mapsNon200ToError.
    // The Swift test goes through CryptoService; the behavior under test is fetchDecoded's
    // 200-only policy, which the Phase 3 services consume through this function.
    #[tokio::test]
    async fn maps_non_200_to_http_error_for_the_dex_feed_policy() {
        let transport = FakeHttp {
            body: b"{}".to_vec(),
            status: 503,
        };
        let result: Result<Probe, FetchError> = fetch_decoded(&transport, &test_url()).await;
        assert_eq!(result.unwrap_err(), FetchError::Http(503));
    }

    // Swift parity: DecodingErrorTests.swift > GeckoTerminalServiceTests > mapsNon200ToError.
    // Same 200-only policy, cited separately because both Swift services pin it.
    #[tokio::test]
    async fn maps_non_200_to_http_error_for_the_gecko_feed_policy() {
        let transport = FakeHttp {
            body: b"{}".to_vec(),
            status: 503,
        };
        let result: Result<Probe, FetchError> = fetch_decoded(&transport, &test_url()).await;
        assert_eq!(result.unwrap_err(), FetchError::Http(503));
    }

    #[tokio::test]
    async fn maps_404_to_http_error_that_is_not_outage_eligible() {
        let transport = FakeHttp {
            body: b"{}".to_vec(),
            status: 404,
        };
        let result: Result<Probe, FetchError> = fetch_decoded(&transport, &test_url()).await;
        let error = result.unwrap_err();
        assert_eq!(error, FetchError::Http(404));
        assert_eq!(
            error.status_failure_kind(),
            StatusFailureKind::InvalidResponse
        );
        assert!(!error
            .status_failure_kind()
            .counts_toward_outage_confirmation());
    }

    #[tokio::test]
    async fn maps_malformed_json_to_decode() {
        let transport = FakeHttp {
            body: b"not json".to_vec(),
            status: 200,
        };
        let result: Result<Probe, FetchError> = fetch_decoded(&transport, &test_url()).await;
        assert_eq!(result.unwrap_err(), FetchError::Decode);
    }

    #[tokio::test]
    async fn propagates_transport_timeout_as_outage_eligible() {
        let transport = FailingHttp {
            kind: TransportErrorKind::TimedOut,
        };
        let result: Result<Probe, FetchError> = fetch_decoded(&transport, &test_url()).await;
        let error = result.unwrap_err();
        assert_eq!(error, FetchError::Transport(TransportErrorKind::TimedOut));
        assert!(error
            .status_failure_kind()
            .counts_toward_outage_confirmation());
    }

    // Hermetic loopback-only check that the live transport enforces its timeout and
    // classifies it as TimedOut (the explicitly allowed loopback exception; no external
    // network is contacted).
    #[tokio::test]
    async fn reqwest_transport_classifies_a_stalled_connection_as_timed_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            // Accept, then hang without writing a byte so the client's timeout fires.
            if let Ok((stream, _)) = listener.accept() {
                std::thread::sleep(Duration::from_millis(500));
                drop(stream);
            }
        });

        let transport = ReqwestTransport::with_timeout(Duration::from_millis(250));
        let url = url::Url::parse(&format!("http://{address}/")).unwrap();
        let result = transport.get(&url).await;
        assert_eq!(result.unwrap_err(), TransportErrorKind::TimedOut);
        let _ = server.join();
    }

    // Pins the frozen HTTP-hardening constants (CLAUDE.md invariant: 12 s timeout,
    // 2 MiB cap).
    #[test]
    fn hardening_constants_match_the_frozen_spec() {
        assert_eq!(config::api::REQUEST_TIMEOUT, Duration::from_secs(12));
        assert_eq!(config::api::MAXIMUM_RESPONSE_BYTES, 2 * 1024 * 1024);
    }

    #[test]
    fn redirects_stay_on_the_original_https_origin() {
        let initial = url::Url::parse("https://api.dexscreener.com/start").unwrap();
        assert!(redirect_target_allowed(
            &initial,
            &url::Url::parse("https://api.dexscreener.com/next").unwrap()
        ));
        assert!(!redirect_target_allowed(
            &initial,
            &url::Url::parse("http://api.dexscreener.com/next").unwrap()
        ));
        assert!(!redirect_target_allowed(
            &initial,
            &url::Url::parse("https://example.com/next").unwrap()
        ));
        assert!(!redirect_target_allowed(
            &initial,
            &url::Url::parse("https://api.dexscreener.com:444/next").unwrap()
        ));
    }

    /// Synthetic error chain for exercising [`classify_connect_chain`] without a network.
    #[derive(Debug)]
    struct ChainError {
        message: &'static str,
        source: Option<Box<dyn std::error::Error + 'static>>,
    }

    impl std::fmt::Display for ChainError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.message)
        }
    }

    impl std::error::Error for ChainError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source
                .as_ref()
                .map(|error| error.as_ref() as &(dyn std::error::Error + 'static))
        }
    }

    fn connect_chain(
        message: &'static str,
        io_kind: Option<std::io::ErrorKind>,
    ) -> Box<ChainError> {
        Box::new(ChainError {
            message,
            source: io_kind.map(|kind| Box::new(std::io::Error::new(kind, "io failure")) as Box<_>),
        })
    }

    // Swift parity: FetchError.swift's transport `default:` arm classifies unrecognized
    // failures as `.unknown` (never outage-eligible). A TLS handshake/certificate
    // failure — io InvalidData under rustls, or no io error in the chain — must land
    // there, not on the outage-eligible DnsLookupFailed.
    #[test]
    fn classifies_tls_and_unrecognized_connect_failures_as_other() {
        let tls = connect_chain(
            "client error (Connect)",
            Some(std::io::ErrorKind::InvalidData),
        );
        assert_eq!(
            classify_connect_chain(tls.as_ref()),
            TransportErrorKind::Other
        );

        let opaque = connect_chain("client error (Connect): unexpected eof", None);
        let kind = classify_connect_chain(opaque.as_ref());
        assert_eq!(kind, TransportErrorKind::Other);
        assert!(!FetchError::Transport(kind)
            .status_failure_kind()
            .counts_toward_outage_confirmation());
    }

    // hyper-util wraps getaddrinfo failures in a ConnectError displaying "dns error";
    // that positive evidence — not a fallback — is what keeps DNS outages
    // outage-eligible (`URLError.dnsLookupFailed` -> serverUnreachable).
    #[test]
    fn classifies_dns_resolution_failures_as_dns_lookup_failed() {
        let dns = Box::new(ChainError {
            message: "client error (Connect)",
            source: Some(Box::new(ChainError {
                message: "dns error",
                source: Some(Box::new(std::io::Error::other(
                    "failed to lookup address information",
                ))),
            })),
        });
        let kind = classify_connect_chain(dns.as_ref());
        assert_eq!(kind, TransportErrorKind::DnsLookupFailed);
        assert!(FetchError::Transport(kind)
            .status_failure_kind()
            .counts_toward_outage_confirmation());
    }

    #[test]
    fn classifies_io_kinds_onto_the_urlerror_analogues() {
        let unreachable = connect_chain(
            "client error (Connect)",
            Some(std::io::ErrorKind::NetworkUnreachable),
        );
        assert_eq!(
            classify_connect_chain(unreachable.as_ref()),
            TransportErrorKind::NotConnectedToInternet
        );

        let refused = connect_chain(
            "client error (Connect)",
            Some(std::io::ErrorKind::ConnectionRefused),
        );
        assert_eq!(
            classify_connect_chain(refused.as_ref()),
            TransportErrorKind::CannotConnectToHost
        );
    }

    // Hermetic loopback: a genuinely refused TCP connect classifies as
    // CannotConnectToHost through the full reqwest error chain.
    #[tokio::test]
    async fn reqwest_transport_classifies_connection_refused_as_cannot_connect() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener); // Close the port so the connect is refused.

        let transport = ReqwestTransport::with_timeout(Duration::from_secs(5));
        let url = url::Url::parse(&format!("http://{address}/")).unwrap();
        let result = transport.get(&url).await;
        assert_eq!(result.unwrap_err(), TransportErrorKind::CannotConnectToHost);
    }

    // Hermetic loopback: a redirect to a non-https URL is refused (macOS ATS parity —
    // URLSession fails a cleartext redirect from the https feeds) and classifies as
    // Other/Unknown, never outage-eligible.
    #[tokio::test]
    async fn refuses_redirects_to_non_https_urls() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::{Read, Write};
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(
                    b"HTTP/1.1 301 Moved Permanently\r\n\
                      Location: http://127.0.0.1:9/\r\n\
                      Content-Length: 0\r\n\
                      Connection: close\r\n\r\n",
                );
            }
        });

        let transport = ReqwestTransport::with_timeout(Duration::from_secs(5));
        let url = url::Url::parse(&format!("http://{address}/")).unwrap();
        let result = transport.get(&url).await;
        let kind = result.unwrap_err();
        assert_eq!(kind, TransportErrorKind::Other);
        assert!(!FetchError::Transport(kind)
            .status_failure_kind()
            .counts_toward_outage_confirmation());
        let _ = server.join();
    }
}
