//! Team tips (docs/team-tips/README.md): the public repo holds only the placeholder example, and every habit
//! id a team's file may name stays in the pack.
use aiq_domain::PackSource;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !matches!(name.as_str(), ".git" | "target" | "node_modules" | "out" | "dist") {
                walk(&p, out);
            }
        } else {
            out.push(p);
        }
    }
}

#[test]
fn team_tips_files_stay_out_of_the_repo() {
    let root = repo();
    let example = root.join("docs/team-tips/example-team-tips.toml");
    let mut files = vec![];
    walk(&root, &mut files);
    let mut found = vec![];
    for f in files {
        let is_toml = f.extension().is_some_and(|e| e == "toml");
        let named = f.file_name().is_some_and(|n| n == "team-tips.toml");
        let uses_schema = is_toml && std::fs::read_to_string(&f).is_ok_and(|t| t.contains("aiq.team-tips.v1"));
        if (named || uses_schema) && f != example {
            found.push(f.strip_prefix(&root).unwrap_or(&f).display().to_string());
        }
    }
    assert!(found.is_empty(), "a team tips file must never be in this repo (it names internal tools): {found:?}");
}

#[test]
fn the_example_uses_placeholder_names_and_real_habit_ids() {
    let text = std::fs::read_to_string(repo().join("docs/team-tips/example-team-tips.toml")).unwrap();
    let t = aiq_core::team_tips::parse(&text).unwrap();
    let pack = aiq_core::pack::BundledPack.load().unwrap();
    assert!(!t.tools.is_empty());
    for x in &t.tools {
        assert!(x.tool.starts_with("Example "), "placeholder names only: {}", x.tool);
        assert!(x.link.as_deref().is_none_or(|l| l.starts_with("https://example.com/")), "{:?}", x.link);
        assert!(pack.moves.iter().any(|m| m.move_id == x.move_id), "{}", x.move_id);
    }
}

#[test]
fn every_habit_id_teams_may_use_is_still_in_the_pack() {
    let text = std::fs::read_to_string(repo().join("docs/team-tips/move-ids.txt")).unwrap();
    let pack = aiq_core::pack::BundledPack.load().unwrap();
    let listed: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    for id in &listed {
        assert!(
            pack.moves.iter().any(|m| m.move_id == *id),
            "habit id {id} left the pack: teams' files name it. Remove it from move-ids.txt on purpose and say so in the release notes."
        );
    }
    for m in &pack.moves {
        assert!(listed.contains(&m.move_id.as_str()), "new habit {}: add it to docs/team-tips/move-ids.txt", m.move_id);
    }
}
