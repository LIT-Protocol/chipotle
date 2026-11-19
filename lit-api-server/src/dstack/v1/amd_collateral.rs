//! AMD endorsement certificates for this instance's SEV-SNP reports.
//!
//! An SNP report is signed by a per-chip key (VCEK) whose certificate AMD
//! serves from its Key Distribution Service. Verifiers need that certificate
//! and AMD's intermediate (ASK). AMD's service sends no CORS headers and rate
//! limits by address, so browsers cannot fetch them and many clients behind
//! one address may be refused. The instance fetches them once and hands them
//! out with its evidence.
//!
//! They are public and AMD-signed: a verifier checks them against AMD root
//! keys it pins itself, so serving them here adds no trust in this server. A
//! wrong or missing certificate can only make verification fail.

use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const KDS_BASE_URL: &str = "https://kdsintf.amd.com/vcek/v1";
const SNP_REPORT_LEN: usize = 1184;
const MAX_CERT_BYTES: usize = 64 * 1024;
/// Minimum gap between fetch attempts, so a failing KDS is not hammered.
const RETRY_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AmdCollateral {
    /// VCEK certificate for this chip and firmware level: DER, hex.
    pub vcek: String,
    /// AMD SEV intermediate (ASK) certificate: PEM.
    pub ask: String,
}

/// What identifies the VCEK AMD issues for a report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VcekRequest {
    product: &'static str,
    /// Lowercase hex chip id, as it appears in the KDS path.
    chip_id: String,
    /// Reported TCB, as the KDS query string.
    tcb_query: String,
}

impl VcekRequest {
    fn vcek_url(&self, base: &str) -> String {
        format!(
            "{base}/{}/{}?{}",
            self.product, self.chip_id, self.tcb_query
        )
    }

    fn cert_chain_url(&self, base: &str) -> String {
        format!("{base}/{}/cert_chain", self.product)
    }
}

/// Reads the processor family, chip id and reported firmware versions from an
/// SNP report. Returns `None` for anything this does not recognise: the
/// certificates are a convenience, so there is nothing to fail.
pub fn vcek_request(report: &[u8]) -> Option<VcekRequest> {
    if report.len() != SNP_REPORT_LEN {
        return None;
    }
    // Report version 3 added the CPUID fields that name the product family.
    if u32::from_le_bytes(report[0..4].try_into().ok()?) != 3 {
        return None;
    }
    let (family, model) = (report[0x188], report[0x189]);
    let product = match (family, model) {
        (0x19, 0x00..=0x0f) => "Milan",
        (0x19, 0x10..=0x1f) | (0x19, 0xa0..=0xaf) => "Genoa",
        (0x1a, 0x00..=0x11) => "Turin",
        _ => return None,
    };
    let chip = &report[0x1a0..0x1e0];
    let tcb = &report[0x180..0x188];
    Some(if product == "Turin" {
        VcekRequest {
            product,
            chip_id: hex::encode(&chip[..8]),
            tcb_query: format!(
                "fmcSPL={}&blSPL={}&teeSPL={}&snpSPL={}&ucodeSPL={}",
                tcb[0], tcb[1], tcb[2], tcb[3], tcb[7]
            ),
        }
    } else {
        VcekRequest {
            product,
            chip_id: hex::encode(chip),
            tcb_query: format!(
                "blSPL={}&teeSPL={}&snpSPL={}&ucodeSPL={}",
                tcb[0], tcb[1], tcb[6], tcb[7]
            ),
        }
    })
}

/// Finds the SNP report inside a dstack VersionedAttestation, in either wire
/// format. `None` for any other platform or an envelope this cannot read.
pub fn snp_report(evidence: &[u8]) -> Option<std::borrow::Cow<'_, [u8]>> {
    use std::borrow::Cow;
    match *evidence.first()? {
        // SCALE: version tag 0, platform variant 3 (SEV-SNP), then the report
        // as a length-prefixed byte vector.
        0x00 => {
            if *evidence.get(1)? != 3 {
                return None;
            }
            let (len, used) = scale_compact(evidence.get(2..)?)?;
            evidence.get(2 + used..2 + used + len).map(Cow::Borrowed)
        }
        // MessagePack map: { platform: { kind: "sev-snp", data: { report } } }.
        0x80..=0x8f | 0xde | 0xdf => {
            let platform = msgpack::map_get(evidence, "platform")?;
            if msgpack::str(msgpack::map_get(platform, "kind")?)? != "sev-snp" {
                return None;
            }
            msgpack::bytes(msgpack::map_get(
                msgpack::map_get(platform, "data")?,
                "report",
            )?)
        }
        _ => None,
    }
    .filter(|report| report.len() == SNP_REPORT_LEN)
}

