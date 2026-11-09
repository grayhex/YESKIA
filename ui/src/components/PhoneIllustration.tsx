import { useI18n } from "../i18n";

/** Условная оригинальная иллюстрация. Фото моделей Nokia не используются (права/ТЗ). */
export function PhoneIllustration() {
  const { t } = useI18n();
  return (
    <figure className="phone" style={{ margin: 0 }}>
      <svg viewBox="0 0 80 150" role="img" aria-label={t("phone.alt")}>
        <rect
          x="6"
          y="4"
          width="68"
          height="142"
          rx="10"
          fill="#2b3442"
          stroke="#0c2d66"
          strokeWidth="3"
        />
        <rect x="13" y="20" width="54" height="64" rx="2" fill="#dbe6f7" stroke="#124191" />
        <rect x="19" y="28" width="42" height="6" fill="#124191" />
        <rect x="19" y="40" width="30" height="4" fill="#8a93a1" />
        <rect x="19" y="48" width="36" height="4" fill="#8a93a1" />
        <rect x="31" y="9" width="18" height="4" rx="2" fill="#566070" />
        {[0, 1, 2].map((r) =>
          [0, 1, 2].map((c) => (
            <rect
              key={`${r}${c}`}
              x={19 + c * 16}
              y={96 + r * 14}
              width="12"
              height="9"
              rx="2"
              fill="#566070"
            />
          )),
        )}
      </svg>
      <figcaption>{t("phone.note")}</figcaption>
    </figure>
  );
}
