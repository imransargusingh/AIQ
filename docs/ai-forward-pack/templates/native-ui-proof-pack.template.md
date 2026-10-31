---
id: "proof-native-ui-<surface>"
title: "Native UI Proof Pack — <Surface>"
type: proof-pack
status: draft
owner: "@<handle>"
phase: "<delivery phase, if applicable>"
tags: [native-ui, proof-pack, accessibility, keyboard, dpi, signing]
links:
  - { to: <spec-or-design-id>, rel: tested-by }
  - { to: kb-native-client-ui-design, rel: depends-on }
review-by: "<ISO date — proof packs regenerate per release>"
summary: >-
  Native UI proof pack for <surface>, covering platform HIG, keyboard traversal,
  accessibility tree, theme/high-contrast, DPI/windowing, performance, OS integration
  and distribution trust. Evidence, oracles and residual risk are recorded per row.
---

# Native UI Proof Pack — <Surface>

*Use with `/ui-design`, `/design-slice` and `/implement` for WPF, WinUI, Avalonia and Blazor Hybrid surfaces. A screenshot or HTML mockup can support direction; it cannot prove native PASS for accessibility, keyboard, DPI, windowing or signing.*

## 1. Medium declaration

| Field | Value |
|---|---|
| Medium | native-desktop / blazor-hybrid / other |
| Platform(s) | windows / cross-platform / other |
| Framework | WPF / WinUI / Avalonia / Blazor Hybrid / other |
| Distribution | MSIX / EXE / Store / other |
| Accessibility API | UI Automation / browser DOM + native shell / other |
| HIG source | <official URL or knowledge source id> |

## 2. Required proof rows

| claim | failing input or condition | oracle | evidence | red observed | confidence | residual risk |
|---|---|---|---|---|---|---|
| Platform HIG is honored | Target platform declared but required menu/window/dialog/shortcut conventions omitted | Reviewer maps declared target to HIG checklist result or Flagged deviation |  | planned |  |  |
| Keyboard traversal works | User cannot complete the core flow without a pointer, including dialogs and recovery states | Recording or automated test shows traversal order, focus trap/restore, default/cancel actions and shortcut map |  | planned |  |  |
| Accessibility tree is correct | A non-text/custom/icon control lacks name/role/state/pattern or focus event | UIA / NSAccessibility / AT-SPI / Appium / Accessibility Insights shows expected properties |  | planned |  |  |
| Theme/high contrast works | OS switches Light/Dark/HighContrast or platform equivalent | Runtime resource-bound UI updates; contrast remains usable; no static/raw token breaks the theme |  | planned |  |  |
| DPI/windowing works | Window moves between mixed-DPI monitors, resizes, restores, minimizes or opens auxiliary windows | Text/icons remain crisp; window state and custom title/chrome behavior match platform expectations |  | planned |  |  |
| Large native lists remain responsive | Dense list/grid exceeds visible viewport by an order of magnitude | Virtualization remains enabled; scroll/input does not block the UI thread beyond the budget |  | planned |  |  |
| OS integration is scoped | File association, URL scheme, tray/dock/menu extra, notification, startup item or update channel is present | Integration uses platform mechanism, least privilege, discoverable controls and reversible settings |  | planned |  |  |
| Distribution trust is handled | Native artifact is unsigned, untrusted, unnotarized or has unknown SmartScreen/Gatekeeper posture | Windows signing/SmartScreen or macOS signing/notarization status is Verified, or release risk is escalated |  | planned |  |  |

## 3. Framework-specific rows

### WPF / WinUI

| claim | failing input or condition | oracle | evidence | red observed | confidence | residual risk |
|---|---|---|---|---|---|---|
| UI Automation metadata is exposed | Custom/icon-only control has no AutomationProperties/AutomationPeer equivalent | Accessibility Insights/UIA tree shows name, role, state and supported patterns |  | planned |  |  |
| Keyboard accelerators and access keys are discoverable | Core command has no menu/label/discoverable shortcut or conflicts with platform convention | Shortcut map and keyboard-only recording prove access |  | planned |  |  |
| HighContrast resources work | HighContrast theme enabled | XAML resources update and foreground/background remains legible |  | planned |  |  |
| Distribution trust is handled | MSIX/EXE/Store artifact unsigned or SmartScreen posture unknown | Signed/trusted certificate, Store signing, or explicit Release Engineer escalation |  | planned |  |  |

### Avalonia

| claim | failing input or condition | oracle | evidence | red observed | confidence | residual risk |
|---|---|---|---|---|---|---|
| AutomationProperties and AutomationId are set where needed | Icon/custom control lacks accessible name or stable automation id | Platform accessibility tree/Appium locates expected properties |  | planned |  |  |
| Focus and KeyboardNavigation work | Tab/XY focus order skips or traps the user | Headless/Appium test proves focus movement and restore |  | planned |  |  |
| Theme variants work | Light/Dark/platform variant changes | Theme resources update without raw literal drift |  | planned |  |  |
| Distribution trust is handled | Windows/macOS/Linux target lacks signing/notarization/SmartScreen or equivalent posture | Target-specific release proof or Release Engineer escalation |  | planned |  |  |

### Blazor Hybrid

| claim | failing input or condition | oracle | evidence | red observed | confidence | residual risk |
|---|---|---|---|---|---|---|
| Native shell and WebView focus hand off correctly | Keyboard traversal enters/exits WebView incorrectly or loses focus after dialog | Keyboard recording/test covers native controls + DOM controls + restore |  | planned |  |  |
| Accessibility covers both layers | DOM is accessible but native shell lacks UIA, or vice versa | Browser accessibility check plus native shell UIA/Appium proof |  | planned |  |  |
| Zoom/text scaling/high contrast works across both layers | OS text/contrast/zoom changes desynchronize shell and WebView | Both native resources and web tokens update/readably render |  | planned |  |  |
| Packaging/signing is handled | Native host package is unsigned/untrusted | Distribution proof or Release Engineer escalation |  | planned |  |  |

## 4. Exemplar/license references used

| Exemplar | License posture | Allowed use | Pattern borrowed |
|---|---|---|---|
|  | MIT / reference-only / Flagged |  |  |

## 5. Gate verdict

| | |
|---|---|
| **Verdict** | PASS / PASS-WITH-CONDITIONS / BLOCK |
| **Blocking findings** |  |
| **Residual risk** |  |
| **Best next action** |  |
