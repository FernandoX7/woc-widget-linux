//! Typed fetch failures and their realm-oriented classification.
//!
//! Port of Networking/FetchError.swift. `StatusFailureKind` lives here rather than in a
//! models module (Swift keeps it in Models.swift) because it is the classification target
//! of [`FetchError::status_failure_kind`]; the file-layout divergence is deliberate.

use crate::strings;

/// The `URLError` transport codes the spec classifies, as data.
///
/// Swift wraps a full `URLError`; Rust gets a closed enum so the classification is
/// exactly portable and constructible in tests. There is no `Cancelled` case: Swift
/// converts `URLError.cancelled` into `CancellationError` before wrapping, and in Rust
/// cancellation is dropping the future, so a cancelled request never produces an error
/// value at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransportErrorKind {
    /// `URLError.notConnectedToInternet`.
    NotConnectedToInternet,
    /// `URLError.networkConnectionLost`.
    NetworkConnectionLost,
    /// `URLError.timedOut`.
    TimedOut,
    /// `URLError.cannotFindHost`.
    CannotFindHost,
    /// `URLError.cannotConnectToHost`.
    CannotConnectToHost,
    /// `URLError.dnsLookupFailed`.
    DnsLookupFailed,
    /// Every other transport failure (Swift's `default` branch over `URLError.code`).
    Other,
}

/// A typed fetch failure, split so transport blips, HTTP statuses, response bounds, and
/// schema breaks are distinguishable. Swift parity: `FetchError` in
/// Networking/FetchError.swift.
///
/// Swift also throws `http(-1)` when the response is not HTTP at all; that case is
/// unrepresentable under reqwest (responses are always HTTP), so no sentinel exists here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The request never produced an HTTP response.
    Transport(TransportErrorKind),
    /// The server answered with a non-200 status.
    Http(u16),
    /// The 200 body exceeded the response cap; checked before decoding.
    ResponseTooLarge {
        /// Observed body size (under the streaming transport, at most `maximum + 1`).
        bytes: usize,
        /// The configured cap ([`crate::config::api::MAXIMUM_RESPONSE_BYTES`]).
        maximum: usize,
    },
    /// The body was not valid JSON for the expected schema.
    Decode,
}

impl std::fmt::Display for FetchError {
    // Developer-facing (logs). User-facing text goes through friendly_message().
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Transport(kind) => {
                let description = match kind {
                    TransportErrorKind::NotConnectedToInternet => "not connected to the internet",
                    TransportErrorKind::NetworkConnectionLost => "network connection lost",
                    TransportErrorKind::TimedOut => "request timed out",
                    TransportErrorKind::CannotFindHost => "cannot find host",
                    TransportErrorKind::CannotConnectToHost => "cannot connect to host",
                    TransportErrorKind::DnsLookupFailed => "DNS lookup failed",
                    TransportErrorKind::Other => "transport failure",
                };
                write!(f, "transport error: {description}")
            }
            FetchError::Http(status) => write!(f, "unexpected HTTP status {status}"),
            FetchError::ResponseTooLarge { bytes, maximum } => {
                write!(
                    f,
                    "response too large: {bytes} bytes exceeds the {maximum}-byte cap"
                )
            }
            FetchError::Decode => write!(f, "response body failed to decode"),
        }
    }
}

impl std::error::Error for FetchError {}

/// Realm-oriented classification used by the UI and outage alert policy. This
/// deliberately separates problems on this machine from evidence that the remote realm
/// may be unavailable. Swift parity: `StatusFailureKind` (declared in Models.swift).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusFailureKind {
    /// This machine's connectivity is the problem.
    LocalNetwork,
    /// The server did not answer in time.
    TimedOut,
    /// The server could not be reached at all.
    ServerUnreachable,
    /// The server answered with a 5xx status.
    ServerError,
    /// The server answered, but not usably (non-5xx status, oversized or undecodable body).
    InvalidResponse,
    /// Unclassified transport failure.
    Unknown,
}

impl StatusFailureKind {
    /// Whether this failure is evidence the realm may be down. Only remote,
    /// outage-eligible failures count; decode/local errors never do (invariant: market
    /// failures never change realm availability).
    pub const fn counts_toward_outage_confirmation(&self) -> bool {
        matches!(
            self,
            Self::TimedOut | Self::ServerUnreachable | Self::ServerError
        )
    }
}

impl FetchError {
    /// Classifies this failure for the outage alert policy.
    /// Swift parity: `FetchError.statusFailureKind`.
    pub fn status_failure_kind(&self) -> StatusFailureKind {
        match self {
            FetchError::Transport(kind) => match kind {
                TransportErrorKind::NotConnectedToInternet
                | TransportErrorKind::NetworkConnectionLost => StatusFailureKind::LocalNetwork,
                TransportErrorKind::TimedOut => StatusFailureKind::TimedOut,
                TransportErrorKind::CannotFindHost
                | TransportErrorKind::CannotConnectToHost
                | TransportErrorKind::DnsLookupFailed => StatusFailureKind::ServerUnreachable,
                TransportErrorKind::Other => StatusFailureKind::Unknown,
            },
            FetchError::Http(status) if (500..=599).contains(status) => {
                StatusFailureKind::ServerError
            }
            FetchError::Http(_) => StatusFailureKind::InvalidResponse,
            FetchError::ResponseTooLarge { .. } | FetchError::Decode => {
                StatusFailureKind::InvalidResponse
            }
        }
    }

