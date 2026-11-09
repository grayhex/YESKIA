//! Проверка репозитория на утечки и соблюдение правил лаборатории (для CI и pre-commit).

use crate::fixture;
use crate::redact::scan_text;
use crate::report;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub file: String,
    pub detail: String,
}

/// Расширения сырых захватов и трасс: в репозитории им не место.
const FORBIDDEN_EXT: &[&str] = &["pcap", "pcapng", "etl", "cap", "usbmon", "btsnoop", "snoop"];
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "dist", "gen"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(&p, out);
            }
        } else {
            out.push(p);
        }
    }
}

/// Сырые трассы где угодно в `root`.
pub fn scan_forbidden_files(root: &Path) -> Vec<Problem> {
    let mut files = Vec::new();
    walk(root, &mut files);
    files
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| FORBIDDEN_EXT.contains(&e.to_ascii_lowercase().as_str()))
        })
        .map(|p| Problem {
            file: rel(root, &p),
            detail: "сырой захват/трасса не должен попадать в репозиторий".into(),
        })
        .collect()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// Строгая проверка каталога протокольных материалов: приватные данные, отчёты, фикстуры.
pub fn scan_protocol_dir(root: &Path, dir: &Path) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut files = Vec::new();
    walk(dir, &mut files);
    for p in files {
        let name = rel(root, &p);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        for f in scan_text(&text) {
            problems.push(Problem {
                file: name.clone(),
                detail: format!("приватные данные ({:?}) на {}:{}", f.kind, f.line, f.col),
            });
        }
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if name.ends_with(".report.json") || name.contains("/cases/") && ext == "json" {
            match report::parse(&text) {
                Ok(r) => {
                    for v in report::validate(&r) {
                        problems.push(Problem {
                            file: name.clone(),
                            detail: format!("{}: {}", v.path, v.rule),
                        });
                    }
                }
                Err(e) => problems.push(Problem {
                    file: name.clone(),
                    detail: format!("не отчёт v1: {e}"),
                }),
            }
        }
        if ext == "jsonl" {
            match fixture::parse(&text) {
                Ok(fx) => {
                    if !fx.header.synthetic {
                        problems.push(Problem {
                            file: name.clone(),
                            detail: "в репозитории допустимы только синтетические фикстуры (synthetic: true)".into(),
                        });
                    }
                }
                Err(e) => problems.push(Problem {
                    file: name.clone(),
                    detail: format!("не фикстура v1: {e}"),
                }),
            }
        }
    }
    problems
}
