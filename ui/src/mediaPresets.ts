// Пресеты подготовки медиа и экраны моделей. Зеркало crates/yeskia-core/src/media.rs (PRESETS) и
// pkg/profile.rs (screen): размеры экрана — по документации SymPort (device_models.json), а не измерение
// на телефоне; пресеты теоретические, `hardware: not-tested`.

export type PresetKind = "video" | "audio" | "image";

export interface MediaPreset {
  id: string;
  kind: PresetKind;
  title: string;
  ext: string;
}

export const PRESETS: MediaPreset[] = [
  { id: "mp4-qvga-h264", kind: "video", title: "MP4 · H.264 baseline · 320×240 · AAC", ext: "mp4" },
  { id: "3gp-qcif-mpeg4", kind: "video", title: "3GP · MPEG-4 · 176×144 · AAC", ext: "3gp" },
  { id: "mp3-128", kind: "audio", title: "MP3 · 128 кбит/с · стерео · 44,1 кГц", ext: "mp3" },
  {
    id: "mp3-64-mono",
    kind: "audio",
    title: "MP3 · 64 кбит/с · моно · 32 кГц (малый размер)",
    ext: "mp3",
  },
  { id: "aac-96", kind: "audio", title: "AAC (M4A) · 96 кбит/с · стерео", ext: "m4a" },
  {
    id: "wallpaper-jpeg",
    kind: "image",
    title: "Обои · JPEG · обрезка под экран модели",
    ext: "jpg",
  },
  {
    id: "photo-fit-jpeg",
    kind: "image",
    title: "Фото · JPEG · вписано в экран модели",
    ext: "jpg",
  },
  { id: "photo-fit-png", kind: "image", title: "Фото · PNG · вписано в экран модели", ext: "png" },
];

export const SCREENS: Record<string, [number, number]> = {
  "nokia-e52": [240, 320],
  "nokia-e72": [320, 240],
  "nokia-n95": [240, 320],
  "nokia-n95-8gb": [240, 320],
  "nokia-n95-family": [240, 320],
  "nokia-n97-mini": [360, 640],
  "nokia-n8": [360, 640],
  "nokia-e7": [360, 640],
  "nokia-7610": [176, 208],
};

/** Готовые настройки рингтона: значения можно менять. */
export const RINGTONE_PRESETS = {
  ringtone: { length: 30, bitrate: 128, mono: true },
  notification: { length: 5, bitrate: 96, mono: true },
} as const;
