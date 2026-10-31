//! D3: aiq-domain depends only on aiq-contracts and std (no I/O crates).
#[test]
fn domain_dependencies_only_contracts() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let deps: Vec<&str> = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split(['=', '.']).next().unwrap().trim())
        .collect();
    assert_eq!(deps, vec!["aiq-contracts"], "aiq-domain must stay pure");
}
