//! Retirement notice for an origin that is being replaced.
//!
//! When an instance serves a hostname with an announced end date, every
//! response carries the standard `Sunset` header (RFC 8594) and a `Link` to the
//! successor origin, so clients that never read an announcement can still
//! discover the deadline in-band. Both come from the instance's environment,
//! so the same build serves the retiring hostname and its successor.
//! Unset (the default) adds nothing.

use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::{Request, Response};

/// When the origin stops serving, as an HTTP-date: `Fri, 15 Jan 2027 00:00:00 GMT`.
pub const SUNSET_ENV: &str = "LIT_ORIGIN_SUNSET";
/// Base URL of the origin that replaces this one.
pub const SUCCESSOR_ENV: &str = "LIT_ORIGIN_SUCCESSOR_URL";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SunsetFairing {
    sunset: String,
    successor: Option<String>,
}

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Whether `value` is an IMF-fixdate (RFC 9110 §5.6.7), the only form `Sunset` allows.
fn is_http_date(value: &str) -> bool {
    // "Sun, 06 Nov 1994 08:49:37 GMT"
    let bytes = value.as_bytes();
    if bytes.len() != 29 || !value.is_ascii() {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| value[range].bytes().all(|b| b.is_ascii_digit());
    let number = |range: std::ops::Range<usize>| value[range].parse::<u32>().unwrap_or(u32::MAX);
    DAYS.contains(&&value[0..3])
        && &value[3..5] == ", "
        && digits(5..7)
        && bytes[7] == b' '
        && MONTHS.contains(&&value[8..11])
        && bytes[11] == b' '
        && digits(12..16)
        && bytes[16] == b' '
        && digits(17..19)
        && bytes[19] == b':'
        && digits(20..22)
        && bytes[22] == b':'
        && digits(23..25)
        && &value[25..] == " GMT"
        && (1..=31).contains(&number(5..7))
        && number(17..19) < 24
        && number(20..22) < 60
        && number(23..25) < 61
}

/// A successor must be a plain `https` origin: it is echoed into a header.
fn is_successor_url(value: &str) -> bool {
    value.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'/' | b'_'))
    })
}

impl SunsetFairing {
    /// Builds the notice from already-read settings. A missing or malformed
    /// sunset date means no notice at all; a malformed successor is dropped.
    pub fn new(sunset: Option<&str>, successor: Option<&str>) -> Option<Self> {
        let sunset = sunset.map(str::trim).filter(|value| !value.is_empty())?;
        if !is_http_date(sunset) {
            tracing::error!(
                "{SUNSET_ENV} is not an HTTP-date such as \"Fri, 15 Jan 2027 00:00:00 GMT\"; no Sunset header will be sent"
            );
            return None;
        }
        let successor = successor
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| {
                if is_successor_url(value) {
                    Some(value.trim_end_matches('/').to_string())
                } else {
                    tracing::error!(
                        "{SUCCESSOR_ENV} is not an https URL; no successor link will be sent"
                    );
                    None
                }
            });
        Some(Self {
            sunset: sunset.to_string(),
            successor,
        })
    }

    pub fn from_env() -> Option<Self> {
        Self::new(
            std::env::var(SUNSET_ENV).ok().as_deref(),
            std::env::var(SUCCESSOR_ENV).ok().as_deref(),
        )
    }
}

#[rocket::async_trait]
impl Fairing for SunsetFairing {
    fn info(&self) -> Info {
        Info {
            name: "Origin sunset notice",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _req: &'r Request<'_>, res: &mut Response<'r>) {
        res.set_header(Header::new("Sunset", self.sunset.clone()));
        if let Some(successor) = &self.successor {
            // Added, not set: a response may already carry Link headers.
            res.adjoin_header(Header::new(
                "Link",
                format!("<{successor}>; rel=\"successor-version\""),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::{get, routes};

    const DATE: &str = "Fri, 15 Jan 2027 00:00:00 GMT";

    #[test]
    fn only_http_dates_are_accepted() {
        assert!(is_http_date(DATE));
        assert!(is_http_date("Sun, 06 Nov 1994 08:49:37 GMT"));
        for bad in [
            "",
            "2027-01-15",
            "2027-01-15T00:00:00Z",
            "Fri, 15 Jan 2027 00:00:00 UTC",
            "Fri, 15 Jan 2027 00:00:00 GMT ",
            "Fri, 32 Jan 2027 00:00:00 GMT",
            "Fri, 15 Jan 2027 24:00:00 GMT",
            "Fri, 15 Foo 2027 00:00:00 GMT",
            "Fri, 15 Jan 2027 00:00:00 GMT\r\nX: y",
        ] {
            assert!(!is_http_date(bad), "{bad:?}");
        }
    }

    #[test]
    fn notice_requires_a_valid_date_and_tolerates_a_bad_successor() {
        assert_eq!(SunsetFairing::new(None, Some("https://new.example")), None);
        assert_eq!(SunsetFairing::new(Some("  "), None), None);
        assert_eq!(SunsetFairing::new(Some("soon"), None), None);
        let notice = SunsetFairing::new(Some(DATE), Some("https://new.example/")).unwrap();
        assert_eq!(notice.successor.as_deref(), Some("https://new.example"));
        for bad in [
            "http://new.example",
            "new.example",
            "https://",
            "https://a.example>; rel=x",
        ] {
            let notice = SunsetFairing::new(Some(DATE), Some(bad)).unwrap();
            assert_eq!(notice.successor, None, "{bad:?}");
        }
    }

    #[get("/ping")]
    fn ping() -> &'static str {
        "pong"
    }

    #[rocket::async_test]
    async fn headers_are_added_to_every_response_including_errors() {
        let notice = SunsetFairing::new(Some(DATE), Some("https://new.example")).unwrap();
        let client = rocket::local::asynchronous::Client::tracked(
            rocket::build().attach(notice).mount("/", routes![ping]),
        )
        .await
        .unwrap();
        for path in ["/ping", "/missing"] {
            let response = client.get(path).dispatch().await;
            assert_eq!(response.headers().get_one("Sunset"), Some(DATE));
            assert_eq!(
                response.headers().get_one("Link"),
                Some("<https://new.example>; rel=\"successor-version\"")
            );
        }
    }

    #[rocket::async_test]
    async fn nothing_is_added_without_configuration() {
        let client =
            rocket::local::asynchronous::Client::tracked(rocket::build().mount("/", routes![ping]))
                .await
                .unwrap();
        let response = client.get("/ping").dispatch().await;
        assert_eq!(response.headers().get_one("Sunset"), None);
        assert_eq!(response.headers().get_one("Link"), None);
    }
}