fn scale_compact(bytes: &[u8]) -> Option<(usize, usize)> {
    let first = *bytes.first()? as usize;
    match first & 3 {
        0 => Some((first >> 2, 1)),
        1 => Some(((first | (*bytes.get(1)? as usize) << 8) >> 2, 2)),
        2 => {
            let rest = bytes.get(1..4)?;
            let value = first
                | (rest[0] as usize) << 8
                | (rest[1] as usize) << 16
                | (rest[2] as usize) << 24;
            Some((value >> 2, 4))
        }
        _ => None,
    }
}

/// Just enough MessagePack to walk string-keyed maps down to a binary value.
mod msgpack {
    fn be(bytes: &[u8], at: usize, len: usize) -> Option<usize> {
        let mut value = 0usize;
        for byte in bytes.get(at..at + len)? {
            value = value.checked_mul(256)?.checked_add(*byte as usize)?;
        }
        Some(value)
    }

    /// (kind, header length, payload length or element count) of the value at
    /// the start. Kinds are normalised: 0xc4 binary, 0xd9 string, 0xdc array,
    /// 0xde map; anything else is a scalar whose whole encoding is the header.
    fn header(bytes: &[u8]) -> Option<(u8, usize, usize)> {
        let tag = *bytes.first()?;
        Some(match tag {
            0x00..=0x7f | 0xe0..=0xff | 0xc0 | 0xc2 | 0xc3 => (tag, 1, 0),
            0x80..=0x8f => (0xde, 1, (tag & 0x0f) as usize),
            0x90..=0x9f => (0xdc, 1, (tag & 0x0f) as usize),
            0xa0..=0xbf => (0xd9, 1, (tag & 0x1f) as usize),
            0xc4 => (0xc4, 2, be(bytes, 1, 1)?),
            0xc5 => (0xc4, 3, be(bytes, 1, 2)?),
            0xc6 => (0xc4, 5, be(bytes, 1, 4)?),
            0xd9 => (0xd9, 2, be(bytes, 1, 1)?),
            0xda => (0xd9, 3, be(bytes, 1, 2)?),
            0xdb => (0xd9, 5, be(bytes, 1, 4)?),
            0xcc | 0xd0 => (tag, 2, 0),
            0xcd | 0xd1 => (tag, 3, 0),
            0xce | 0xd2 | 0xca => (tag, 5, 0),
            0xcf | 0xd3 | 0xcb => (tag, 9, 0),
            0xdc => (0xdc, 3, be(bytes, 1, 2)?),
            0xdd => (0xdc, 5, be(bytes, 1, 4)?),
            0xde => (0xde, 3, be(bytes, 1, 2)?),
            0xdf => (0xde, 5, be(bytes, 1, 4)?),
            _ => return None,
        })
    }

    /// Encoded length of the value at the start of `bytes`.
    fn skip(bytes: &[u8], depth: u8) -> Option<usize> {
        if depth > 16 {
            return None;
        }
        let (kind, head, n) = header(bytes)?;
        let total = match kind {
            0xc4 | 0xd9 => head.checked_add(n)?,
            0xdc | 0xde => {
                let items = if kind == 0xde { n.checked_mul(2)? } else { n };
                let mut at = head;
                for _ in 0..items {
                    at = at.checked_add(skip(bytes.get(at..)?, depth + 1)?)?;
                }
                at
            }
            _ => head,
        };
        (total <= bytes.len()).then_some(total)
    }

    pub fn str(bytes: &[u8]) -> Option<&str> {
        let (kind, head, n) = header(bytes)?;
        if kind != 0xd9 {
            return None;
        }
        std::str::from_utf8(bytes.get(head..head + n)?).ok()
    }

    /// A byte vector, in either form serde writes one: a binary string, or an
    /// array of integers 0..=255.
    pub fn bytes(bytes: &[u8]) -> Option<std::borrow::Cow<'_, [u8]>> {
        let (kind, head, n) = header(bytes)?;
        match kind {
            0xc4 => bytes.get(head..head + n).map(std::borrow::Cow::Borrowed),
            0xdc if n <= 4096 => {
                let mut out = Vec::with_capacity(n);
                let mut at = head;
                for _ in 0..n {
                    match *bytes.get(at)? {
                        value @ 0x00..=0x7f => {
                            out.push(value);
                            at += 1;
                        }
                        0xcc => {
                            out.push(*bytes.get(at + 1)?);
                            at += 2;
                        }
                        _ => return None,
                    }
                }
                Some(std::borrow::Cow::Owned(out))
            }
            _ => None,
        }
    }

    /// The encoded value stored under `key` in the map at the start of `bytes`.
    pub fn map_get<'a>(bytes: &'a [u8], key: &str) -> Option<&'a [u8]> {
        let (kind, head, n) = header(bytes)?;
        if kind != 0xde {
            return None;
        }
        let mut at = head;
        for _ in 0..n {
            let key_bytes = bytes.get(at..)?;
            let key_len = skip(key_bytes, 0)?;
            let value = bytes.get(at + key_len..)?;
            let value_len = skip(value, 0)?;
            if str(key_bytes) == Some(key) {
                return value.get(..value_len);
            }
            at += key_len + value_len;
        }
        None
    }
}

