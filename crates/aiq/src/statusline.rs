//! `aiq statusline copilot`: Copilot CLI's status-line command where it is not `aiq-statusline.exe` (macOS, Linux,
//! a person's own status-line script, and Windows installs from before 0.1.20 until their wrapper is refreshed).
//! The same code as `aiq-statusline.exe` (crate `aiq-statusline`), so both always print the same line.

pub fn run() -> std::process::ExitCode {
    aiq_statusline::run()
}
