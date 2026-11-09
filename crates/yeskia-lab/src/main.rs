use std::path::{Path, PathBuf};
use std::process::ExitCode;
use yeskia_lab::{analyze, fixture, inventory, redact, report, scan};

const USAGE: &str = "yeskia-lab <команда>

  scan [корень]                          проверка репозитория: сырые трассы, приватные данные, отчёты, фикстуры
  validate <report.json>...              проверка отчётов эксперимента (формат v1)
  redact-text <вход> [выход]             обезличивание текста (IMEI, серийники, MAC, пути, e-mail…)
  redact-fixture <вход> <выход> [--keep N]  маскирование нагрузки кадров с сохранением длины и первых N байт
  analyze <fixture.jsonl>                обмены, повторы, сбросы, гипотеза framing (JSON)
  diff <a.jsonl> <b.jsonl>               динамические поля между двумя сессиями (JSON)
  inventory [--out файл]                 инвентаризация стенда + read-only попытка идентификации → заготовка отчёта";

fn read(p: &str) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))
}

fn today() -> String {
    // Дата UTC без внешних зависимостей (алгоритм civil-from-days).
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    match cmd {
        "scan" => {
            let root = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("."));
            let mut problems = scan::scan_forbidden_files(&root);
            problems.extend(scan::scan_protocol_dir(
                &root,
                &root.join("docs").join("protocol"),
            ));
            for p in &problems {
                eprintln!("{}: {}", p.file, p.detail);
            }
            if problems.is_empty() {
                println!("scan: нарушений нет");
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("scan: найдено нарушений: {}", problems.len());
                Ok(ExitCode::from(1))
            }
        }
        "validate" => {
            let mut bad = 0;
            for p in &args[1..] {
                match report::parse(&read(p)?) {
                    Ok(r) => {
                        let v = report::validate(&r);
                        if v.is_empty() {
                            println!("{p}: ok");
                        } else {
                            bad += 1;
                            for x in v {
                                eprintln!("{p}: {}: {}", x.path, x.rule);
                            }
                        }
                    }
                    Err(e) => {
                        bad += 1;
                        eprintln!("{p}: {e}");
                    }
                }
            }
            Ok(if bad == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        "redact-text" => {
            let (out, f) = redact::redact_text(&read(args.get(1).ok_or(USAGE)?)?);
            match args.get(2) {
                Some(o) => std::fs::write(o, out).map_err(|e| e.to_string())?,
                None => print!("{out}"),
            }
            eprintln!("заменено фрагментов: {}", f.len());
            Ok(ExitCode::SUCCESS)
        }
        "redact-fixture" => {
            let mut fx = fixture::parse(&read(args.get(1).ok_or(USAGE)?)?)?;
            let keep = args
                .iter()
                .position(|a| a == "--keep")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse().ok())
                .unwrap_or(4usize);
            for r in &mut fx.records {
                if let fixture::Record::Frame { hex, .. } = r {
                    let data = fixture::parse_hex(hex)?;
                    *hex = fixture::to_hex(&redact::mask_payload(&data, keep));
                }
                if let fixture::Record::Marker { marker, .. } = r {
                    *marker = redact::redact_text(marker).0;
                }
            }
            fx.header.payload_masked = true;
            std::fs::write(args.get(2).ok_or(USAGE)?, fixture::serialize(&fx))
                .map_err(|e| e.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
        "analyze" => {
            let a = analyze::analyze(&fixture::parse(&read(args.get(1).ok_or(USAGE)?)?)?);
            println!(
                "{}",
                serde_json::to_string_pretty(&a).map_err(|e| e.to_string())?
            );
            Ok(ExitCode::SUCCESS)
        }
        "diff" => {
            let a = analyze::analyze(&fixture::parse(&read(args.get(1).ok_or(USAGE)?)?)?);
            let b = analyze::analyze(&fixture::parse(&read(args.get(2).ok_or(USAGE)?)?)?);
            println!(
                "{}",
                serde_json::to_string_pretty(&analyze::diff_sessions(&a, &b))
                    .map_err(|e| e.to_string())?
            );
            Ok(ExitCode::SUCCESS)
        }
        "inventory" => {
            let r = inventory::collect_now(&today());
            let json = serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?;
            match args
                .iter()
                .position(|a| a == "--out")
                .and_then(|i| args.get(i + 1))
            {
                Some(p) => {
                    std::fs::write(Path::new(p), &json).map_err(|e| e.to_string())?;
                    eprintln!("заготовка отчёта записана; заполните поля TODO и проверьте: yeskia-lab validate {p}");
                }
                None => println!("{json}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ошибка: {e}");
            ExitCode::from(2)
        }
    }
}