struct Cached {
    request: VcekRequest,
    collateral: AmdCollateral,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);
static LAST_ATTEMPT: Mutex<Option<Instant>> = Mutex::new(None);
static FETCHING: AtomicBool = AtomicBool::new(false);

async fn get_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("AMD KDS request failed: {}", e.without_url()))?;
    // Bound the body while reading it, not after it is all in memory.
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("AMD KDS response unreadable: {}", e.without_url()))?
    {
        if chunk.len() > MAX_CERT_BYTES.saturating_sub(body.len()) {
            return Err("AMD KDS response exceeds the size limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    if body.is_empty() {
        return Err("AMD KDS response is empty".into());
    }
    Ok(body)
}

/// The first PEM certificate in AMD's `cert_chain` response, which is the ASK.
fn first_pem_certificate(chain: &[u8]) -> Option<String> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let text = std::str::from_utf8(chain).ok()?;
    let start = text.find(BEGIN)?;
    let end = start + text[start..].find(END)? + END.len();
    Some(format!("{}\n", &text[start..end]))
}

async fn fetch(request: &VcekRequest, base: &str) -> Result<AmdCollateral, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("failed to create AMD KDS client: {e}"))?;
    let vcek = get_bytes(&client, &request.vcek_url(base)).await?;
    // A DER certificate is one SEQUENCE.
    if vcek.first() != Some(&0x30) {
        return Err("AMD KDS did not return a DER certificate".into());
    }
    let chain = get_bytes(&client, &request.cert_chain_url(base)).await?;
    let ask = first_pem_certificate(&chain)
        .ok_or_else(|| "AMD KDS cert_chain has no certificate".to_string())?;
    Ok(AmdCollateral {
        vcek: hex::encode(vcek),
        ask,
    })
}

/// Certificates for the report inside `evidence`, if they are already cached.
/// Never waits on the network: a miss starts one background fetch (at most one
/// per `RETRY_INTERVAL`) and returns `None`, and the client falls back to AMD.
pub fn for_evidence(evidence: &[u8]) -> Option<AmdCollateral> {
    let request = vcek_request(&snp_report(evidence)?)?;
    if let Some(cached) = CACHE.lock().ok()?.as_ref()
        && cached.request == request
    {
        return Some(cached.collateral.clone());
    }
    spawn_fetch(request);
    None
}