    /// User-facing message, matching the Swift `friendlyMessage` table exactly.
    pub fn friendly_message(&self) -> String {
        match self {
            FetchError::Transport(kind) => match kind {
                TransportErrorKind::NotConnectedToInternet
                | TransportErrorKind::NetworkConnectionLost => strings::error_no_internet(),
                TransportErrorKind::TimedOut => strings::error_timed_out(),
                TransportErrorKind::CannotFindHost
                | TransportErrorKind::CannotConnectToHost
                | TransportErrorKind::DnsLookupFailed => strings::error_unreachable(),
                TransportErrorKind::Other => strings::error_connection(),
            },
            FetchError::Http(_) => strings::error_connection(),
            FetchError::ResponseTooLarge { .. } | FetchError::Decode => strings::error_generic(),
        }
    }
}

/// Store-side fallback for any error reaching a status catch site: a [`FetchError`] maps
/// via its friendly message; anything else gets the generic message.
/// Swift parity: the free function `friendly(_:)` in Networking/FetchError.swift.
pub fn friendly(error: &(dyn std::error::Error + 'static)) -> String {
    match error.downcast_ref::<FetchError>() {
        Some(fetch_error) => fetch_error.friendly_message(),
        None => strings::error_generic(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-in for Swift's `CancellationError`: any non-FetchError error type.
    #[derive(Debug)]
    struct CancellationLikeError;

    impl std::fmt::Display for CancellationLikeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "operation cancelled")
        }
    }

    impl std::error::Error for CancellationLikeError {}

    // Swift parity: DecodingErrorTests.swift > FetchErrorTests > mapsTransportCodes
    #[test]
    fn maps_transport_codes() {
        assert_eq!(
            friendly(&FetchError::Transport(
                TransportErrorKind::NotConnectedToInternet
            )),
            "No internet connection"
        );
        assert_eq!(
            friendly(&FetchError::Transport(
                TransportErrorKind::NetworkConnectionLost
            )),
            "No internet connection"
        );
        assert_eq!(
            friendly(&FetchError::Transport(TransportErrorKind::TimedOut)),
            "Server timed out"
        );
        assert_eq!(
            friendly(&FetchError::Transport(TransportErrorKind::CannotFindHost)),
            "Can't reach server"
        );
        assert_eq!(
            friendly(&FetchError::Transport(
                TransportErrorKind::CannotConnectToHost
            )),
            "Can't reach server"
        );
        assert_eq!(
            friendly(&FetchError::Transport(TransportErrorKind::DnsLookupFailed)),
            "Can't reach server"
        );
        // Swift asserts URLError(.badServerResponse), which lands in the default branch;
        // TransportErrorKind::Other is that branch.
        assert_eq!(
            friendly(&FetchError::Transport(TransportErrorKind::Other)),
            "Connection error"
        );
    }

    // Swift parity: DecodingErrorTests.swift > FetchErrorTests > mapsHttpAndDecodeAndUnknown
    #[test]
    fn maps_http_and_decode_and_unknown() {
        assert_eq!(friendly(&FetchError::Http(503)), "Connection error");
        assert_eq!(friendly(&FetchError::Decode), "Couldn't load status");
        assert_eq!(friendly(&CancellationLikeError), "Couldn't load status");
    }

    // Swift parity: DecodingErrorTests.swift > FetchErrorTests >
    // classifiesLocalAndRemoteFailuresForOutageConfirmation
    #[test]
    fn classifies_local_and_remote_failures_for_outage_confirmation() {
        assert_eq!(
            FetchError::Transport(TransportErrorKind::NotConnectedToInternet).status_failure_kind(),
            StatusFailureKind::LocalNetwork
        );
        assert_eq!(
            FetchError::Transport(TransportErrorKind::TimedOut).status_failure_kind(),
            StatusFailureKind::TimedOut
        );
        assert_eq!(
            FetchError::Http(503).status_failure_kind(),
            StatusFailureKind::ServerError
        );
        assert_eq!(
            FetchError::Http(401).status_failure_kind(),
            StatusFailureKind::InvalidResponse
        );
        assert_eq!(
            FetchError::ResponseTooLarge {
                bytes: 3,
                maximum: 2
            }
            .status_failure_kind(),
            StatusFailureKind::InvalidResponse
        );
        assert!(StatusFailureKind::TimedOut.counts_toward_outage_confirmation());
        assert!(!StatusFailureKind::LocalNetwork.counts_toward_outage_confirmation());
    }

    // Pins the frozen invariant: ONLY timedOut / serverUnreachable / serverError are
    // outage-eligible; decode/local/unknown failures never count.
    #[test]
    fn only_remote_failures_are_outage_eligible() {
        assert!(StatusFailureKind::TimedOut.counts_toward_outage_confirmation());
        assert!(StatusFailureKind::ServerUnreachable.counts_toward_outage_confirmation());
        assert!(StatusFailureKind::ServerError.counts_toward_outage_confirmation());
        assert!(!StatusFailureKind::LocalNetwork.counts_toward_outage_confirmation());
        assert!(!StatusFailureKind::InvalidResponse.counts_toward_outage_confirmation());
        assert!(!StatusFailureKind::Unknown.counts_toward_outage_confirmation());
    }
}
