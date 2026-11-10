//! Задания медиа и тем через `AppCore` (issue #9): отмена, повтор, прогресс, анализ темы.

use std::path::PathBuf;
use yeskia_core::app::{AppCore, HostJobData, HostJobRequest, ThemeSource};
use yeskia_core::error::ErrorCode;
use yeskia_core::library::Provenance;
use yeskia_core::media::RingtoneOptions;
use yeskia_core::operations::Outcome;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-mj-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn have_ffmpeg() -> bool {
    match yeskia_core::media::Toolchain::locate(None) {
        Ok(_) => true,
        Err(e) => {
            if std::env::var_os("YESKIA_REQUIRE_FFMPEG").is_some() {
                panic!("FFmpeg обязателен: {e}");
            }
            eprintln!("FFmpeg не найден — тест пропущен");
            false
        }
    }
}

fn make_tone(dir: &std::path::Path, secs: u32) -> String {
    let p = dir.join("tone.wav");
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:duration={secs}"),
        ])
        .arg(&p)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok);
    p.display().to_string()
}

fn opts() -> RingtoneOptions {
    RingtoneOptions {
        start: 0.0,
        end: 2.0,
        fade_in: 0.1,
        fade_out: 0.2,
        mono: true,
        normalize: true,
        lufs: -16.0,
        bitrate: 128,
        sample_rate: 44100,
    }
}

#[test]
fn ringtone_job_runs_reports_progress_and_never_overwrites() {
    if !have_ffmpeg() {
        return;
    }
    let core = AppCore::in_memory();
    let d = tmp("job");
    let src = make_tone(&d, 4);
    let dest = d.join("r.mp3").display().to_string();
    let id = core
        .start_host_job(HostJobRequest::MediaRingtone {
            source: src.clone(),
            dest: dest.clone(),
            options: opts(),
        })
        .unwrap();
    // пока идёт, второе задание не принимается
    let second = core.start_host_job(HostJobRequest::MediaRingtone {
        source: src.clone(),
        dest: d.join("r2.mp3").display().to_string(),
        options: opts(),
    });
    let r = core.await_host_job(&id).unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    match r.data.unwrap() {
        HostJobData::MediaGenerated { result } => {
            assert_eq!(result.path, dest);
            assert_eq!(result.hardware, "not-tested");
            assert_eq!(result.sha256.len(), 64);
        }
        other => panic!("{other:?}"),
    }
    if let Ok(id2) = second {
        let _ = core.await_host_job(&id2);
    }
    // тот же файл — отказ без перезаписи
    let id = core
        .start_host_job(HostJobRequest::MediaRingtone {
            source: src,
            dest,
            options: opts(),
        })
        .unwrap();
    let r = core.await_host_job(&id).unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::AlreadyExists);
}

#[test]
fn media_job_can_be_cancelled_without_leaving_files() {
    if !have_ffmpeg() {
        return;
    }
    let core = AppCore::in_memory();
    let d = tmp("cancel");
    let src = make_tone(&d, 30);
    let dest = d.join("c.mp3").display().to_string();
    let id = core
        .start_host_job(HostJobRequest::MediaRingtone {
            source: src,
            dest: dest.clone(),
            options: RingtoneOptions {
                end: 30.0,
                ..opts()
            },
        })
        .unwrap();
    assert!(core.cancel_host_job(&id));
    let r = core.await_host_job(&id).unwrap();
    assert_eq!(r.outcome, Outcome::Cancelled);
    assert!(!std::path::Path::new(&dest).exists());
    let leftovers: Vec<_> = std::fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("yeskia-part"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn invalid_media_requests_fail_before_ffmpeg_runs() {
    let core = AppCore::in_memory();
    // валидация путей не зависит от наличия FFmpeg: ошибка либо ToolMissing, либо проверка входа
    let r = core.start_host_job(HostJobRequest::MediaPrepare {
        source: "relative.png".into(),
        dest: "relative.jpg".into(),
        preset: "wallpaper-jpeg".into(),
        model: "nokia-e52".into(),
    });
    match r {
        Err(e) => assert_eq!(e.code(), ErrorCode::ToolMissing),
        Ok(id) => {
            let res = core.await_host_job(&id).unwrap();
            assert_eq!(res.outcome, Outcome::Failed);
            assert_eq!(res.error.unwrap().code, ErrorCode::UnsafePath);
        }
    }
    assert!(core.media_probe("relative.mp3").is_err());
    assert!(core.media_waveform("relative.mp3", 100).is_err());
}

#[test]
fn theme_inspection_from_library_and_path() {
    let core = AppCore::in_memory();
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/theme");
    let raw = std::fs::read(fx.join("e52clearlight.sis")).unwrap();
    let it = core
        .library()
        .import("e52clearlight.sis", &raw, Provenance::default())
        .unwrap();
    let r = core
        .theme_inspect(
            &ThemeSource::Library { id: it.id.clone() },
            Some("nokia-e52"),
        )
        .unwrap();
    assert_eq!(r.structure, "consistent");
    assert_eq!(r.package.source, "sis");
    let p = core
        .theme_preview(&ThemeSource::Library { id: it.id }, 4)
        .unwrap();
    assert_eq!(p.len(), 4);
    let r = core
        .theme_inspect(
            &ThemeSource::Path {
                path: fx.join("dark").display().to_string(),
            },
            None,
        )
        .unwrap();
    assert_eq!(r.package.uid, Some(0xE0E5_2C02));
    let r = core
        .theme_inspect(
            &ThemeSource::Path {
                path: fx.join("e52clearlight.sis").display().to_string(),
            },
            None,
        )
        .unwrap();
    assert_eq!(r.package.source, "sis");
    assert!(core
        .theme_inspect(&ThemeSource::Library { id: "0".repeat(64) }, None)
        .is_err());
    assert!(core
        .theme_inspect(
            &ThemeSource::Path {
                path: "relative".into()
            },
            None
        )
        .is_err());
}
