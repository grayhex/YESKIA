---
version: "alpha"
name: "Soft UI Evolution"
description: "Design evolved neumorphism with improved contrast (WCAG AA+), modern aesthetics, subtle depth, accessibility focus. Ideal for landing pages, saas. AI-ready template."
colors:
  primary: "#87CEEB"
  secondary: "#FFB6C1"
  tertiary: "#90EE90"
typography:
  h1:
    fontFamily: System UI stack
    fontSize: 2.25rem
    fontWeight: 700
  body-md:
    fontFamily: System UI stack
    fontSize: 1rem
    fontWeight: 400
  label-caps:
    fontFamily: System UI stack
    fontSize: 0.75rem
    fontWeight: 500
rounded:
  sm: 10px
  md: 20px
  lg: 30px
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    rounded: "{rounded.sm}"
    padding: 12px
---

## Overview

Design evolved neumorphism with improved contrast (WCAG AA+), modern aesthetics, subtle depth, accessibility focus. Ideal for landing pages, saas. AI-ready template. Remember 2020 neumorphism? That beautiful disaster. Designers lost their minds over those puffy, extruded buttons — and then watched usability testers fail to distinguish active states from disabled ones. The contrast ratios were abysmal. WCAG compliance? Nonexistent. The style died fast, but the desire for tactile, physical-feeling interfaces never went away.

Samsung's One UI quietly showed the path forward. Soft shadows, generous padding, rounded containers — but with actual color differentiation and readable text. They proved you could have depth without sacrificing legibility. Apple's visionOS spatial design language pushed it further: layered surfaces with clear hierarchy, not just embossed sameness.

By 2026, Soft UI Evolution is what neumorphism should have been from the start. Real contrast ratios meeting AA standards. Shadows that communicate elevation, not decoration. Color palettes that use soft tones without washing out interactive elements. The tactile metaphor survived — it just grew up.

- Density: 5/10 — Balanced
- Variance: 4/10 — Moderate
- Motion: 4/10 — Subtle

- **Style:** Soft, Subtle, Pastel, Refined
- **Keywords:** Evolved soft UI, better contrast, modern aesthetics, subtle depth, accessibility-focused, improved shadows, hybrid
- **Era:** 2020s Modern
- **Light/Dark:** ✓ Full / ✓ Full

## Colors

- **Soft Blue** (#87CEEB) — Accent highlight, links and focus states
- **Soft Pink** (#FFB6C1) — Primary text color
- **Soft Green** (#90EE90) — Supporting palette color


## Typography

- **Display / Hero:** System UI stack (-apple-system, sans-serif) — Weight 700, tight tracking, used for headline impact
- **Body:** System UI stack (-apple-system, sans-serif) — Weight 400, 16px/1.6 line-height, max 72ch per line
- **UI Labels / Captions:** System UI stack (-apple-system, sans-serif) — 0.875rem, weight 500, slight letter-spacing
- **Monospace:** JetBrains Mono — Used for code, metadata, and technical values

Scale:
- Hero: clamp(2.5rem, 5vw, 4rem)
- H1: 2.25rem
- H2: 1.5rem
- Body: 1rem / 1.6
- Small: 0.875rem


## Layout

- **Grid:** CSS Grid primary. Max-width containment: 1280px centered with 1.5rem side padding.
- **Spacing rhythm:** Balanced. Base unit: 0.5rem (8px).
- **Section vertical gaps:** clamp(4rem, 8vw, 8rem).
- **Hero layout:** Split-screen (text left, visual right).
- **Feature sections:** Zig-zag alternating text+image rows. No 3-equal-columns.
- **Mobile collapse:** All multi-column layouts collapse below 768px. No horizontal overflow.
- **z-index contract:** base (0) / sticky-nav (100) / overlay (200) / modal (300) / toast (500).


## Elevation & Depth

Improved shadows (softer than flat, clearer than neumorphism), modern (200-300ms), focus visible, WCAG AA/AAA

- **Physics:** Ease-out curves, 200-300ms duration. Smooth and predictable.
- **Entry animations:** Fade + translate-Y (16px → 0) over 420ms ease-out. Staggered cascades for lists: 80ms between items.
- **Hover states:** Subtle color shift + shadow adjustment over 200ms.
- **Page transitions:** Fade only (200ms).
- **Performance:** Only transform and opacity animated. No layout-triggering properties.


## Shapes

Base corner radius: 10px. See rounded tokens in front matter for the full scale.


## Components

- **Primary Button:** Rounded (10px) shape. Accent color fill. Hover: 8% darken + subtle lift shadow. Active: -1px translate tactile press. Font weight 600. No outer glows.
- **Secondary / Ghost Button:** Outline variant. 1.5px border in muted color. Text in primary color. Hover: subtle background fill.
- **Cards:** Rounded (10px) corners. Surface background. Subtle shadow (0 2px 12px rgba(0,0,0,0.06)). 1px border stroke.
- **Inputs:** Label above input. 1px border stroke. Focus ring: 2px accent color offset 2px. Error text below in semantic red. No floating labels.
- **Navigation:** Primary surface background. Active item: accent color indicator. Font weight 500 when active.
- **Skeletons:** Shimmer animation matching component dimensions. No circular spinners.
- **Empty States:** Icon-based composition with descriptive text and action button.


## Do's and Don'ts

- No emojis in UI — use icon system only (Lucide, Heroicons)
- No pure black (#000000) — use off-black or charcoal variants
- No oversaturated accent colors (saturation cap: 80%)
- No 3-column equal-width feature layouts — use zig-zag or asymmetric grid
- No `h-screen` — use `min-h-[100dvh]`
- No AI copywriting clichés: "Elevate", "Seamless", "Unleash", "Next-Gen"
- No broken external image links — use picsum.photos or inline SVG
- No generic lorem ipsum in demos

- Do Improved contrast AA/AAA
- Do Soft shadows modern
- Do Border-radius 8-12px
- Do Animations 200-300ms
- Do Focus states visible
- Do Color hierarchy clear


## Use Case

Landing pages, SaaS

<!-- Source: https://designmd.app/library/soft-ui-evolution · designmd.app -->
