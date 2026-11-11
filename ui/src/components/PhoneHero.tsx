import { useI18n } from "../i18n";
import e72 from "../assets/phones/nokia-e72.webp";
import n8 from "../assets/phones/nokia-n8.webp";
import e7 from "../assets/phones/nokia-e7-open.webp";

/** Decorative collection, independent of discovered devices. */
export function PhoneHero() {
  const { t } = useI18n();
  return (
    <section className="phone-hero">
      <div className="hero-copy">
        <p className="eyebrow">{t("hero.label")}</p>
        <h2>{t("hero.title")}</h2>
        <p>{t("hero.text")}</p>
      </div>
      <div className="hero-phones" aria-hidden="true">
        <img src={e7} alt="" className="hero-e7" />
        <img src={n8} alt="" className="hero-n8" />
        <img src={e72} alt="" className="hero-e72" />
      </div>
    </section>
  );
}
