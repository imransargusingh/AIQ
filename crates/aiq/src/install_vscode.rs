//! `aiq install|uninstall vscode`: add or remove AIQ's VS Code extension with VS Code's own `code` command,
//! in each VS Code build found (stable `code`, Insiders `code-insiders`).
//! The extension (`extensions/vscode`) shows a tip after a Copilot agent turn ends; the release ships it as
//! `aiq.vsix` beside `aiq.exe`. AIQ never writes VS Code's files itself: `code --install-extension` does,
//! and `code --uninstall-extension` is the undo.
use crate::cli::confirm;
use aiq_core::tools::{VSCODE_EXTENSION_ID, vscode_builds, vscode_has_aiq};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// The extension file the release puts beside `aiq.exe` (`packaging/windows/install.ps1` copies it to bin).
pub const VSIX: &str = "aiq.vsix";

/// What one run of `code` did: succeeded, or its last error line.
fn run_code(cli: &Path, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let mut c = Command::new(cli);
    c.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash from AIQ home
    }
    let o = c.output().map_err(|e| format!("Could not run {} ({e}).", cli.display()))?;
    if o.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&o.stderr);
    let out = String::from_utf8_lossy(&o.stdout);
    let last = err.lines().chain(out.lines()).rfind(|l| !l.trim().is_empty()).unwrap_or("no message");
    Err(format!("VS Code's command failed: {}", last.trim()))
}

/// The bundled extension, if this copy of AIQ has one.
pub fn bundled_vsix(exe_dir: &Path) -> Option<PathBuf> {
    Some(exe_dir.join(VSIX)).filter(|p| p.is_file())
}

/// `aiq install vscode`: add AIQ's extension to every VS Code build found (stable and Insiders). `strict`:
/// run by name, so no VS Code or no extension file is a failure; from `aiq install` (connect every tool found)
/// a copy of AIQ without `aiq.vsix` (a developer build) skips VS Code with one line instead of failing setup.
pub fn install_with(yes: bool, strict: bool) -> ExitCode {
    let fail = if strict { ExitCode::from(1) } else { ExitCode::SUCCESS };
    let found: Vec<_> = vscode_builds().into_iter().filter_map(|v| v.cli.clone().map(|c| (v, c))).collect();
    if found.is_empty() {
        println!("VS Code's code command wasn't found. Install VS Code, then run aiq install vscode.");
        return fail;
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
    let Some(vsix) = bundled_vsix(&exe_dir) else {
        println!(
            "Skipped: this copy of AIQ has no {VSIX} beside it, so AIQ can't add its VS Code extension. Install AIQ from the release zip to get it."
        );
        return fail;
    };
    let names = found.iter().map(|(v, _)| v.build.name).collect::<Vec<_>>().join(" and ");
    println!("AIQ will add its extension to {names} with VS Code's own command:\n");
    for (_, cli) in &found {
        println!("  {} --install-extension {} --force", cli.display(), vsix.display());
    }
    println!(
        "\nIt shows an AIQ tip in VS Code after a Copilot agent turn ends. Your other extensions and settings stay as they are."
    );
    if !confirm(&format!("Add the AIQ extension to {names}?"), yes) {
        println!("Nothing changed.");
        return ExitCode::SUCCESS;
    }
    let mut failed = false;
    for (v, cli) in &found {
        // --force: a reinstall replaces an older AIQ extension with this release's.
        match run_code(cli, &["--install-extension".as_ref(), vsix.as_os_str(), "--force".as_ref()]) {
            Ok(()) => println!("Added to {}.", v.build.name),
            Err(m) => {
                println!("{}: {m}", v.build.name);
                failed = true;
            }
        }
    }
    if failed {
        return ExitCode::from(1);
    }
    println!(
        "Installed. Tips show in VS Code when you use Copilot Chat in Agent mode with the session target set to Copilot. If VS Code is open, it may ask you to reload the window."
    );
    ExitCode::SUCCESS
}

pub fn install(yes: bool) -> ExitCode {
    install_with(yes, true)
}

/// `aiq uninstall vscode`: remove AIQ's extension from every VS Code build whose extension list names it.
pub fn uninstall(yes: bool) -> ExitCode {
    let with: Vec<_> = vscode_builds().into_iter().filter(|v| vscode_has_aiq(&v.extensions)).collect();
    if with.is_empty() {
        println!("The AIQ extension isn't installed in VS Code.");
        return ExitCode::SUCCESS;
    }
    let mut failed = false;
    for v in with.iter().filter(|v| v.cli.is_none()) {
        println!(
            "{} lists the AIQ extension, but its {} command wasn't found. Remove it in {}: Extensions, AIQ, Uninstall.",
            v.build.name, v.build.command, v.build.name
        );
        failed = true;
    }
    let run: Vec<_> = with.iter().filter_map(|v| v.cli.as_ref().map(|c| (v.build.name, c))).collect();
    if run.is_empty() {
        return ExitCode::from(1);
    }
    let names = run.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(" and ");
    println!("AIQ will remove its extension from {names} with VS Code's own command:\n");
    for (_, cli) in &run {
        println!("  {} --uninstall-extension {VSCODE_EXTENSION_ID}", cli.display());
    }
    println!();
    if !confirm(&format!("Remove the AIQ extension from {names}?"), yes) {
        println!("Nothing changed.");
        return ExitCode::SUCCESS;
    }
    for (name, cli) in &run {
        match run_code(cli, &["--uninstall-extension".as_ref(), VSCODE_EXTENSION_ID.as_ref()]) {
            Ok(()) => println!("Removed the AIQ extension from {name}."),
            Err(m) => {
                println!("{name}: {m}");
                failed = true;
            }
        }
    }
    if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_file_is_found_only_beside_aiq() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(bundled_vsix(d.path()), None);
        std::fs::create_dir(d.path().join(VSIX)).unwrap();
        assert_eq!(bundled_vsix(d.path()), None, "a folder named like it is not the extension");
        let e = tempfile::tempdir().unwrap();
        std::fs::write(e.path().join(VSIX), b"PK").unwrap();
        assert_eq!(bundled_vsix(e.path()), Some(e.path().join(VSIX)));
    }

    /// The id AIQ uninstalls and looks for is the one the extension is built with (publisher.name).
    #[test]
    fn extension_id_matches_the_extension_package() {
        let pkg: serde_json::Value =
            serde_json::from_str(include_str!("../../../extensions/vscode/package.json")).unwrap();
        let id = format!("{}.{}", pkg["publisher"].as_str().unwrap(), pkg["name"].as_str().unwrap());
        assert_eq!(id, VSCODE_EXTENSION_ID);
    }
}
