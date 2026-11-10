//! Подписи SIS и сертификаты X.509: **математическая** проверка, не доверие.
//!
//! Корректная подпись, даты сертификата, доверие конкретного телефона, отзыв, привязки
//! IMEI/SID/VID и успешная установка — разные факты. Здесь проверяется только первое и второе;
//! доверие телефона всегда `unknown`, отзыв `not-checked`. Алгоритмы: RSA/SHA-1 и RSA/SHA-256
//! (подпись SIS), цепочка — RSA SHA-1/256/384/512 и ECDSA P-256/P-384. DSA/SHA-1 (допустим в SIS)
//! помечается `unsupported`, а не «верно» и не «неверно». SHA-1 — устаревший алгоритм, это помечено.

use super::sis::{self, Cursor, SisError};
use ring::signature::{self, UnparsedPublicKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use x509_parser::prelude::*;

const MAX_CHAIN_BYTES: usize = 512 * 1024;
const MAX_CERTS: usize = 16;
const MAX_CHAINS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Extension {
    pub oid: String,
    pub critical: bool,
    pub interpreted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CertInfo {
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub sha256: String,
    pub not_before: String,
    pub not_after: String,
    /// `valid` | `expired` | `not-yet-valid` на момент проверки.
    pub validity: String,
    pub key_type: String,
    pub self_issued: bool,
    pub extensions: Vec<Extension>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignatureInfo {
    pub oid: String,
    /// `valid` | `invalid` | `unsupported`
    pub status: String,
    pub legacy_sha1: bool,
    pub signed_bytes: usize,
    pub signed_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainReport {
    /// `valid` | `invalid` | `unsupported`
    pub status: String,
    pub certificates: Vec<CertInfo>,
    pub signatures: Vec<SignatureInfo>,
    /// Всегда `unknown`: корневые сертификаты телефона здесь не проверяются.
    pub phone_trust: String,
    /// `valid-self-signature` | `invalid` | `issuer-missing` | `unsupported`
    pub root_signature: String,
    pub chain_links: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignatureReport {
    pub chains: Vec<ChainReport>,
    /// `unsigned` | `valid` | `invalid` | `unsupported`
    pub status: String,
    pub trust: String,
    pub revocation: String,
    pub phone_constraints: String,
    pub at: String,
}

/// Секунды Unix → `ГГГГ-ММ-ДДTЧЧ:ММ:СС+00:00`.
pub fn iso_utc(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let rem = ts.rem_euclid(86_400);
    // civil_from_days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}+00:00",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn split_der(raw: &[u8]) -> Result<Vec<&[u8]>, String> {
    if raw.len() > MAX_CHAIN_BYTES {
        return Err("Certificate chain exceeds limit".into());
    }
    let mut rest = raw;
    let mut out = Vec::new();
    while !rest.is_empty() {
        if out.len() >= MAX_CERTS || rest.len() < 2 || rest[0] != 0x30 {
            return Err("Invalid DER certificate chain".into());
        }
        let (mut size, mut offset) = (rest[1] as usize, 2usize);
        if rest[1] & 0x80 != 0 {
            let count = (rest[1] & 0x7f) as usize;
            if !(1..=4).contains(&count) || rest.len() < 2 + count {
                return Err("Invalid DER length".into());
            }
            size = rest[2..2 + count]
                .iter()
                .fold(0usize, |a, &b| (a << 8) | b as usize);
            offset = 2 + count;
        }
        let end = offset.checked_add(size).ok_or("Invalid DER length")?;
        if end > rest.len() {
            return Err("Truncated DER certificate".into());
        }
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    Ok(out)
}

fn key_type(cert: &X509Certificate) -> String {
    match cert
        .public_key()
        .algorithm
        .algorithm
        .to_id_string()
        .as_str()
    {
        "1.2.840.113549.1.1.1" => "RSA".into(),
        "1.2.840.10045.2.1" => "EC".into(),
        "1.2.840.10040.4.1" => "DSA".into(),
        other => format!("other({other})"),
    }
}

fn metadata(cert: &X509Certificate, at: i64) -> CertInfo {
    let nb = cert.validity().not_before.timestamp();
    let na = cert.validity().not_after.timestamp();
    CertInfo {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        serial: format!("0x{:x}", cert.serial),
        sha256: sis::hex(&Sha256::digest(cert.as_ref())),
        not_before: iso_utc(nb),
        not_after: iso_utc(na),
        validity: if at < nb {
            "not-yet-valid"
        } else if at > na {
            "expired"
        } else {
            "valid"
        }
        .into(),
        key_type: key_type(cert),
        self_issued: cert.subject().as_raw() == cert.issuer().as_raw(),
        extensions: cert
            .extensions()
            .iter()
            .map(|e| Extension {
                oid: e.oid.to_id_string(),
                critical: e.critical,
                interpreted: !matches!(
                    e.parsed_extension(),
                    ParsedExtension::UnsupportedExtension { .. }
                        | ParsedExtension::ParseError { .. }
                ),
            })
            .collect(),
    }
}

#[derive(PartialEq, Eq)]
enum Verdict {
    Valid,
    Invalid,
    Unsupported,
}

fn verify_with(
    alg: &'static dyn signature::VerificationAlgorithm,
    key: &[u8],
    msg: &[u8],
    sig: &[u8],
) -> Verdict {
    if UnparsedPublicKey::new(alg, key).verify(msg, sig).is_ok() {
        Verdict::Valid
    } else {
        Verdict::Invalid
    }
}

/// Подпись данных SIS (`msg`) ключом сертификата. OID алгоритма — из поля подписи SIS.
fn verify_sis_signature(cert: &X509Certificate, oid: &str, msg: &[u8], sig: &[u8]) -> Verdict {
    let key = cert.public_key().subject_public_key.data.as_ref();
    match oid {
        "1.2.840.113549.1.1.5" => verify_with(
            &signature::RSA_PKCS1_1024_8192_SHA1_FOR_LEGACY_USE_ONLY,
            key,
            msg,
            sig,
        ),
        "1.2.840.113549.1.1.11" => verify_with(
            &signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY,
            key,
            msg,
            sig,
        ),
        _ => Verdict::Unsupported,
    }
}

/// Подпись сертификата `child` ключом `issuer`.
fn verify_cert_link(child: &X509Certificate, issuer: &X509Certificate) -> Verdict {
    let key = issuer.public_key().subject_public_key.data.as_ref();
    let msg = child.tbs_certificate.as_ref();
    let sig = child.signature_value.data.as_ref();
    let alg: &'static dyn signature::VerificationAlgorithm =
        match child.signature_algorithm.algorithm.to_id_string().as_str() {
            "1.2.840.113549.1.1.5" => &signature::RSA_PKCS1_1024_8192_SHA1_FOR_LEGACY_USE_ONLY,
            "1.2.840.113549.1.1.11" => &signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY,
            "1.2.840.113549.1.1.12" => &signature::RSA_PKCS1_2048_8192_SHA384,
            "1.2.840.113549.1.1.13" => &signature::RSA_PKCS1_1024_8192_SHA512_FOR_LEGACY_USE_ONLY,
            "1.2.840.10045.4.3.2" => &signature::ECDSA_P256_SHA256_ASN1,
            "1.2.840.10045.4.3.3" => &signature::ECDSA_P384_SHA384_ASN1,
            _ => return Verdict::Unsupported,
        };
    verify_with(alg, key, msg, sig)
}

fn label(v: &Verdict) -> &'static str {
    match v {
        Verdict::Valid => "valid",
        Verdict::Invalid => "invalid",
        Verdict::Unsupported => "unsupported",
    }
}

/// Проверка всех цепочек подписей пакета. `at` — момент проверки дат (секунды Unix).
pub fn inspect_signatures(raw: &[u8], at: i64) -> Result<SignatureReport, SisError> {
    let body = sis::controller_body(raw)?;
    let mut cursor = Cursor::new(&body);
    let mut chains: Vec<ChainReport> = Vec::new();
    while !cursor.done() {
        let start = cursor.pos;
        let f = cursor.field(None)?;
        if f.ftype != sis::T_SIGNATURE_CHAIN {
            continue;
        }
        if chains.len() >= MAX_CHAINS {
            return Err(SisError("Too many SIS signature chains".into()));
        }
        chains.push(inspect_chain(f.payload, &body[..start], at));
    }
    let status = if chains.is_empty() {
        "unsigned"
    } else if chains.iter().any(|c| c.status == "invalid") {
        "invalid"
    } else if chains.iter().any(|c| c.status == "unsupported") {
        "unsupported"
    } else {
        "valid"
    };
    Ok(SignatureReport {
        chains,
        status: status.into(),
        trust: "unknown".into(),
        revocation: "not-checked".into(),
        phone_constraints: "not-verified".into(),
        at: iso_utc(at),
    })
}

fn inspect_chain(payload: &[u8], signed: &[u8], at: i64) -> ChainReport {
    let mut entry = ChainReport {
        status: "invalid".into(),
        certificates: Vec::new(),
        signatures: Vec::new(),
        phone_trust: "unknown".into(),
        root_signature: "issuer-missing".into(),
        chain_links: Vec::new(),
        error: None,
    };
    if let Err(e) = fill_chain(&mut entry, payload, signed, at) {
        entry.error = Some(e.chars().take(200).collect());
    }
    entry
}

fn fill_chain(
    entry: &mut ChainReport,
    payload: &[u8],
    signed: &[u8],
    at: i64,
) -> Result<(), String> {
    let e = |x: SisError| x.0;
    let mut sc = Cursor::new(payload);
    let (_, signatures) = sc.array(Some(sis::T_SIGNATURE)).map_err(e)?;
    let cp = sc.field(Some(sis::T_CERTIFICATE_CHAIN)).map_err(e)?.payload;
    let der = Cursor::new(cp).field(Some(sis::T_BLOB)).map_err(e)?.payload;
    let parts = split_der(der)?;
    let mut chain = Vec::new();
    for p in &parts {
        let (rest, c) =
            X509Certificate::from_der(p).map_err(|x| format!("invalid certificate: {x}"))?;
        if !rest.is_empty() {
            return Err("trailing data after certificate".into());
        }
        chain.push(c);
    }
    if chain.is_empty() || signatures.is_empty() || signatures.len() > 16 {
        return Err("Missing certificate/signature".into());
    }
    entry.certificates = chain.iter().map(|c| metadata(c, at)).collect();
    let signed_sha256 = sis::hex(&Sha256::digest(signed));
    let mut statuses: Vec<&'static str> = Vec::new();
    for s in signatures {
        let mut fields = Cursor::new(s);
        let alg = fields
            .field(Some(sis::T_SIGNATURE_ALGORITHM))
            .map_err(e)?
            .payload;
        let oid_raw = Cursor::new(alg)
            .field(Some(sis::T_STRING))
            .map_err(e)?
            .payload;
        let oid: String = String::from_utf16_lossy(
            &oid_raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        );
        let value = fields.field(Some(sis::T_BLOB)).map_err(e)?.payload;
        let v = verify_sis_signature(&chain[0], &oid, signed, value);
        statuses.push(label(&v));
        entry.signatures.push(SignatureInfo {
            legacy_sha1: oid == "1.2.840.113549.1.1.5" || oid == "1.2.840.10040.4.3",
            oid,
            status: label(&v).into(),
            signed_bytes: signed.len(),
            signed_sha256: signed_sha256.clone(),
        });
    }
    let last = chain.last().unwrap();
    if last.issuer().as_raw() == last.subject().as_raw() {
        let v = verify_cert_link(last, last);
        entry.root_signature = match v {
            Verdict::Valid => "valid-self-signature",
            Verdict::Invalid => "invalid",
            Verdict::Unsupported => "unsupported",
        }
        .into();
        if entry.root_signature == "invalid" {
            statuses.push("invalid");
        } else if entry.root_signature == "unsupported" {
            statuses.push("unsupported");
        }
    }
    for pair in chain.windows(2) {
        let (child, issuer) = (&pair[0], &pair[1]);
        let status = if child.issuer().as_raw() != issuer.subject().as_raw() {
            "invalid"
        } else {
            label(&verify_cert_link(child, issuer))
        };
        statuses.push(status);
        entry.chain_links.push(status.into());
    }
    entry.status = if statuses.contains(&"invalid") {
        "invalid"
    } else if statuses.contains(&"unsupported") {
        "unsupported"
    } else {
        "valid"
    }
    .into();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(iso_utc(1_762_732_800), "2025-11-10T00:00:00+00:00");
        assert_eq!(iso_utc(-1), "1969-12-31T23:59:59+00:00");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00+00:00");
    }

    #[test]
    fn der_splitting_rejects_garbage() {
        assert!(split_der(&[1, 2, 3]).is_err());
        assert!(split_der(&[0x30, 0x05, 1]).is_err());
        assert!(split_der(&[0x30, 0x85, 0, 0, 0, 0, 1]).is_err());
        assert!(split_der(&[]).unwrap().is_empty());
    }
}
