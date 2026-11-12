# Bluetooth selection, theme and layout follow-up

This is a separate change after #41–43 / PR #45, based on the owner's Windows
feedback. It keeps the Fluent surfaces, operation queues and per-operation
transport routing.

## Bluetooth boundary

Discovery reads the Windows paired-device cache with inquiry explicitly disabled.
It does not run SDP, open RFCOMM, probe greetings or connect services. The
Bluetooth Class of Device must use format 0 and Major Device Class Phone (0x02).
Audio/video, computer, peripheral and unknown classes are excluded even if their
friendly name says Nokia/E72. Pairing/name alone is not model identity or proof
of rshell. The Windows opener rechecks pairing and class before its first active
SDP/RFCOMM call. Only the explicitly selected phone is then contacted; strict
rshell greeting validation remains required. Cached routes cannot bypass the
class guard.

A regression test runs repeated Engine discovery with a phone, headphones named
“Nokia E72”, a computer and an unknown-class device: only the phone is listed,
with zero open calls. One explicit connection attempts exactly that phone once.
The existing heartbeat is limited to already explicitly connected agent sessions;
startup and refresh never call connect. Physical Windows radio/headset behaviour
still requires owner acceptance; no local cloud test is labelled a hardware run.

Unknown Class of Device is deliberately not guessed from the name. If an actual
Nokia reports an unknown class, inspect its Windows pairing/class rather than
re-enabling broad probing of arbitrary endpoints.

## Appearance

The persisted theme is an explicit light/dark enum. Old settings without the
field migrate to light, and the OS dark preference does not silently change it.
The visible header switch saves through the existing settings backend. Native
form controls get the same `color-scheme` as the app. The Tauri window frame also
uses the saved preference on startup and settings changes. A layout effect applies
palette changes with transitions suspended for the first frame, preventing the
white buttons seen during a light-to-dark transition. Normal hover transitions
resume afterwards. Browser checks reject pale secondary/selected controls in dark
mode; a first-frame transition sample also checks the actual computed backgrounds.

The exact transparent PNG supplied by the owner is bundled locally, enlarged in
header and sidebar. CSS clips transparent margins; the source pixels are unchanged.
Windows icons come from that same source. Menu icons use the existing semantic
accent palette, including selection outlines/markers and forced-colour support.

## Connection workspace and grid audit

Bluetooth and USB have separate instructions and detected counts. Automatic
means detection; connecting still requires the owner's selected-phone request.
USB PC Suite/PIM is distinguished from MTP/mass-storage files; Bluetooth describes
rshelld file access. A visible transport badge follows the selected route.

The previous stack of full device panels is replaced by a compact device list and
one selected-device panel. Long lists and expanded capability tables own their
scrolling; initial standard/developer views fit without page/panel overflow at
1366×768 and 1500×900. Transport filters do not initiate connections. The duplicated
numbered USB guide is removed. Developer data remains available inside details.
Write failures retain uncertainty and the original recovery action text.

All navigation pages were captured and their panel edges checked. Shared panel
first/last spacing and button alignment are normalized; the Home heading now
starts at the grid edge, with actions grouped to the right. Developer system
facts now live with the other system details in Settings, reached by the Home
System action, so they do not push the dashboard below the fold.
The layout matrix covers both languages/themes, standard/developer mode, multiple
routes, empty/connected/read-only states, long lists and 150%/200% scaling. The
first-run, TLS and local contact editor remain covered from PR #45.

## Checks

136 UI tests, Rust core/ADbian tests and clippy, privacy scan, UI production build,
and Windows GNU native cross-check pass. The browser matrix has 343 successful
checks, including the first dark frame with animation enabled. Review screenshots
use explicitly synthetic routes and a visible MOCK banner:
[Light](screenshots/fluent/connection-light.png),
[Dark](screenshots/fluent/connection-dark.png).
Windows MSVC packaging/smoke/NSIS installation checks run on the PR before merge.
