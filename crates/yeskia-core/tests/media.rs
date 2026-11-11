//! Офлайн smoke-тесты подготовки медиа через настоящий FFmpeg (issue #9).
//! Запускаются явно через `--ignored`; `YESKIA_REQUIRE_FFMPEG=1` требует наличие инструмента.

use std::path::{Path, PathBuf};
use std::process::Command;
use yeskia_core::error::{Error, ErrorCode};
use yeskia_core::media::{Generated, RingtoneOptions, Toolchain};

fn toolchain() -> Option<Toolchain> {
    match Toolchain::locate(None) {
        Ok(t) => Some(t),
        Err(e) => {
            if std::env::var_os("YESKIA_REQUIRE_FFMPEG").is_some() {
                panic!("FFmpeg обязателен в этом окружении: {e}");
            }
            eprintln!("FFmpeg не найден — тест пропущен: {e}");
            None
        }
    }
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-media-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Создаёт тестовый файл самим FFmpeg (источники lavfi — без внешних данных).
fn make(t: &Toolchain, dir: &Path, name: &str, input: &[&str], out: &[&str]) -> String {
    let path = dir.join(name);
    let st = Command::new(&t.info.ffmpeg_path)
        .args(["-v", "error", "-y", "-f", "lavfi"])
        .args(input)
        .args(out)
        .arg(&path)
        .status()
        .unwrap();
    assert!(st.success(), "не удалось создать {name}");
    path.display().to_string()
}

fn ok() -> Result<(), Error> {
    Ok(())
}

fn opts(start: f64, end: f64) -> RingtoneOptions {
    RingtoneOptions {
        start,
        end,
        fade_in: 0.2,
        fade_out: 0.3,
        mono: true,
        normalize: false,
        lufs: -16.0,
        bitrate: 128,
        sample_rate: 44100,
    }
}

fn nostage(_: u64, _: u64) {}

#[test]
#[ignore = "requires external FFmpeg; run explicitly with --ignored"]
fn toolchain_reports_version_license_and_encoders() {
    let Some(t) = toolchain() else { return };
    assert!(!t.info.version.is_empty());
    assert!(
        ["lgpl", "lgpl-v3", "gpl", "gpl-v3", "nonfree"].contains(&t.info.license.as_str()),
        "{}",
        t.info.license
    );
    assert_eq!(t.info.redistributable, t.info.license != "nonfree");
    assert!(
        t.info.encoders.iter().any(|e| e == "libmp3lame"),
        "для рингтонов нужен libmp3lame"
    );
}

#[test]
#[ignore = "requires external FFmpeg; run explicitly with --ignored"]
fn probe_distinguishes_audio_video_image_and_rejects_junk() {
    let Some(t) = toolchain() else { return };
    let d = tmp("probe");
    let wav = make(
        &t,
        &d,
        "tone.wav",
        &["-i", "sine=frequency=440:duration=2"],
        &[],
    );
    let png = make(
        &t,
        &d,
        "pic.png",
        &["-i", "color=c=red:s=64x48"],
        &["-frames:v", "1"],
    );
    let a = t.probe(Path::new(&wav), &ok).unwrap();
    assert_eq!(a.kind, "audio");
    assert!((a.duration - 2.0).abs() < 0.1);
    assert!(a.audio.is_some() && a.video.is_none());
    let i = t.probe(Path::new(&png), &ok).unwrap();
    assert_eq!(i.kind, "image");
    assert_eq!(
        (
            i.video.as_ref().unwrap().width,
            i.video.as_ref().unwrap().height
        ),
        (Some(64), Some(48))
    );
    let junk = d.join("junk.mp3");
    std::fs::write(&junk, "это совсем не аудио".as_bytes()).unwrap();
    assert_eq!(
        t.probe(&junk, &ok).unwrap_err().code(),
        ErrorCode::InvalidArgument
    );
    // чужая папка, пустой файл и относительный путь отвергаются раньше FFmpeg
    assert!(yeskia_core::media::check_input(&d.display().to_string()).is_err());
    let empty = d.join("empty.wav");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(
        yeskia_core::media::check_input(&empty.display().to_string())
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
}

#[test]
#[ignore = "requires external FFmpeg; run explicitly with --ignored"]
fn waveform_shows_silence_then_sound() {
    let Some(t) = toolchain() else { return };
    let d = tmp("wave");
    // 2 с тишины, затем 2 с тона
    let wav = make(
        &t,
        &d,
        "w.wav",
        &[
            "-i",
            "aevalsrc='if(lt(t,2),0,0.8*sin(2*PI*440*t))':d=4:s=8000",
        ],
        &[],
    );
    let p = t.waveform(Path::new(&wav), 40, &ok).unwrap();
    assert_eq!(p.len(), 40);
    assert!(
        p[..15].iter().all(|&v| v < 0.01),
        "начало — тишина: {:?}",
        &p[..15]
    );
    assert!(
        p[25..].iter().all(|&v| v > 0.5),
        "конец — звук: {:?}",
        &p[25..]
    );
    assert!(t.waveform(Path::new(&wav), 4, &ok).is_err());
}

fn peak(t: &Toolchain, path: &str) -> f32 {
    t.waveform(Path::new(path), 50, &ok)
        .unwrap()
        .into_iter()
        .fold(0.0, f32::max)
}

#[test]
#[ignore = "requires external FFmpeg; run explicitly with --ignored"]
fn ringtone_trims_fades_normalizes_exports_and_reads_back() {
    let Some(t) = toolchain() else { return };
    let d = tmp("ring");
    // тихий тон 6 секунд (амплитуда 0.05)
    let src = make(
        &t,
        &d,
        "quiet.wav",
        &["-i", "sine=frequency=440:duration=6,volume=0.05"],
        &[],
    );
    let dest = d.join("ring.mp3").display().to_string();
    let stages = std::sync::Mutex::new(Vec::new());
    let g: Generated = t
        .ringtone(&src, &dest, &opts(1.0, 3.0), &ok, &|a, b| {
            stages.lock().unwrap().push((a, b))
        })
        .unwrap();
    assert_eq!(g.path, dest);
    assert!((g.duration - 2.0).abs() < 1e-9);
    assert_eq!(g.hardware, "not-tested");
    assert!(g.bytes > 1000 && g.sha256.len() == 64);
    let st = stages.lock().unwrap();
    assert_eq!(st.last(), Some(&(4, 4)));
    // read-back: файл на диске — MP3 ожидаемой длительности, хеш совпадает
    let info = t.probe(Path::new(&dest), &ok).unwrap();
    assert_eq!(info.kind, "audio");
    assert!((info.duration - 2.0).abs() < 0.3, "{}", info.duration);
    assert_eq!(
        info.container.as_deref().map(|c| c.contains("mp3")),
        Some(true)
    );
    let (bytes, sha) = yeskia_core::fsutil::hash_file(Path::new(&dest), None).unwrap();
    assert_eq!((bytes, sha), (g.bytes, g.sha256.clone()));
    // fade-out: конец тише середины
    let w = t.waveform(Path::new(&dest), 20, &ok).unwrap();
    assert!(w[19] < w[10], "затухание в конце: {w:?}");
    // временных файлов не осталось
    let left: Vec<_> = std::fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("yeskia-part"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    // повтор в тот же файл не перезаписывает
    let again = t.ringtone(&src, &dest, &opts(0.0, 1.0), &ok, &nostage);
    assert!(matches!(again, Err(Error::AlreadyExists)));
    assert_eq!(
        yeskia_core::fsutil::hash_file(Path::new(&dest), None)
            .unwrap()
            .1,
        g.sha256
    );

    // нормализация действительно поднимает тихий звук
    let plain = d.join("plain.mp3").display().to_string();
    let loud = d.join("loud.mp3").display().to_string();
    t.ringtone(
        &src,
        &plain,
        &RingtoneOptions {
            fade_in: 0.0,
            fade_out: 0.0,
            ..opts(0.0, 5.0)
        },
        &ok,
        &nostage,
    )
    .unwrap();
    t.ringtone(
        &src,
        &loud,
        &RingtoneOptions {
            fade_in: 0.0,
            fade_out: 0.0,
            normalize: true,
            ..opts(0.0, 5.0)
        },
        &ok,
        &nostage,
    )
    .unwrap();
    let (pp, pl) = (peak(&t, &plain), peak(&t, &loud));
    assert!(pl > pp * 3.0, "нормализация: {pp} → {pl}");
    // исходник не изменился
    assert!(Path::new(&src).exists());
}

#[test]
#[ignore = "requires external FFmpeg; run explicitly with --ignored"]
fn ringtone_errors_are_clear_and_leave_nothing_behind() {
    let Some(t) = toolchain() else { return };
    let d = tmp("ring-err");
    let src = make(
        &t,
        &d,
        "s.wav",
        &["-i", "sine=frequency=440:duration=3"],
        &[],
    );
    let dest = d.join("x.mp3").display().to_string();
    // фрагмент вне дорожки / слишком короткий
    assert_eq!(
        t.ringtone(&src, &dest, &opts(0.0, 30.0), &ok, &nostage)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        t.ringtone(&src, &dest, &opts(1.0, 1.05), &ok, &nostage)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidArgument
    );
    // чужое расширение результата и несуществующая папка
    assert!(t
        .ringtone(
            &src,
            &d.join("x.wav").display().to_string(),
            &opts(0.0, 1.0),
            &ok,
            &nostage
        )
        .is_err());
    assert!(t
        .ringtone(
            &src,
            &d.join("нет").join("x.mp3").display().to_string(),
            &opts(0.0, 1.0),
            &ok,
            &nostage
        )
        .is_err());
    // не аудио
    let txt = d.join("t.mp3");
    std::fs::write(&txt, b"text").unwrap();
    assert!(t
        .ringtone(
            &txt.display().to_string(),
            &dest,
            &opts(0.0, 1.0),
            &ok,
            &nostage
        )
        .is_err());
    // видео без звука
    let silent = make(
        &t,
        &d,
        "v.mp4",
        &["-i", "testsrc=size=64x48:rate=5:duration=2"],
        &["-pix_fmt", "yuv420p"],
    );
    assert!(t
        .ringtone(&silent, &dest, &opts(0.0, 1.0), &ok, &nostage)
        .is_err());
    // отмена: к моменту запуска кодирования проверка возвращает Cancelled
    let calls = std::sync::atomic::AtomicU32::new(0);
    let cancel = || {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 3 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    };
    let r = t.ringtone(&src, &dest, &opts(0.0, 2.0), &cancel, &nostage);
    assert!(matches!(r, Err(Error::Cancelled)), "{:?}", r.err());
    assert!(!Path::new(&dest).exists(), "итогового файла нет");
    let names: Vec<_> = std::fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| !n.contains("yeskia-part")),
        "{names:?}"
    );
}

fn dims(t: &Toolchain, path: &str) -> (u32, u32) {
    let i = t.probe(Path::new(path), &ok).unwrap();
    let v = i.video.unwrap();
    (v.width.unwrap(), v.height.unwrap())
}

#[test]
#[ignore = "requires external FFmpeg; run explicitly with --ignored"]
fn image_presets_follow_each_models_screen_geometry() {
    let Some(t) = toolchain() else { return };
    let d = tmp("img");
    let src = make(
        &t,
        &d,
        "wide.png",
        &["-i", "testsrc=size=1000x500:rate=1"],
        &["-frames:v", "1"],
    );
    for (model, (w, h)) in [
        ("nokia-e52", (240, 320)),
        ("nokia-e72", (320, 240)),
        ("nokia-7610", (176, 208)),
    ] {
        let cover = d.join(format!("{model}-cover.jpg")).display().to_string();
        let g = t
            .prepare(&src, &cover, "wallpaper-jpeg", model, &ok, &nostage)
            .unwrap();
        assert_eq!(dims(&t, &cover), (w, h), "{model}: обои заполняют экран");
        assert_eq!(g.hardware, "not-tested");
        assert!(
            g.description.contains("по документации"),
            "заявлено как документированное, не измеренное"
        );
        let fit = d.join(format!("{model}-fit.png")).display().to_string();
        t.prepare(&src, &fit, "photo-fit-png", model, &ok, &nostage)
            .unwrap();
        let (fw, fh) = dims(&t, &fit);
        assert!(
            fw <= w && fh <= h && (fw == w || fh == h),
            "{model}: вписано {fw}x{fh} в {w}x{h}"
        );
        assert_eq!(
            fw * 500,
            fh * 1000,
            "{model}: пропорции сохранены ({fw}x{fh})"
        );
    }
    // модель обязательна для картинок; неизвестный пресет отвергается
    let dest = d.join("z.jpg").display().to_string();
    assert!(t
        .prepare(&src, &dest, "wallpaper-jpeg", "", &ok, &nostage)
        .is_err());
    assert!(t
        .prepare(
            &src,
            &dest,
            "wallpaper-jpeg",
            "nokia-unknown",
            &ok,
            &nostage
        )
        .is_err());
    assert!(t
        .prepare(&src, &dest, "rm-rf", "nokia-e52", &ok, &nostage)
        .is_err());
    // пресет не того вида: аудио-пресет для картинки
    assert!(t
        .prepare(
            &src,
            &d.join("a.mp3").display().to_string(),
            "mp3-128",
            "nokia-e52",
            &ok,
            &nostage
        )
        .is_err());
}

#[test]
#[ignore = "requires external FFmpeg; run explicitly with --ignored"]
fn audio_and_video_presets_produce_readable_files() {
    let Some(t) = toolchain() else { return };
    let d = tmp("av");
    let wav = make(
        &t,
        &d,
        "a.wav",
        &["-i", "sine=frequency=330:duration=2"],
        &[],
    );
    let out = d.join("a128.mp3").display().to_string();
    let g = t.prepare(&wav, &out, "mp3-128", "", &ok, &nostage).unwrap();
    let info = t.probe(Path::new(&out), &ok).unwrap();
    assert_eq!(info.audio.as_ref().unwrap().channels, Some(2));
    assert_eq!(info.audio.as_ref().unwrap().sample_rate, Some(44100));
    assert_eq!(
        g.sha256,
        yeskia_core::fsutil::hash_file(Path::new(&out), None)
            .unwrap()
            .1
    );
    let mono = d.join("a64.mp3").display().to_string();
    t.prepare(&wav, &mono, "mp3-64-mono", "", &ok, &nostage)
        .unwrap();
    assert_eq!(
        t.probe(Path::new(&mono), &ok)
            .unwrap()
            .audio
            .unwrap()
            .channels,
        Some(1)
    );

    let mp4 = make(
        &t,
        &d,
        "v.mp4",
        &["-i", "testsrc=size=640x360:rate=10:duration=2"],
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:duration=2",
            "-pix_fmt",
            "yuv420p",
            "-shortest",
        ],
    );
    let dest = d.join("out.mp4").display().to_string();
    match t.prepare(&mp4, &dest, "mp4-qvga-h264", "", &ok, &nostage) {
        Ok(_) => assert_eq!(dims(&t, &dest), (320, 240)),
        // сборка без libx264: понятная ошибка, а не заглушка
        Err(Error::ToolMissing(m)) => {
            assert!(!t.info.encoders.iter().any(|e| e == "libx264"));
            assert!(m.contains("libx264"));
        }
        Err(e) => panic!("{e}"),
    }
    let gp = d.join("out.3gp").display().to_string();
    t.prepare(&mp4, &gp, "3gp-qcif-mpeg4", "", &ok, &nostage)
        .unwrap();
    assert_eq!(dims(&t, &gp), (176, 144));
}
