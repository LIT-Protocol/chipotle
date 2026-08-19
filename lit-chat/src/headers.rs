//! Security-header fairing (plans/tee-chat-app.md section 4.1 hard
//! requirements): strict CSP (self-only scripts, no inline, no CDN), plus
//! the usual hardening headers. Applied to every response from both
//! binaries — static assets included.

use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::{Request, Response};

pub struct SecurityHeaders;

#[rocket::async_trait]
impl Fairing for SecurityHeaders {
    fn info(&self) -> Info {
        Info {
            name: "Security headers (CSP, no-sniff, frame-deny)",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _req: &'r Request<'_>, res: &mut Response<'r>) {
        res.set_header(Header::new(
            "Content-Security-Policy",
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; \
             img-src 'self' data:; font-src 'self'; base-uri 'none'; form-action 'self'; \
             frame-ancestors 'none'",
        ));
        res.set_header(Header::new("X-Content-Type-Options", "nosniff"));
        res.set_header(Header::new("X-Frame-Options", "DENY"));
        res.set_header(Header::new("Referrer-Policy", "no-referrer"));
        res.set_header(Header::new(
            "Permissions-Policy",
            "geolocation=(), microphone=(), camera=()",
        ));
        // No content is cacheable by shared caches; the app is cookie-scoped.
        res.set_header(Header::new("Cache-Control", "no-store"));
    }
}
