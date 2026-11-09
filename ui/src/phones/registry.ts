// Реестр графики телефонов. Модели и геометрия экрана — из реестра s60modern
// (symport/phone/device_models.json, commit efaf53d); происхождение и статус прав — docs/ASSETS.md.
import e52 from "../assets/phones/nokia-e52.webp";
import e72 from "../assets/phones/nokia-e72.webp";
import n95 from "../assets/phones/nokia-n95.webp";
import n958gb from "../assets/phones/nokia-n95-8gb.webp";
import n95family from "../assets/phones/nokia-n95-family.svg";
import n97mini from "../assets/phones/nokia-n97-mini.webp";
import n8 from "../assets/phones/nokia-n8.webp";
import e7 from "../assets/phones/nokia-e7.webp";
import n7610 from "../assets/phones/nokia-7610.webp";

export type Provenance =
  /** Сгенерированная иллюстрация по фото-референсу (оригинальные фото не поставляются). */
  | "reference-based"
  /** Оригинальная схематичная графика проекта. */
  | "schematic";

export interface PhoneModel {
  id: string;
  name: string;
  family: string;
  /** Нормализованные обозначения (см. `normalizeModel`), по которым модель опознаётся. */
  aliases: string[];
  image: string;
  /** Размер и область экрана на иллюстрации (для будущих превью), пиксели изображения. */
  artwork?: {
    width: number;
    height: number;
    screen: { x: number; y: number; width: number; height: number };
  };
  provenance: Provenance;
  /** Права на иллюстрацию не подтверждены (производное от сторонних фото); см. docs/ASSETS.md. */
  rightsVerified: boolean;
  source: string;
}

export const PHONES: PhoneModel[] = [
  {
    id: "nokia-e52",
    name: "Nokia E52",
    family: "Eseries",
    aliases: ["e52"],
    image: e52,
    artwork: { width: 831, height: 1893, screen: { x: 130, y: 263, width: 566, height: 722 } },
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://en.wikipedia.org/wiki/Nokia_E52",
  },
  {
    id: "nokia-e72",
    name: "Nokia E72",
    family: "Eseries",
    aliases: ["e72"],
    image: e72,
    artwork: { width: 894, height: 1758, screen: { x: 124, y: 338, width: 650, height: 448 } },
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://en.wikipedia.org/wiki/Nokia_E72",
  },
  {
    id: "nokia-n95",
    name: "Nokia N95",
    family: "Nseries",
    aliases: ["n95-1", "n95-3", "n95-5", "n95 nam", "n95 china"],
    image: n95,
    artwork: { width: 1000, height: 1868, screen: { x: 145, y: 264, width: 711, height: 948 } },
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://en.wikipedia.org/wiki/Nokia_N95",
  },
  {
    id: "nokia-n95-8gb",
    name: "Nokia N95 8GB",
    family: "Nseries",
    aliases: ["n95-2", "n95-4", "n95-6", "n95 8gb", "n95 8 gb"],
    image: n958gb,
    artwork: { width: 1000, height: 1868, screen: { x: 102, y: 240, width: 795, height: 1060 } },
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://en.wikipedia.org/wiki/Nokia_N95",
  },
  {
    // Общее «N95» без указания варианта: схематичная графика, а не конкретный корпус.
    id: "nokia-n95-family",
    name: "Nokia N95",
    family: "Nseries",
    aliases: ["n95"],
    image: n95family,
    artwork: { width: 1000, height: 1868, screen: { x: 154, y: 272, width: 692, height: 923 } },
    provenance: "schematic",
    rightsVerified: true,
    source: "https://en.wikipedia.org/wiki/Nokia_N95",
  },
  {
    id: "nokia-n97-mini",
    name: "Nokia N97 mini",
    family: "Nseries",
    aliases: ["n97 mini", "n97mini", "n97 mini-1"],
    image: n97mini,
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://en.wikipedia.org/wiki/Nokia_N97_mini",
  },
  {
    id: "nokia-n8",
    name: "Nokia N8-00",
    family: "Nseries",
    aliases: ["n8", "n8-00"],
    image: n8,
    artwork: { width: 1064, height: 1576, screen: { x: 249, y: 282, width: 567, height: 1008 } },
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://github.com/grayhex/s60modern/issues/68",
  },
  {
    id: "nokia-e7",
    name: "Nokia E7-00",
    family: "Eseries",
    aliases: ["e7", "e7-00"],
    image: e7,
    artwork: { width: 927, height: 1814, screen: { x: 141, y: 340, width: 648, height: 1152 } },
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://github.com/grayhex/s60modern/issues/68",
  },
  {
    id: "nokia-7610",
    name: "Nokia 7610",
    family: "Series 60 2nd Edition",
    aliases: ["7610"],
    image: n7610,
    provenance: "reference-based",
    rightsVerified: false,
    source: "https://en.wikipedia.org/wiki/Nokia_7610",
  },
];

/** `Nokia E52-1 (MOCK)` → `e52-1`: без бренда, скобок, лишних пробелов, в нижнем регистре. */
export function normalizeModel(raw: string): string {
  return raw
    .replace(/\([^)]*\)/g, " ")
    .replace(/^\s*nokia\s+/i, "")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
}

/**
 * Находит иллюстрацию по **подтверждённой** модели (результат идентификации по протоколу).
 * Только точные совпадения (обозначение или обозначение без регионального суффикса `-N`);
 * префиксов нет, чтобы не путать близкие модели. Неизвестная модель → `null` (нейтральная графика).
 */
export function findPhone(model: string | null | undefined): PhoneModel | null {
  if (!model) return null;
  const n = normalizeModel(model);
  if (!n) return null;
  const exact = PHONES.find((p) => p.aliases.includes(n));
  if (exact) return exact;
  const base = n.replace(/-\d+$/, "");
  if (base !== n) return PHONES.find((p) => p.aliases.includes(base)) ?? null;
  return null;
}