fn spawn_fetch(request: VcekRequest) {
    {
        let Ok(mut last) = LAST_ATTEMPT.lock() else {
            return;
        };
        if last.is_some_and(|at| at.elapsed() < RETRY_INTERVAL) {
            return;
        }
        if FETCHING.swap(true, Ordering::SeqCst) {
            return;
        }
        *last = Some(Instant::now());
    }
    tokio::spawn(async move {
        match fetch(&request, KDS_BASE_URL).await {
            Ok(collateral) => {
                tracing::info!(
                    product = request.product,
                    "cached AMD endorsement certificates"
                );
                if let Ok(mut cache) = CACHE.lock() {
                    *cache = Some(Cached {
                        request,
                        collateral,
                    });
                }
            }
            Err(e) => tracing::warn!(error = %e, "could not fetch AMD endorsement certificates"),
        }
        FETCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The staging report the JavaScript verifier is tested against.
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../lit-static/testdata/attestation-sev-snp.json"
        ))
        .unwrap()
    }

    fn evidence() -> Vec<u8> {
        hex::decode(fixture()["attestation"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn real_evidence_yields_the_kds_request_the_sdk_derives() {
        let evidence = evidence();
        let report = snp_report(&evidence).expect("SEV-SNP report");
        let request = vcek_request(&report).expect("recognised product");
        // Must equal lit-static/attestation.test.mjs "KDS URL binds the chip".
        let url = request.vcek_url(KDS_BASE_URL);
        assert!(
            url.starts_with("https://kdsintf.amd.com/vcek/v1/Milan/cc618c5b"),
            "{url}"
        );
        assert!(
            url.ends_with("?blSPL=4&teeSPL=0&snpSPL=24&ucodeSPL=222"),
            "{url}"
        );
        assert_eq!(request.chip_id.len(), 128);
        assert_eq!(
            request.cert_chain_url(KDS_BASE_URL),
            "https://kdsintf.amd.com/vcek/v1/Milan/cert_chain"
        );
    }

    #[test]
    fn turin_uses_the_short_chip_id_and_fmc() {
        let mut report = vec![0u8; SNP_REPORT_LEN];
        report[0] = 3;
        report[0x188] = 0x1a;
        report[0x189] = 0x02;
        report[0x180..0x188].copy_from_slice(&[1, 2, 3, 4, 0, 0, 0, 9]);
        report[0x1a0..0x1a8].copy_from_slice(&[0xaa; 8]);
        let request = vcek_request(&report).unwrap();
        assert_eq!(
            request.vcek_url("https://kds"),
            "https://kds/Turin/aaaaaaaaaaaaaaaa?fmcSPL=1&blSPL=2&teeSPL=3&snpSPL=4&ucodeSPL=9"
        );
    }

    #[test]
    fn unrecognised_reports_and_envelopes_yield_nothing() {
        assert_eq!(vcek_request(&[0u8; 10]), None);
        let mut report = vec![0u8; SNP_REPORT_LEN];
        report[0] = 2; // no CPUID fields before version 3
        assert_eq!(vcek_request(&report), None);
        report[0] = 3;
        report[0x188] = 0x17; // not an SNP-capable family
        assert_eq!(vcek_request(&report), None);

        let evidence = evidence();
        assert!(snp_report(&[]).is_none());
        assert!(snp_report(&[0x00]).is_none());
        assert!(
            snp_report(&[0x00, 0x00, 0x04, 0xff]).is_none(),
            "TDX variant"
        );
        assert!(snp_report(&evidence[..600]).is_none(), "truncated");
        assert!(snp_report(&[0xff, 0x00]).is_none());
        assert!(for_evidence(&[0x00, 0x00]).is_none());
    }

    #[test]
    fn report_is_found_in_a_messagepack_envelope() {
        let scale = evidence();
        let report = snp_report(&scale).unwrap().to_vec();
        fn s(out: &mut Vec<u8>, value: &str) {
            out.push(0xa0 | value.len() as u8);
            out.extend_from_slice(value.as_bytes());
        }
        let mut doc = vec![0x83];
        s(&mut doc, "version");
        doc.push(1);
        // A value before the one we want, so lookup has to skip nested data.
        s(&mut doc, "stack");
        doc.push(0x82);
        s(&mut doc, "kind");
        s(&mut doc, "dstack");
        s(&mut doc, "data");
        doc.extend_from_slice(&[0x92, 0xcd, 0x01, 0x00, 0xc4, 0x02, 0xaa, 0xbb]);
        s(&mut doc, "platform");
        doc.push(0x82);
        s(&mut doc, "kind");
        s(&mut doc, "sev-snp");
        s(&mut doc, "data");
        doc.push(0x82);
        s(&mut doc, "mr_config");
        s(&mut doc, "{}");
        s(&mut doc, "report");
        doc.extend_from_slice(&[0xc5, (report.len() >> 8) as u8, report.len() as u8]);
        doc.extend_from_slice(&report);
        assert_eq!(snp_report(&doc).as_deref(), Some(report.as_slice()));

        // serde also writes a byte vector as an array of integers.
        let mut array_doc = doc[..doc.len() - report.len() - 3].to_vec();
        array_doc.extend_from_slice(&[0xdc, (report.len() >> 8) as u8, report.len() as u8]);
        for byte in &report {
            if *byte < 0x80 {
                array_doc.push(*byte);
            } else {
                array_doc.extend_from_slice(&[0xcc, *byte]);
            }
        }
        assert_eq!(snp_report(&array_doc).as_deref(), Some(report.as_slice()));

        // A truncated document, and another platform, yield nothing.
        assert!(snp_report(&doc[..doc.len() - 1]).is_none());
        let at = doc.windows(7).position(|w| w == b"sev-snp").unwrap();
        doc[at..at + 7].copy_from_slice(b"tdx-xxx");
        assert!(snp_report(&doc).is_none());
    }

    #[test]
    fn ask_is_the_first_certificate_of_the_chain() {
        let chain = b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nBBBB\n-----END CERTIFICATE-----\n";
        assert_eq!(
            first_pem_certificate(chain).unwrap(),
            "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n"
        );
        assert!(first_pem_certificate(b"not pem").is_none());
    }
}
