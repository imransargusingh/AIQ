//! Windows pop-up for a delivered tip (setting `tip_popup`, Imran 2026-11-03). It helps when the person
//! has looked away from the terminal while the agent worked. It adds no tips: it shows only a card the
//! budget already delivered (daily cap, 30-minute quiet period, one per session).
//!
//! simplify: the system PowerShell shows the toast through Windows' own toast API, so AIQ adds no crate.
//! Ceiling: Windows names the sender "Windows PowerShell", and a click does nothing. Upgrade trigger: a
//! person finds the sender name confusing, or Home should open on click; then register an AIQ
//! AppUserModelID on the install.ps1 Start menu shortcut and show the toast from Rust.
//! assume: a toast from PowerShell's AppUserModelID shows on Windows 10 and 11 without setup (what the
//! BurntToast module relies on). Confirm: the next laptop upgrade shows a pop-up at a tip. If false:
//! nothing shows and nothing breaks (errors are swallowed in the child); the terminal tip still shows.
use aiq_contracts::card::CardV1;

/// Longest pop-up body; Windows cuts a toast's text near here anyway.
const MAX_BODY_CHARS: usize = 300;

/// The pop-up's title and body: the noticed sentence and what to try, no commands (first-use review).
pub fn text(card: &CardV1) -> (String, String) {
    let clean = |s: &str| s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>();
    let noticed = clean(card.noticed.trim());
    let mut first = noticed.chars();
    let noticed = first.next().map(|c| c.to_uppercase().chain(first).collect::<String>()).unwrap_or_default();
    let body = format!("{noticed} Try this: {}", clean(card.try_this.trim()));
    let body = if body.chars().count() > MAX_BODY_CHARS {
        format!("{}…", body.chars().take(MAX_BODY_CHARS - 1).collect::<String>())
    } else {
        body
    };
    ("AIQ tip".to_owned(), body)
}

/// The fixed script. The card's words arrive in environment variables and are XML-escaped there, so no
/// card text is ever part of the command line or the script.
#[cfg_attr(not(windows), allow(dead_code))]
const SCRIPT: &str = "$ErrorActionPreference='Stop';\
[void][Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];\
[void][Windows.Data.Xml.Dom.XmlDocument,Windows.Data.Xml.Dom.XmlDocument,ContentType=WindowsRuntime];\
$e=[Security.SecurityElement];$x=New-Object Windows.Data.Xml.Dom.XmlDocument;\
$x.LoadXml(\"<toast><visual><binding template='ToastGeneric'><text>$($e::Escape($env:AIQ_POPUP_TITLE))</text>\
<text>$($e::Escape($env:AIQ_POPUP_BODY))</text></binding></visual></toast>\");\
$id='{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe';\
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($id).Show([Windows.UI.Notifications.ToastNotification]::new($x))";

/// Show the pop-up without waiting: the core's single writer never blocks on it.
pub fn show(card: &CardV1) {
    let (title, body) = text(card);
    let ok = spawn(&title, &body).is_ok();
    tracing::info!(aiq.outcome = if ok { "popup_started" } else { "popup_failed" }, "tip pop-up");
}

#[cfg(windows)]
fn spawn(title: &str, body: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let ps = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    Command::new(ps)
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", SCRIPT])
        .env("AIQ_POPUP_TITLE", title)
        .env("AIQ_POPUP_BODY", body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

#[cfg(not(windows))]
fn spawn(_title: &str, _body: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "pop-ups are Windows only"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(noticed: &str) -> CardV1 {
        CardV1 {
            schema: aiq_contracts::card::SCHEMA.into(),
            card_id: "01J".into(),
            opportunity_id: "o".into(),
            tip_id: "t".into(),
            noticed: noticed.into(),
            why: "w".into(),
            try_this: "say what changed.".into(),
            evidence_ref: "aiq show 01J".into(),
            pack_version: "p".into(),
            starter: None,
        }
    }

    #[test]
    fn text_is_capitalised_clean_short_and_has_no_command() {
        let (t, b) = text(&card("you've reworded\u{1b}[31m this 3 times."));
        assert_eq!(t, "AIQ tip");
        assert_eq!(b, "You've reworded [31m this 3 times. Try this: say what changed.");
        assert!(!b.contains("aiq show"));
        let (_, long) = text(&card(&"x".repeat(500)));
        assert_eq!(long.chars().count(), MAX_BODY_CHARS);
    }

    #[test]
    fn script_never_holds_card_text() {
        assert!(SCRIPT.contains("$env:AIQ_POPUP_BODY") && SCRIPT.contains("Escape("));
    }
}
