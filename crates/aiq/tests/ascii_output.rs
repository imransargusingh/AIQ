//! Control for defect class OUT-A: the CLI printed "52–79%" and Windows PowerShell 5.1 showed "52ΓÇô79%"
//! (measured on the laptop, 2026-11-02). The terminal's code page is not ours to choose, so everything the
//! `aiq` binary prints itself is ASCII. Scans non-test, non-comment source lines of the files that build
//! CLI text. Tip lines rendered by Claude Code or Copilot are not in scope (those hosts render UTF-8).
use std::path::Path;

fn non_ascii_code_lines(path: &Path) -> Vec<String> {
    let src = std::fs::read_to_string(path).unwrap();
    let mut bad = Vec::new();
    for (i, line) in src.lines().enumerate() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        if line.trim_start().starts_with("//") {
            continue;
        }
        if !line.is_ascii() {
            bad.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
        }
    }
    bad
}

#[test]
fn cli_output_source_is_ascii() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(root.join("src"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.push(root.join("../aiq-domain/src/check.rs"));
    let bad: Vec<String> = files.iter().flat_map(|f| non_ascii_code_lines(f)).collect();
    assert!(bad.is_empty(), "non-ASCII in CLI output code (shows as mojibake in PowerShell 5.1):\n{}", bad.join("\n"));
}
