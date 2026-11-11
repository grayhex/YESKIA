//! Versioned, opt-in helper protocol; never send these command names to upstream ADBian.
#![no_std]
extern crate alloc;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use serde::{Deserialize, Serialize};
pub const PROTOCOL: u16 = 1;
pub const MAX_DOCUMENT: usize = 32 * 1024;
pub const MAX_PAGE: u16 = 32;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Hello,
    Capabilities,
    AppsList { offset: u32, limit: u16 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub request_id: String,
    pub query: Query,
}
impl Request {
    pub fn valid(&self) -> bool {
        self.protocol == PROTOCOL
            && self.request_id.len() == 32
            && self.request_id.bytes().all(|b| b.is_ascii_hexdigit())
            && match self.query {
                Query::AppsList { offset, limit } => {
                    offset <= 4096 && limit > 0 && limit <= MAX_PAGE
                }
                _ => true,
            }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppRecord {
    pub uid: u32,
    pub caption: String,
    pub caption_truncated: bool,
}
impl core::fmt::Debug for AppRecord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AppRecord([private])")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Data {
    Capabilities {
        services: Vec<String>,
        contacts: bool,
        calendar: bool,
        sms: bool,
    },
    Apps {
        entries: Vec<AppRecord>,
        next_offset: Option<u32>,
    },
    Error {
        code: i32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub protocol: u16,
    pub request_id: String,
    pub agent_version: String,
    pub build: String,
    pub heap_before_kib: Option<u32>,
    pub heap_after_kib: Option<u32>,
    pub data: Data,
}
pub trait SystemApi {
    fn apps(
        &mut self,
        offset: u32,
        limit: u16,
    ) -> core::result::Result<(Vec<AppRecord>, Option<u32>), i32>;
    fn heap_kib(&self) -> Option<u32> {
        None
    }
}
pub fn handle(request: &Request, api: &mut dyn SystemApi, build: &str) -> Response {
    let before = api.heap_kib();
    let data = if !request.valid() {
        Data::Error { code: -6 }
    } else {
        match request.query {
            Query::Hello | Query::Capabilities => Data::Capabilities {
                services: alloc::vec!["apps.registry.read".to_string()],
                contacts: false,
                calendar: false,
                sms: false,
            },
            Query::AppsList { offset, limit } => match api.apps(offset, limit) {
                Ok((entries, next_offset))
                    if entries.len() <= limit as usize
                        && entries.iter().all(|e| e.caption.chars().count() <= 64)
                        && next_offset.map_or(true, |n| n > offset && n <= 4096) =>
                {
                    Data::Apps {
                        entries,
                        next_offset,
                    }
                }
                Ok(_) => Data::Error { code: -6 },
                Err(code) => Data::Error { code },
            },
        }
    };
    Response {
        protocol: PROTOCOL,
        request_id: request.request_id.clone(),
        agent_version: VERSION.to_string(),
        build: build.to_string(),
        heap_before_kib: before,
        heap_after_kib: api.heap_kib(),
        data,
    }
}
pub fn decode_request(bytes: &[u8]) -> core::result::Result<Request, &'static str> {
    if bytes.len() > MAX_DOCUMENT {
        return Err("document_limit");
    }
    let req: Request = serde_json::from_slice(bytes).map_err(|_| "invalid_request")?;
    if !req.valid() {
        return Err("invalid_request");
    }
    Ok(req)
}
pub fn decode_response(
    bytes: &[u8],
    expected_id: &str,
    query: &Query,
) -> core::result::Result<Response, &'static str> {
    if bytes.len() > MAX_DOCUMENT {
        return Err("document_limit");
    }
    let response: Response = serde_json::from_slice(bytes).map_err(|_| "invalid_response")?;
    if response.protocol != PROTOCOL
        || response.request_id != expected_id
        || response.agent_version.len() > 64
        || response.build.is_empty()
        || response.build.len() > 80
    {
        return Err("version_or_request_mismatch");
    }
    match (&response.data, query) {
        (
            Data::Capabilities {
                services,
                contacts,
                calendar,
                sms,
            },
            Query::Hello | Query::Capabilities,
        ) if services == &alloc::vec!["apps.registry.read".to_string()]
            && !contacts
            && !calendar
            && !sms => {}
        (
            Data::Apps {
                entries,
                next_offset,
            },
            Query::AppsList { offset, limit },
        ) if entries.len() <= *limit as usize
            && entries.iter().all(|e| e.caption.chars().count() <= 64)
            && next_offset.map_or(true, |n| n > *offset && n <= 4096) => {}
        (Data::Error { .. }, _) => {}
        _ => return Err("unexpected_service"),
    }
    Ok(response)
}
pub fn encode_response(response: &Response) -> core::result::Result<Vec<u8>, &'static str> {
    let data = serde_json::to_vec(response).map_err(|_| "encode")?;
    if data.len() > MAX_DOCUMENT {
        return Err("document_limit");
    }
    Ok(data)
}
#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    struct Fake;
    impl SystemApi for Fake {
        fn apps(
            &mut self,
            offset: u32,
            limit: u16,
        ) -> core::result::Result<(Vec<AppRecord>, Option<u32>), i32> {
            Ok((
                (offset..offset + u32::from(limit))
                    .map(|uid| AppRecord {
                        uid,
                        caption: "Synthetic application 🧪".to_string(),
                        caption_truncated: false,
                    })
                    .collect(),
                Some(offset + u32::from(limit)),
            ))
        }
        fn heap_kib(&self) -> Option<u32> {
            Some(128)
        }
    }
    fn req(query: Query) -> Request {
        Request {
            protocol: 1,
            request_id: "0123456789abcdef0123456789abcdef".into(),
            query,
        }
    }
    #[test]
    fn fake_phone_pages_are_bounded_and_roundtrip() {
        let req = req(Query::AppsList {
            offset: 0,
            limit: MAX_PAGE,
        });
        let r = handle(&req, &mut Fake, "synthetic-build");
        let bytes = encode_response(&r).unwrap();
        assert!(bytes.len() < MAX_DOCUMENT);
        assert_eq!(
            decode_response(&bytes, &req.request_id, &req.query).unwrap(),
            r
        );
        assert!(decode_response(&bytes, "different-id", &req.query).is_err());
        assert!(decode_response(&bytes, &req.request_id, &Query::Hello).is_err());
    }
    #[test]
    fn pim_is_not_advertised_and_wrong_versions_fail() {
        let mut req = req(Query::Capabilities);
        let r = handle(&req, &mut Fake, "test");
        assert!(matches!(
            r.data,
            Data::Capabilities {
                contacts: false,
                calendar: false,
                sms: false,
                ..
            }
        ));
        req.protocol = 2;
        assert!(matches!(
            handle(&req, &mut Fake, "test").data,
            Data::Error { .. }
        ));
        assert!(decode_request(&serde_json::to_vec(&req).unwrap()).is_err());
    }
    #[test]
    fn host_benchmark_serializes_a_maximum_caption_page() {
        let request = req(Query::AppsList {
            offset: 0,
            limit: 32,
        });
        let mut r = handle(&request, &mut Fake, "test");
        if let Data::Apps { entries, .. } = &mut r.data {
            for e in entries {
                e.caption = "\u{0001}".repeat(64);
            }
        }
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let bytes = encode_response(&r).unwrap();
            assert!(bytes.len() < MAX_DOCUMENT);
        }
        std::println!(
            "100 bounded page encodes: {:?}; E72 heap/time must be measured separately",
            start.elapsed()
        );
    }
    #[test]
    fn rejects_zero_page_oversize_and_caption_data_in_debug() {
        assert!(!req(Query::AppsList {
            offset: 0,
            limit: 0
        })
        .valid());
        assert!(!req(Query::AppsList {
            offset: 0,
            limit: 33
        })
        .valid());
        assert!(decode_request(&alloc::vec![b' ';MAX_DOCUMENT+1]).is_err());
        let e = AppRecord {
            uid: 1,
            caption: "private title".into(),
            caption_truncated: false,
        };
        assert!(!alloc::format!("{e:?}").contains("private title"));
    }
}
