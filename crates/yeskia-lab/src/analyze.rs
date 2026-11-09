//! Анализ журнала кадров: обмены «запрос → ответ», повторы, сбросы шины, гипотеза о framing,
//! сравнение сессий. Результат — только **гипотезы** (`Inferred`): ни один вывод не означает,
//! что протокол «поддерживается» (это возможно лишь после успешного общения адаптера YESKIA).

use crate::fixture::{parse_hex, Dir, Fixture, Kind, Record};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Exchange {
    /// Метка действия UI, во время которой отправлен запрос.
    pub marker: Option<String>,
    pub request_t_ms: u64,
    pub request_ep: String,
    pub request_len: usize,
    /// Первые байты запроса (для классификации команд), без остального содержимого.
    pub request_head_hex: String,
    pub response_ep: Option<String>,
    pub response_len: Option<usize>,
    pub response_frames: u32,
    pub latency_ms: Option<u64>,
    /// Сколько раз запрос повторён без ответа.
    pub retries: u32,
    pub interrupted_by_reset: bool,
    #[serde(skip)]
    pub request: Vec<u8>,
    #[serde(skip)]
    pub response: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Framing {
    pub offset: usize,
    pub size: usize,
    pub endian: Endian,
    /// Поле считает всю нагрузку (`false`) или только часть после поля длины (`true`).
    pub excludes_length_field: bool,
    pub matched: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hypothesis {
    pub statement: String,
    /// Всегда `inferred`: гипотезы анализатора не подтверждают поддержку.
    pub evidence: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Analysis {
    pub frames: usize,
    pub markers: Vec<String>,
    pub exchanges: Vec<Exchange>,
    pub resets_t_ms: Vec<u64>,
    pub framing: Option<Framing>,
    pub hypotheses: Vec<Hypothesis>,
}

fn hyp(s: String) -> Hypothesis {
    Hypothesis {
        statement: s,
        evidence: "inferred",
    }
}

fn read_len(p: &[u8], offset: usize, size: usize, endian: Endian) -> Option<usize> {
    let b = p.get(offset..offset + size)?;
    let mut v = 0usize;
    match endian {
        Endian::Little => b.iter().rev().for_each(|x| v = (v << 8) | *x as usize),
        Endian::Big => b.iter().for_each(|x| v = (v << 8) | *x as usize),
    }
    Some(v)
}

fn detect_framing(payloads: &[&[u8]]) -> Option<Framing> {
    if payloads.len() < 3 {
        return None;
    }
    let mut best: Option<Framing> = None;
    for offset in 0..=2usize {
        for size in [1usize, 2, 4] {
            for endian in [Endian::Little, Endian::Big] {
                for excludes in [false, true] {
                    let matched = payloads
                        .iter()
                        .filter(|p| {
                            let Some(v) = read_len(p, offset, size, endian) else {
                                return false;
                            };
                            let expect = if excludes {
                                p.len().saturating_sub(offset + size)
                            } else {
                                p.len()
                            };
                            v == expect && v > 0
                        })
                        .count();
                    let cand = Framing {
                        offset,
                        size,
                        endian,
                        excludes_length_field: excludes,
                        matched,
                        total: payloads.len(),
                    };
                    if best.as_ref().map_or(true, |b| matched > b.matched) {
                        best = Some(cand);
                    }
                }
            }
        }
    }
    // Гипотеза принимается только при уверенном совпадении (≥80% кадров, не менее трёх).
    best.filter(|b| b.matched >= 3 && b.matched * 10 >= b.total * 8)
}

pub fn analyze(f: &Fixture) -> Analysis {
    let mut marker: Option<String> = None;
    let mut markers = Vec::new();
    let mut exchanges: Vec<Exchange> = Vec::new();
    let mut resets = Vec::new();
    let mut frames = 0usize;
    let mut bulk_payloads: Vec<Vec<u8>> = Vec::new();

    for r in &f.records {
        match r {
            Record::Marker { marker: m, .. } => {
                marker = Some(m.clone());
                markers.push(m.clone());
            }
            Record::Event { t_ms, event } => {
                if event == "reset" {
                    resets.push(*t_ms);
                    if let Some(open) = exchanges.last_mut().filter(|e| e.response_len.is_none()) {
                        open.interrupted_by_reset = true;
                    }
                }
            }
            Record::Frame {
                t_ms,
                dir,
                ep,
                kind,
                hex,
            } => {
                frames += 1;
                let data = parse_hex(hex).unwrap_or_default();
                if *kind == Kind::Bulk {
                    bulk_payloads.push(data.clone());
                }
                match dir {
                    Dir::Out => {
                        if let Some(open) = exchanges.last_mut().filter(|e| {
                            // обмен, прерванный сбросом шины, закрыт: повтор после сброса — новый обмен
                            e.response_len.is_none()
                                && !e.interrupted_by_reset
                                && e.request == data
                                && e.request_ep == *ep
                        }) {
                            open.retries += 1;
                        } else {
                            exchanges.push(Exchange {
                                marker: marker.clone(),
                                request_t_ms: *t_ms,
                                request_ep: ep.clone(),
                                request_len: data.len(),
                                request_head_hex: crate::fixture::to_hex(
                                    &data[..data.len().min(4)],
                                ),
                                response_ep: None,
                                response_len: None,
                                response_frames: 0,
                                latency_ms: None,
                                retries: 0,
                                interrupted_by_reset: false,
                                request: data,
                                response: Vec::new(),
                            });
                        }
                    }
                    Dir::In => {
                        if let Some(e) = exchanges.last_mut().filter(|e| !e.interrupted_by_reset) {
                            if e.response_ep.is_none() {
                                e.response_ep = Some(ep.clone());
                                e.latency_ms = Some(t_ms.saturating_sub(e.request_t_ms));
                            }
                            e.response_frames += 1;
                            e.response_len = Some(e.response_len.unwrap_or(0) + data.len());
                            e.response.extend_from_slice(&data);
                        }
                    }
                }
            }
        }
    }

    let refs: Vec<&[u8]> = bulk_payloads.iter().map(|p| p.as_slice()).collect();
    let framing = detect_framing(&refs);

    let mut hypotheses = Vec::new();
    if let Some(fr) = &framing {
        hypotheses.push(hyp(format!(
            "bulk-кадры, вероятно, начинаются с поля длины: смещение {}, {} байт, {}, {} (совпало {} из {} кадров)",
            fr.offset,
            fr.size,
            if fr.endian == Endian::Little { "little-endian" } else { "big-endian" },
            if fr.excludes_length_field { "без учёта самого поля" } else { "вся нагрузка" },
            fr.matched,
            fr.total
        )));
    }
    for (i, e) in exchanges.iter().enumerate() {
        if e.retries > 0 {
            hypotheses.push(hyp(format!(
                "обмен {}: запрос повторён {} раз без ответа (возможен тайм-аут/повтор на уровне протокола)",
                i + 1,
                e.retries
            )));
        }
        if e.interrupted_by_reset {
            hypotheses.push(hyp(format!(
                "обмен {}: прерван сбросом шины до ответа",
                i + 1
            )));
        }
        if e.response_len.is_none() && !e.interrupted_by_reset {
            hypotheses.push(hyp(format!("обмен {}: ответ не наблюдался", i + 1)));
        }
    }
    Analysis {
        frames,
        markers,
        exchanges,
        resets_t_ms: resets,
        framing,
        hypotheses,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DynamicField {
    pub exchange: usize,
    /// `request` или `response`.
    pub side: &'static str,
    pub offsets: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionDiff {
    pub aligned_exchanges: usize,
    pub dynamic: Vec<DynamicField>,
    pub hypotheses: Vec<Hypothesis>,
}

/// Сравнивает две сессии одного сценария: байты, отличающиеся на тех же позициях, — кандидаты в
/// динамические поля (счётчик/время/nonce/challenge). Это **не** доказательство и не способ обхода защиты.
pub fn diff_sessions(a: &Analysis, b: &Analysis) -> SessionDiff {
    let mut dynamic = Vec::new();
    let n = a.exchanges.len().min(b.exchanges.len());
    for i in 0..n {
        for (side, x, y) in [
            ("request", &a.exchanges[i].request, &b.exchanges[i].request),
            (
                "response",
                &a.exchanges[i].response,
                &b.exchanges[i].response,
            ),
        ] {
            if x.len() == y.len() && !x.is_empty() {
                let offsets: Vec<usize> = (0..x.len()).filter(|&k| x[k] != y[k]).collect();
                if !offsets.is_empty() {
                    dynamic.push(DynamicField {
                        exchange: i + 1,
                        side,
                        offsets,
                    });
                }
            }
        }
    }
    let hypotheses = dynamic
        .iter()
        .map(|d| {
            hyp(format!(
                "обмен {} ({}): байты на смещениях {:?} различаются между сессиями — возможное динамическое поле (счётчик, время, nonce); без дополнительных экспериментов это не подтверждено",
                d.exchange, d.side, d.offsets
            ))
        })
        .collect();
    SessionDiff {
        aligned_exchanges: n,
        dynamic,
        hypotheses,
    }
}
