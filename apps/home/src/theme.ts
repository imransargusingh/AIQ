// The theme the person picks in Settings (thread "Add a dark mode theme option", 2026-11-05).
// "system" (the default) leaves <html> without data-theme, so the CSS media queries follow Windows: light, dark,
// and high contrast. "light" or "dark" sets data-theme, which wins over Windows. Windows high contrast themes
// (forced colours) still win over both. Kept in this computer's WebView storage, like the single-key switch.
export type Theme = "system" | "light" | "dark";

export const THEME_PREF = "aiq.theme";
export const THEMES: { v: Theme; label: string }[] = [
  { v: "system", label: "Match Windows" },
  { v: "light", label: "Light" },
  { v: "dark", label: "Dark" },
];

export function readTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_PREF);
    return t === "light" || t === "dark" ? t : "system";
  } catch {
    return "system";
  }
}

/** Show the theme now: the page at once, and the window's own title bar inside the app. */
export function applyTheme(t: Theme) {
  const root = document.documentElement;
  if (t === "system") delete root.dataset.theme;
  else root.dataset.theme = t;
  if (window.__TAURI_INTERNALS__) {
    import("@tauri-apps/api/window")
      .then((w) => w.getCurrentWindow().setTheme(t === "system" ? null : t))
      .catch(() => {
        /* the title bar keeps Windows' theme; the page already shows the choice */
      });
  }
}

export function saveTheme(t: Theme) {
  try {
    if (t === "system") localStorage.removeItem(THEME_PREF);
    else localStorage.setItem(THEME_PREF, t);
  } catch {
    /* private storage off: the choice lasts this session */
  }
  applyTheme(t);
}
