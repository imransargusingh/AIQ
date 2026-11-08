//! `aiq-statusline.exe`: Copilot CLI's status-line command on Windows (see the library).
//! `--version` prints this program's own version; release.yml compares it to decide whether to reuse the file.
fn main() -> std::process::ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("aiq-statusline {}", aiq_statusline::VERSION);
        return std::process::ExitCode::SUCCESS;
    }
    aiq_statusline::run()
}
