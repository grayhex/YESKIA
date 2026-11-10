//! Локальная подготовка медиа через FFmpeg (внешний процесс, не библиотека): проверка файла,
//! волна, рингтон (обрезка, затухание, моно, нормализация), пресеты под экран модели.
//!
//! Принципы:
//! * FFmpeg/ffprobe — **внешний инструмент** пользователя или установщика (лицензия определяется
//!   по `-version` и показывается); без них возвращается `ToolMissing`, а не заглушка;
//! * процесс запускается без оболочки, с проверенными числами/перечислениями вместо строк
//!   пользователя; разрешены только `file`/`pipe` и перечисленные демультиплексоры, сети нет;
//! * лимиты: вход 64 МиБ / 10 минут, выход 64 МиБ, 40 Мпикс, таймауты, отмена между опросами;
//! * результат пишется во временный файл и размещается **без перезаписи**, затем перечитывается
//!   (ffprobe + SHA-256). Пресеты — теоретические по экрану модели: `hardware: not-tested`;
//! * состояния `generated` / `transferred` / `installed` / `verified on phone` различаются:
//!   этот модуль отвечает только за `generated`.

use crate::error::{Error, Result};
use crate::fsutil::{hash_file, place_file, temp_path_for, validate_host_path};
use crate::operations::OverwritePolicy;
use crate::pkg::profile;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_INPUT: u64 = 64 * 1024 * 1024;
pub const MAX_OUTPUT: u64 = 64 * 1024 * 1024;
pub const MAX_DURATION: f64 = 600.0;
pub const MAX_PIXELS: u64 = 40_000_000;
/// Форматы, которые разрешено открывать (остальные демультиплексоры FFmpeg отключены).
const FORMATS: &str = "mp3,wav,aac,mov,mp4,m4a,3gp,3g2,mj2,flac,ogg,matroska,webm,asf,aiff,avi,mpegts,flv,image2,png_pipe,jpeg_pipe,bmp_pipe,gif,webp_pipe";
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const AUDIO_TIMEOUT: Duration = Duration::from_secs(80);
const VIDEO_TIMEOUT: Duration = Duration::from_secs(240);
const LOG_CAP: usize = 1024 * 1024;
const WAVE_RATE: u32 = 8000;

fn invalid<T>(m: impl Into<String>) -> Result<T> {
    Err(Error::InvalidArgument(m.into()))
}

// ---------- инструмент ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolInfo {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub version: String,
    /// `lgpl` | `lgpl-v3` | `gpl` | `gpl-v3` | `nonfree` | `unknown` — по параметрам сборки.
    pub license: String,
    /// Сборка с `--enable-nonfree` не распространяется: только для личного использования.
    pub redistributable: bool,
    pub encoders: Vec<String>,
}

pub struct Toolchain {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    pub info: ToolInfo,
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn find_in(dir: &Path, base: &str) -> Option<PathBuf> {
    let p = dir.join(exe_name(base));
    p.is_file().then_some(p)
}

fn license_of(config: &str) -> (&'static str, bool) {
    let has = |f: &str| config.split_whitespace().any(|t| t == f);
    if has("--enable-nonfree") {
        ("nonfree", false)
    } else if has("--enable-gpl") {
        (
            if has("--enable-version3") {
                "gpl-v3"
            } else {
                "gpl"
            },
            true,
        )
    } else if config.contains("--") || !config.is_empty() {
        (
            if has("--enable-version3") {
                "lgpl-v3"
            } else {
                "lgpl"
            },
            true,
        )
    } else {
        ("unknown", false)
    }
}

impl Toolchain {
    /// Порядок поиска: настроенная папка → `tools/ffmpeg` и папка рядом с приложением → `PATH`.
    pub fn locate(configured: Option<&Path>) -> Result<Toolchain> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(c) = configured {
            dirs.push(c.to_path_buf());
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(d) = exe.parent() {
                dirs.push(d.join("tools").join("ffmpeg"));
                dirs.push(d.join("ffmpeg"));
            }
        }
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        Self::locate_in(dirs)
    }

    /// Поиск в заданных папках (без `PATH`): основа `locate` и тестов.
    pub fn locate_in(dirs: Vec<PathBuf>) -> Result<Toolchain> {
        for d in dirs {
            if let (Some(ffmpeg), Some(ffprobe)) = (find_in(&d, "ffmpeg"), find_in(&d, "ffprobe")) {
                return Self::from_paths(ffmpeg, ffprobe);
            }
        }
        Err(Error::ToolMissing(
            "FFmpeg не найден. Укажите папку с ffmpeg и ffprobe в настройках или положите их в папку tools\\ffmpeg рядом с приложением."
                .into(),
        ))
    }

    fn from_paths(ffmpeg: PathBuf, ffprobe: PathBuf) -> Result<Toolchain> {
        let out = capture(
            &ffmpeg,
            &["-hide_banner", "-version"],
            PROBE_TIMEOUT,
            &|| Ok(()),
        )?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let version = text
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("ffmpeg version "))
            .and_then(|l| l.split_whitespace().next())
            .ok_or_else(|| Error::ToolMissing("файл ffmpeg не отвечает как FFmpeg".into()))?
            .to_string();
        let config = text
            .lines()
            .find_map(|l| l.strip_prefix("configuration:"))
            .unwrap_or("");
        let (license, redistributable) = license_of(config);
        let enc = capture(
            &ffmpeg,
            &["-hide_banner", "-encoders"],
            PROBE_TIMEOUT,
            &|| Ok(()),
        )?;
        let encoders = String::from_utf8_lossy(&enc.stdout)
            .lines()
            .filter_map(|l| {
                let mut p = l.split_whitespace();
                let flags = p.next()?;
                (flags.len() == 6 && flags.starts_with(['A', 'V']))
                    .then(|| p.next().map(str::to_string))?
            })
            .collect();
        Ok(Toolchain {
            info: ToolInfo {
                ffmpeg_path: ffmpeg.display().to_string(),
                ffprobe_path: ffprobe.display().to_string(),
                version,
                license: license.into(),
                redistributable,
                encoders,
            },
            ffmpeg,
            ffprobe,
        })
    }

    fn has_encoder(&self, name: &str) -> bool {
        self.info.encoders.iter().any(|e| e == name)
    }

    fn need_encoder(&self, name: &str) -> Result<()> {
        if self.has_encoder(name) {
            Ok(())
        } else {
            Err(Error::ToolMissing(format!(
                "в этой сборке FFmpeg нет кодека {name}; нужна сборка с его поддержкой"
            )))
        }
    }
}

// ---------- запуск процесса ----------

struct Captured {
    stdout: Vec<u8>,
    stderr: String,
}

fn read_capped(
    mut r: impl Read + Send + 'static,
    cap: usize,
) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 64 * 1024];
        let mut over = false;
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.len() + n <= cap {
                        buf.extend_from_slice(&chunk[..n]);
                    } else {
                        over = true; // дочитываем в пустоту, чтобы процесс не завис на записи
                    }
                }
            }
        }
        (buf, over)
    })
}

/// Запуск без оболочки: ограниченный вывод, таймаут, отмена. Ненулевой код → `OperationFailed`.
fn capture_with(
    exe: &Path,
    args: &[OsString],
    timeout: Duration,
    stdout_cap: usize,
    check: &dyn Fn() -> Result<()>,
) -> Result<Captured> {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("LC_ALL", "C");
    for k in ["PATH", "SystemRoot", "TEMP", "TMP", "TMPDIR"] {
        if let Some(v) = std::env::var_os(k) {
            cmd.env(k, v);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::ToolMissing(format!("не удалось запустить {}: {e}", exe.display())))?;
    let out = read_capped(child.stdout.take().unwrap(), stdout_cap);
    let err = read_capped(child.stderr.take().unwrap(), LOG_CAP);
    let started = Instant::now();
    let status = loop {
        if let Err(e) = check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(e) => return Err(e.into()),
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Timeout);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (stdout, over_out) = out.join().unwrap_or_default();
    let (stderr, _) = err.join().unwrap_or_default();
    if over_out {
        return Err(Error::LimitExceeded(
            "вывод инструмента слишком велик".into(),
        ));
    }
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    if !status.success() {
        let last = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("инструмент завершился с ошибкой");
        return Err(Error::OperationFailed(format!(
            "FFmpeg не смог обработать файл: {}",
            last.chars().take(200).collect::<String>()
        )));
    }
    Ok(Captured { stdout, stderr })
}

fn capture(
    exe: &Path,
    args: &[&str],
    timeout: Duration,
    check: &dyn Fn() -> Result<()>,
) -> Result<Captured> {
    let a: Vec<OsString> = args.iter().map(OsString::from).collect();
    capture_with(exe, &a, timeout, 4 * 1024 * 1024, check)
}

fn os(s: impl Into<OsString>) -> OsString {
    s.into()
}

// ---------- проверка входа ----------

/// Входной файл: абсолютный путь Windows, обычный файл, размер в пределах.
pub fn check_input(path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    validate_host_path(p)?;
    let meta = std::fs::metadata(crate::fsutil::long_path(p))
        .map_err(|_| Error::HostAccess("файл недоступен".into()))?;
    if !meta.is_file() {
        return invalid("нужен файл, а не папка");
    }
    if meta.len() == 0 || meta.len() > MAX_INPUT {
        return Err(Error::LimitExceeded(
            "размер файла должен быть от 1 байта до 64 МиБ".into(),
        ));
    }
    Ok(p.to_path_buf())
}

// ---------- проверка файла ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    /// `audio` | `video` | `image`
    pub kind: String,
    pub duration: f64,
    pub container: Option<String>,
    pub video: Option<StreamInfo>,
    pub audio: Option<StreamInfo>,
    pub size: u64,
}

fn s(v: &serde_json::Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::to_string)
}
fn n(v: &serde_json::Value, k: &str) -> Option<u32> {
    v.get(k)
        .and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_str().and_then(|t| t.parse().ok()))
        })
        .and_then(|x| u32::try_from(x).ok())
}

fn stream(v: &serde_json::Value) -> StreamInfo {
    StreamInfo {
        codec: s(v, "codec_name"),
        width: n(v, "width"),
        height: n(v, "height"),
        sample_rate: n(v, "sample_rate"),
        channels: n(v, "channels"),
    }
}

impl Toolchain {
    pub fn probe(&self, path: &Path, check: &dyn Fn() -> Result<()>) -> Result<MediaInfo> {
        let size = std::fs::metadata(crate::fsutil::long_path(path))?.len();
        let args = vec![
            os("-v"),
            os("error"),
            os("-max_alloc"),
            os("67108864"),
            os("-protocol_whitelist"),
            os("file"),
            os("-format_whitelist"),
            os(FORMATS),
            os("-show_entries"),
            os("format=duration,format_name:stream=codec_type,codec_name,width,height,sample_rate,channels"),
            os("-of"),
            os("json"),
            os(path),
        ];
        let out =
            capture_with(&self.ffprobe, &args, PROBE_TIMEOUT, 1024 * 1024, check).map_err(|e| {
                match e {
                    Error::OperationFailed(_) => Error::InvalidArgument(
                        "файл не удалось прочитать как аудио, видео или картинку".into(),
                    ),
                    e => e,
                }
            })?;
        let v: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|_| Error::InvalidArgument("ffprobe вернул непонятный ответ".into()))?;
        let streams = v["streams"].as_array().cloned().unwrap_or_default();
        let find = |t: &str| streams.iter().find(|x| x["codec_type"] == t).map(stream);
        let (video, audio) = (find("video"), find("audio"));
        let container = s(&v["format"], "format_name");
        let duration = v["format"]["duration"]
            .as_str()
            .and_then(|d| d.parse::<f64>().ok())
            .filter(|d| d.is_finite());
        let image_like = container
            .as_deref()
            .is_some_and(|c| c.contains("image2") || c.ends_with("_pipe"));
        let kind = if video.is_some() && audio.is_none() && (image_like || duration.is_none()) {
            "image"
        } else if video.is_some() {
            "video"
        } else if audio.is_some() {
            "audio"
        } else {
            return invalid("в файле нет аудио, видео или картинки");
        };
        let duration = if kind == "image" {
            0.0
        } else {
            match duration {
                Some(d) if (0.1..=MAX_DURATION).contains(&d) => d,
                _ => return invalid("длительность должна быть от 0,1 секунды до 10 минут"),
            }
        };
        if let Some(vs) = &video {
            let px = vs.width.unwrap_or(0) as u64 * vs.height.unwrap_or(0) as u64;
            if px == 0 || px > MAX_PIXELS {
                return Err(Error::LimitExceeded(
                    "размер изображения должен быть не больше 40 Мпикс".into(),
                ));
            }
        }
        Ok(MediaInfo {
            kind: kind.into(),
            duration,
            container,
            video,
            audio,
            size,
        })
    }

    /// Огибающая громкости: `buckets` значений 0..1 по всей дорожке (декодирование в моно 8 кГц).
    pub fn waveform(
        &self,
        path: &Path,
        buckets: usize,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<f32>> {
        if !(8..=4000).contains(&buckets) {
            return invalid("число отрезков волны — от 8 до 4000");
        }
        let args = vec![
            os("-v"),
            os("error"),
            os("-nostdin"),
            os("-max_alloc"),
            os("67108864"),
            os("-protocol_whitelist"),
            os("file"),
            os("-format_whitelist"),
            os(FORMATS),
            os("-i"),
            os(path),
            os("-map"),
            os("0:a:0"),
            os("-vn"),
            os("-ac"),
            os("1"),
            os("-ar"),
            os(WAVE_RATE.to_string()),
            os("-f"),
            os("s16le"),
            os("-"),
        ];
        let cap = (MAX_DURATION as usize + 5) * WAVE_RATE as usize * 2;
        let out = capture_with(&self.ffmpeg, &args, AUDIO_TIMEOUT, cap, check)?;
        Ok(peaks(&out.stdout, buckets))
    }
}

/// Пики s16le-сэмплов по `buckets` отрезкам (0..1).
pub fn peaks(pcm: &[u8], buckets: usize) -> Vec<f32> {
    let samples: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    if samples.is_empty() || buckets == 0 {
        return vec![0.0; buckets];
    }
    (0..buckets)
        .map(|b| {
            let lo = b * samples.len() / buckets;
            let hi = ((b + 1) * samples.len() / buckets)
                .max(lo + 1)
                .min(samples.len());
            let m = samples[lo..hi]
                .iter()
                .map(|&v| (v as i32).unsigned_abs())
                .max()
                .unwrap_or(0);
            ((m as f32 / 32768.0).min(1.0) * 1000.0).round() / 1000.0
        })
        .collect()
}

// ---------- рингтон ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingtoneOptions {
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    #[serde(default = "yes")]
    pub mono: bool,
    #[serde(default = "yes")]
    pub normalize: bool,
    #[serde(default = "lufs")]
    pub lufs: f64,
    #[serde(default = "bitrate")]
    pub bitrate: u32,
    #[serde(default = "rate")]
    pub sample_rate: u32,
}
fn yes() -> bool {
    true
}
fn lufs() -> f64 {
    -16.0
}
fn bitrate() -> u32 {
    128
}
fn rate() -> u32 {
    44100
}

const BITRATES: [u32; 9] = [32, 48, 64, 96, 128, 160, 192, 256, 320];
const RATES: [u32; 8] = [8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000];

impl RingtoneOptions {
    /// Проверка чисел: только конечные значения в допустимых пределах.
    pub fn validate(&self, source_duration: f64) -> Result<()> {
        let fin = |v: f64| v.is_finite();
        if ![self.start, self.end, self.fade_in, self.fade_out, self.lufs]
            .iter()
            .all(|&v| fin(v))
        {
            return invalid("параметры должны быть конечными числами");
        }
        if self.start < 0.0 || self.end > source_duration + 0.001 || self.start >= self.end {
            return invalid("фрагмент выходит за пределы дорожки");
        }
        let len = self.end - self.start;
        if !(0.1..=180.0).contains(&len) {
            return invalid("фрагмент должен длиться от 0,1 до 180 секунд");
        }
        let fade_max = (len / 2.0).min(10.0);
        if self.fade_in < 0.0
            || self.fade_in > fade_max
            || self.fade_out < 0.0
            || self.fade_out > fade_max
        {
            return invalid("затухание — от 0 до половины фрагмента (не больше 10 с)");
        }
        if !(-30.0..=-9.0).contains(&self.lufs) {
            return invalid("целевая громкость — от −30 до −9 LUFS");
        }
        if !BITRATES.contains(&self.bitrate) || !RATES.contains(&self.sample_rate) {
            return invalid("неподдерживаемые битрейт или частота дискретизации");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Generated {
    /// Итоговый файл на компьютере. Статус `generated`: файл создан и перечитан.
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub duration: f64,
    pub description: String,
    /// Всегда `not-tested`: ни воспроизведение, ни приём телефоном не проверялись.
    pub hardware: String,
}

fn fnum(v: f64) -> String {
    let t = format!("{v:.3}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Измерения loudnorm из журнала первого прохода.
fn parse_loudnorm(log: &str) -> Option<[f64; 5]> {
    let at = log.rfind("\"input_i\"")?;
    let open = log[..at].rfind('{')?;
    let close = log[at..].find('}')? + at;
    let v: serde_json::Value = serde_json::from_str(&log[open..=close]).ok()?;
    let mut out = [0.0; 5];
    for (i, k) in [
        "input_i",
        "input_tp",
        "input_lra",
        "input_thresh",
        "target_offset",
    ]
    .iter()
    .enumerate()
    {
        let x: f64 = v.get(*k)?.as_str()?.parse().ok()?;
        if !x.is_finite() {
            return None;
        }
        out[i] = x;
    }
    Some(out)
}

fn base_args(input: &Path, start: f64, duration: f64) -> Vec<OsString> {
    vec![
        os("-hide_banner"),
        os("-loglevel"),
        os("info"),
        os("-nostdin"),
        os("-max_alloc"),
        os("67108864"),
        os("-threads"),
        os("2"),
        os("-filter_threads"),
        os("1"),
        os("-protocol_whitelist"),
        os("file"),
        os("-format_whitelist"),
        os(FORMATS),
        // -ss/-t до -i: отсчёт времени начинается с 0, иначе afade считает от начала файла
        os("-ss"),
        os(fnum(start)),
        os("-t"),
        os(fnum(duration)),
        os("-i"),
        os(input),
    ]
}

fn check_dest(dest: &str, ext: &[&str]) -> Result<PathBuf> {
    let p = Path::new(dest);
    validate_host_path(p)?;
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::UnsafePath("нет имени файла".into()))?
        .to_ascii_lowercase();
    if !ext.iter().any(|e| name.ends_with(&format!(".{e}"))) {
        return invalid(format!(
            "имя результата должно заканчиваться на .{}",
            ext.join(" или .")
        ));
    }
    let parent = p
        .parent()
        .ok_or_else(|| Error::UnsafePath("нет папки".into()))?;
    if !crate::fsutil::long_path(parent).is_dir() {
        return Err(Error::HostAccess("папка назначения не найдена".into()));
    }
    Ok(p.to_path_buf())
}

/// Прогресс по этапам: `(сделано, всего)`.
pub type Stage<'a> = &'a dyn Fn(u64, u64);

impl Toolchain {
    /// Рингтон из фрагмента `source` в `dest` (MP3). Исходник не меняется; существующий файл не
    /// перезаписывается. Результат перечитывается (ffprobe) и хешируется.
    pub fn ringtone(
        &self,
        source: &str,
        dest: &str,
        opts: &RingtoneOptions,
        check: &dyn Fn() -> Result<()>,
        stage: Stage,
    ) -> Result<Generated> {
        self.need_encoder("libmp3lame")?;
        let src = check_input(source)?;
        let dest_path = check_dest(dest, &["mp3"])?;
        if crate::fsutil::long_path(&dest_path).exists() {
            return Err(Error::AlreadyExists);
        }
        stage(0, 4);
        let info = self.probe(&src, check)?;
        if info.audio.is_none() {
            return invalid("в файле нет звуковой дорожки");
        }
        opts.validate(info.duration)?;
        let dur = opts.end - opts.start;
        let mut filters: Vec<String> = Vec::new();
        if opts.mono {
            filters.push("aformat=channel_layouts=mono".into());
        }
        stage(1, 4);
        if opts.normalize {
            let mut target = format!("loudnorm=I={}:TP=-1.5:LRA=11", fnum(opts.lufs));
            let mut a = base_args(&src, opts.start, dur);
            a.extend([
                os("-map"),
                os("0:a:0"),
                os("-vn"),
                os("-af"),
                os(format!("{},{target}:print_format=json", filters.join(","))
                    .trim_start_matches(',')),
                os("-f"),
                os("null"),
                os("-"),
            ]);
            let first = capture_with(&self.ffmpeg, &a, AUDIO_TIMEOUT, 1024, check)?;
            if let Some(m) = parse_loudnorm(&first.stderr) {
                target.push_str(&format!(
                    ":measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}:linear=true",
                    m[0], m[1], m[2], m[3], m[4]
                ));
            }
            filters.push(target);
        }
        if opts.fade_in > 0.0 {
            filters.push(format!("afade=t=in:st=0:d={}", fnum(opts.fade_in)));
        }
        if opts.fade_out > 0.0 {
            filters.push(format!(
                "afade=t=out:st={}:d={}",
                fnum(dur - opts.fade_out),
                fnum(opts.fade_out)
            ));
        }
        stage(2, 4);
        let tmp = temp_path_for(&dest_path);
        let mut a = base_args(&src, opts.start, dur);
        a.extend([
            os("-map"),
            os("0:a:0"),
            os("-vn"),
            os("-sn"),
            os("-dn"),
            os("-map_metadata"),
            os("-1"),
        ]);
        if !filters.is_empty() {
            a.extend([os("-af"), os(filters.join(","))]);
        }
        a.extend([
            os("-c:a"),
            os("libmp3lame"),
            os("-b:a"),
            os(format!("{}k", opts.bitrate)),
            os("-ar"),
            os(opts.sample_rate.to_string()),
            os("-ac"),
            os(if opts.mono { "1" } else { "2" }),
            os("-fs"),
            os(MAX_OUTPUT.to_string()),
            os("-f"),
            os("mp3"),
            os("-y"),
            os(&tmp),
        ]);
        let run = capture_with(&self.ffmpeg, &a, AUDIO_TIMEOUT, 1024, check);
        let finish = || -> Result<Generated> {
            run?;
            stage(3, 4);
            self.verify_output(&tmp, dur, "audio", check)?;
            let (bytes, sha256) = hash_file(&crate::fsutil::long_path(&tmp), None)?;
            if bytes == 0 || bytes > MAX_OUTPUT {
                return Err(Error::LimitExceeded(
                    "результат пустой или слишком большой".into(),
                ));
            }
            place_file(
                &crate::fsutil::long_path(&tmp),
                &crate::fsutil::long_path(&dest_path),
                OverwritePolicy::Never,
            )?;
            // перечитываем уже размещённый файл
            let (b2, h2) = hash_file(&crate::fsutil::long_path(&dest_path), None)?;
            if b2 != bytes || h2 != sha256 {
                return Err(Error::VerificationFailed);
            }
            stage(4, 4);
            Ok(Generated {
                path: dest_path.display().to_string(),
                bytes,
                sha256,
                duration: dur,
                description: format!(
                    "MP3 · {} · {} кбит/с · {} Гц · {} с",
                    if opts.mono {
                        "моно"
                    } else {
                        "стерео"
                    },
                    opts.bitrate,
                    opts.sample_rate,
                    fnum(dur)
                ),
                hardware: "not-tested".into(),
            })
        };
        let r = finish();
        let _ = std::fs::remove_file(crate::fsutil::long_path(&tmp));
        r
    }

    /// Результат прочитан ffprobe: нужный тип потока, длительность около ожидаемой.
    fn verify_output(
        &self,
        tmp: &Path,
        expect: f64,
        kind: &str,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        // temp-имя без расширения: ffprobe определяет формат по содержимому. FFmpeg получает обычный
        // путь (не `\\?\`): пути длиннее 260 символов он на Windows не откроет — это ограничение инструмента.
        let info = self
            .probe(&crate::fsutil::long_path(tmp), check)
            .map_err(|e| match e {
                Error::InvalidArgument(_) => Error::VerificationFailed,
                e => e,
            })?;
        let ok = match kind {
            "audio" => info.audio.is_some() && (info.duration - expect).abs() <= 1.0,
            "video" => info.video.is_some() && (info.duration - expect).abs() <= 2.0,
            _ => info.video.is_some(),
        };
        if ok {
            Ok(())
        } else {
            Err(Error::VerificationFailed)
        }
    }
}

// ---------- пресеты ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Preset {
    pub id: &'static str,
    /// `video` | `audio` | `image`
    pub kind: &'static str,
    pub title: &'static str,
    pub ext: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "mp4-qvga-h264",
        kind: "video",
        title: "MP4 · H.264 baseline · 320×240 · AAC",
        ext: "mp4",
    },
    Preset {
        id: "3gp-qcif-mpeg4",
        kind: "video",
        title: "3GP · MPEG-4 · 176×144 · AAC",
        ext: "3gp",
    },
    Preset {
        id: "mp3-128",
        kind: "audio",
        title: "MP3 · 128 кбит/с · стерео · 44,1 кГц",
        ext: "mp3",
    },
    Preset {
        id: "mp3-64-mono",
        kind: "audio",
        title: "MP3 · 64 кбит/с · моно · 32 кГц (малый размер)",
        ext: "mp3",
    },
    Preset {
        id: "aac-96",
        kind: "audio",
        title: "AAC (M4A) · 96 кбит/с · стерео",
        ext: "m4a",
    },
    Preset {
        id: "wallpaper-jpeg",
        kind: "image",
        title: "Обои · JPEG · обрезка под экран модели",
        ext: "jpg",
    },
    Preset {
        id: "photo-fit-jpeg",
        kind: "image",
        title: "Фото · JPEG · вписано в экран модели",
        ext: "jpg",
    },
    Preset {
        id: "photo-fit-png",
        kind: "image",
        title: "Фото · PNG · вписано в экран модели",
        ext: "png",
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Экран модели по документированному профилю. Это не измерение на телефоне.
pub fn screen_of(model: &str) -> Option<(u32, u32)> {
    profile::model(model).map(|m| m.screen)
}

impl Toolchain {
    /// Подготовка файла по пресету. Для картинок размер берётся с экрана выбранной модели
    /// (E52: 240×320, E72: 320×240); пригодность телефону не утверждается.
    pub fn prepare(
        &self,
        source: &str,
        dest: &str,
        preset_id: &str,
        model: &str,
        check: &dyn Fn() -> Result<()>,
        stage: Stage,
    ) -> Result<Generated> {
        let pr =
            preset(preset_id).ok_or_else(|| Error::InvalidArgument("неизвестный пресет".into()))?;
        let src = check_input(source)?;
        let dest_path = check_dest(dest, &[pr.ext])?;
        if crate::fsutil::long_path(&dest_path).exists() {
            return Err(Error::AlreadyExists);
        }
        stage(0, 3);
        let info = self.probe(&src, check)?;
        let wrong = || {
            Error::InvalidArgument(format!(
                "пресет «{}» не подходит для файла вида «{}»",
                pr.title, info.kind
            ))
        };
        let (w, h) = if pr.kind == "image" {
            screen_of(model).ok_or_else(|| {
                Error::InvalidArgument(
                    "для картинок выберите модель телефона (размер экрана)".into(),
                )
            })?
        } else {
            (0, 0)
        };
        let mut a = base_args(&src, 0.0, MAX_DURATION);
        let (timeout, expect, kind) = match pr.id {
            "mp4-qvga-h264" | "3gp-qcif-mpeg4" => {
                if info.video.is_none() {
                    return Err(wrong());
                }
                let (vw, vh, fps, vcodec): (u32, u32, u32, &[&str]) = if pr.id == "mp4-qvga-h264" {
                    self.need_encoder("libx264")?;
                    self.need_encoder("aac")?;
                    (
                        320,
                        240,
                        25,
                        &[
                            "-c:v",
                            "libx264",
                            "-profile:v",
                            "baseline",
                            "-preset",
                            "veryfast",
                            "-b:v",
                            "384k",
                            "-maxrate",
                            "448k",
                            "-bufsize",
                            "768k",
                        ],
                    )
                } else {
                    self.need_encoder("mpeg4")?;
                    self.need_encoder("aac")?;
                    (176, 144, 15, &["-c:v", "mpeg4", "-b:v", "128k"])
                };
                a.extend([os("-map"), os("0:v:0")]);
                if info.audio.is_some() {
                    a.extend([os("-map"), os("0:a:0")]);
                }
                a.extend([
                    os("-sn"),
                    os("-dn"),
                    os("-map_metadata"),
                    os("-1"),
                    os("-vf"),
                    os(format!(
                        "scale={vw}:{vh}:force_original_aspect_ratio=decrease,pad={vw}:{vh}:(ow-iw)/2:(oh-ih)/2,fps={fps},format=yuv420p"
                    )),
                ]);
                a.extend(vcodec.iter().map(|s| os(*s)));
                if info.audio.is_some() {
                    let (abr, ar, ac) = if pr.id == "mp4-qvga-h264" {
                        ("96k", "44100", "2")
                    } else {
                        ("64k", "22050", "1")
                    };
                    a.extend([
                        os("-c:a"),
                        os("aac"),
                        os("-b:a"),
                        os(abr),
                        os("-ar"),
                        os(ar),
                        os("-ac"),
                        os(ac),
                    ]);
                }
                a.extend([os("-f"), os(if pr.ext == "mp4" { "mp4" } else { "3gp" })]);
                (VIDEO_TIMEOUT, info.duration, "video")
            }
            "mp3-128" | "mp3-64-mono" | "aac-96" => {
                if info.audio.is_none() {
                    return Err(wrong());
                }
                a.extend([
                    os("-map"),
                    os("0:a:0"),
                    os("-vn"),
                    os("-sn"),
                    os("-dn"),
                    os("-map_metadata"),
                    os("-1"),
                ]);
                let spec: &[&str] = match pr.id {
                    "mp3-128" => {
                        self.need_encoder("libmp3lame")?;
                        &[
                            "-c:a",
                            "libmp3lame",
                            "-b:a",
                            "128k",
                            "-ar",
                            "44100",
                            "-ac",
                            "2",
                            "-f",
                            "mp3",
                        ]
                    }
                    "mp3-64-mono" => {
                        self.need_encoder("libmp3lame")?;
                        &[
                            "-c:a",
                            "libmp3lame",
                            "-b:a",
                            "64k",
                            "-ar",
                            "32000",
                            "-ac",
                            "1",
                            "-f",
                            "mp3",
                        ]
                    }
                    _ => {
                        self.need_encoder("aac")?;
                        &[
                            "-c:a", "aac", "-b:a", "96k", "-ar", "44100", "-ac", "2", "-f", "ipod",
                        ]
                    }
                };
                a.extend(spec.iter().map(|s| os(*s)));
                (AUDIO_TIMEOUT, info.duration, "audio")
            }
            _ => {
                if info.kind != "image" {
                    return Err(wrong());
                }
                let cover = pr.id == "wallpaper-jpeg";
                let png = pr.ext == "png";
                self.need_encoder(if png { "png" } else { "mjpeg" })?;
                let vf = if cover {
                    format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}")
                } else {
                    format!("scale={w}:{h}:force_original_aspect_ratio=decrease")
                };
                a.extend([
                    os("-map"),
                    os("0:v:0"),
                    os("-frames:v"),
                    os("1"),
                    os("-map_metadata"),
                    os("-1"),
                    os("-vf"),
                    os(if png {
                        vf
                    } else {
                        format!("{vf},format=yuvj420p")
                    }),
                ]);
                if png {
                    a.extend([os("-c:v"), os("png"), os("-f"), os("image2")]);
                } else {
                    a.extend([
                        os("-c:v"),
                        os("mjpeg"),
                        os("-q:v"),
                        os("3"),
                        os("-f"),
                        os("image2"),
                    ]);
                }
                (PROBE_TIMEOUT * 3, 0.0, "image")
            }
        };
        stage(1, 3);
        let tmp = temp_path_for(&dest_path);
        a.extend([os("-fs"), os(MAX_OUTPUT.to_string()), os("-y"), os(&tmp)]);
        let run = capture_with(&self.ffmpeg, &a, timeout, 1024, check);
        let finish = || -> Result<Generated> {
            run?;
            stage(2, 3);
            self.verify_output(&tmp, expect, kind, check)?;
            let (bytes, sha256) = hash_file(&crate::fsutil::long_path(&tmp), None)?;
            if bytes == 0 || bytes > MAX_OUTPUT {
                return Err(Error::LimitExceeded(
                    "результат пустой или слишком большой".into(),
                ));
            }
            place_file(
                &crate::fsutil::long_path(&tmp),
                &crate::fsutil::long_path(&dest_path),
                OverwritePolicy::Never,
            )?;
            let (b2, h2) = hash_file(&crate::fsutil::long_path(&dest_path), None)?;
            if b2 != bytes || h2 != sha256 {
                return Err(Error::VerificationFailed);
            }
            stage(3, 3);
            Ok(Generated {
                path: dest_path.display().to_string(),
                bytes,
                sha256,
                duration: expect,
                description: if pr.kind == "image" {
                    format!("{} · {w}×{h} (экран модели по документации)", pr.title)
                } else {
                    pr.title.to_string()
                },
                hardware: "not-tested".into(),
            })
        };
        let r = finish();
        let _ = std::fs::remove_file(crate::fsutil::long_path(&tmp));
        r
    }
}

/// Общий разделяемый указатель на найденный инструмент (кэш в `AppCore`).
pub type SharedToolchain = Arc<Mutex<Option<Arc<Toolchain>>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_detection() {
        assert_eq!(license_of(" --enable-gpl --enable-libx264"), ("gpl", true));
        assert_eq!(
            license_of(" --enable-gpl --enable-version3"),
            ("gpl-v3", true)
        );
        assert_eq!(
            license_of(" --enable-nonfree --enable-gpl"),
            ("nonfree", false)
        );
        assert_eq!(license_of(" --enable-shared"), ("lgpl", true));
        assert_eq!(
            license_of(" --enable-version3 --enable-shared"),
            ("lgpl-v3", true)
        );
        assert_eq!(license_of(""), ("unknown", false));
    }

    #[test]
    fn peaks_cover_the_whole_track() {
        let mut pcm = Vec::new();
        for i in 0..8000i32 {
            let v: i16 = if i < 4000 { 0 } else { 16384 };
            pcm.extend(v.to_le_bytes());
        }
        let p = peaks(&pcm, 4);
        assert_eq!(p, vec![0.0, 0.0, 0.5, 0.5]);
        assert_eq!(peaks(&[], 3), vec![0.0; 3]);
        let loud = (-32768i16).to_le_bytes();
        assert_eq!(peaks(&loud, 1), vec![1.0]);
    }

    #[test]
    fn ringtone_options_are_bounded() {
        let ok = RingtoneOptions {
            start: 1.0,
            end: 11.0,
            fade_in: 0.5,
            fade_out: 1.0,
            mono: true,
            normalize: true,
            lufs: -16.0,
            bitrate: 128,
            sample_rate: 44100,
        };
        assert!(ok.validate(60.0).is_ok());
        let bad = |f: fn(&mut RingtoneOptions)| {
            let mut o = ok.clone();
            f(&mut o);
            o.validate(60.0).is_err()
        };
        assert!(bad(|o| o.start = -1.0));
        assert!(bad(|o| o.end = 61.0));
        assert!(bad(|o| o.end = 1.05));
        assert!(bad(|o| o.end = 200.0));
        assert!(bad(|o| o.start = 12.0));
        assert!(bad(|o| o.fade_in = 6.0));
        assert!(bad(|o| o.fade_out = f64::NAN));
        assert!(bad(|o| o.lufs = 0.0));
        assert!(bad(|o| o.bitrate = 100));
        assert!(bad(|o| o.sample_rate = 12345));
        assert!(bad(|o| o.start = f64::INFINITY));
    }

    #[test]
    fn loudnorm_log_is_parsed_and_garbage_is_not() {
        let log = "noise\n[Parsed_loudnorm_0 @ 0x1] \n{\n\t\"input_i\" : \"-23.5\",\n\t\"input_tp\" : \"-5.0\",\n\t\"input_lra\" : \"3.0\",\n\t\"input_thresh\" : \"-34.0\",\n\t\"output_i\" : \"-16\",\n\t\"target_offset\" : \"0.5\"\n}\n";
        assert_eq!(parse_loudnorm(log), Some([-23.5, -5.0, 3.0, -34.0, 0.5]));
        assert_eq!(parse_loudnorm("nothing"), None);
        assert_eq!(parse_loudnorm("{\"input_i\": \"inf\"}"), None);
    }

    #[test]
    fn inputs_and_outputs_are_validated() {
        assert!(check_input("relative.mp3").is_err());
        assert!(check_input(
            &std::env::temp_dir()
                .join("yeskia-no-such.mp3")
                .display()
                .to_string()
        )
        .is_err());
        assert!(
            check_input(&std::env::temp_dir().display().to_string()).is_err(),
            "папка"
        );
        let d = std::env::temp_dir();
        assert!(check_dest(&d.join("a.wav").display().to_string(), &["mp3"]).is_err());
        assert!(check_dest(&d.join("nul.mp3").display().to_string(), &["mp3"]).is_err());
        assert!(check_dest(
            &d.join("no-such-dir-x").join("a.mp3").display().to_string(),
            &["mp3"]
        )
        .is_err());
        assert!(check_dest(&d.join("ok.mp3").display().to_string(), &["mp3"]).is_ok());
    }

    #[test]
    fn missing_tool_is_a_typed_error() {
        let dir = std::env::temp_dir().join("yeskia-no-ffmpeg-here");
        let _ = std::fs::create_dir_all(&dir);
        match Toolchain::locate_in(vec![dir]) {
            Err(Error::ToolMissing(m)) => assert!(m.contains("FFmpeg")),
            other => panic!("{:?}", other.err()),
        }
        let r = capture_with(
            Path::new("/no/such/ffmpeg-binary"),
            &[],
            Duration::from_secs(2),
            1024,
            &|| Ok(()),
        );
        assert!(matches!(r, Err(Error::ToolMissing(_))));
    }

    #[cfg(unix)]
    fn script(tag: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = std::env::temp_dir().join(format!("yeskia-fake-{tag}-{}", std::process::id()));
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[cfg(unix)]
    #[test]
    fn runner_enforces_timeout_cancel_exit_code_and_output_cap() {
        let slow = script("slow", "sleep 5");
        let t0 = Instant::now();
        let r = capture_with(&slow, &[], Duration::from_millis(300), 1024, &|| Ok(()));
        assert!(matches!(r, Err(Error::Timeout)));
        assert!(
            t0.elapsed() < Duration::from_secs(3),
            "процесс убит по таймауту"
        );

        let calls = std::sync::atomic::AtomicU32::new(0);
        let cancel = || {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 3 {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        };
        let t0 = Instant::now();
        let r = capture_with(&slow, &[], Duration::from_secs(30), 1024, &cancel);
        assert!(matches!(r, Err(Error::Cancelled)));
        assert!(
            t0.elapsed() < Duration::from_secs(3),
            "процесс убит по отмене"
        );

        let fail = script("fail", "echo 'Invalid data found' >&2; exit 1");
        match capture_with(&fail, &[], Duration::from_secs(5), 1024, &|| Ok(())) {
            Err(Error::OperationFailed(m)) => assert!(m.contains("Invalid data found")),
            other => panic!("{:?}", other.err()),
        }

        let big = script("big", "head -c 200000 /dev/zero");
        let r = capture_with(&big, &[], Duration::from_secs(5), 1024, &|| Ok(()));
        assert!(matches!(r, Err(Error::LimitExceeded(_))));

        // аргументы не проходят через оболочку
        let echo = script("echo", "echo \"$1\"");
        let out = capture_with(
            &echo,
            &[os("$(echo pwned); `id`")],
            Duration::from_secs(5),
            1024,
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "$(echo pwned); `id`"
        );
    }

    #[test]
    fn presets_and_screens() {
        assert_eq!(screen_of("nokia-e52"), Some((240, 320)));
        assert_eq!(screen_of("nokia-e72"), Some((320, 240)));
        assert_eq!(screen_of("nokia-7610"), Some((176, 208)));
        assert_eq!(screen_of("unknown"), None);
        assert!(preset("mp3-128").is_some() && preset("rm -rf").is_none());
        for p in PRESETS {
            assert!(!p.title.is_empty() && !p.ext.is_empty());
        }
    }
}
