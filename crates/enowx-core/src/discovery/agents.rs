//! Discover agent definitions from disk and merge them over the shipped roster.
//!
//! Agents are files rather than Rust so the roster can be extended without a
//! recompile — a roster that needs one will not be extended. The layout
//! mirrors skills: `<dir>/<name>.md` with frontmatter, found by walking up
//! from the workspace and then in the user's home.
//!
//! Precedence, strongest first: project file, user file, shipped default. A
//! project definition shadowing a user one is deliberate, the same way a
//! project skill shadows a user skill.

use std::{collections::HashMap, fs, path::Path};

use super::{parse_frontmatter, user_home, walk_up, Discovery, SkillScope};
use crate::agent_def::{builtin_agents, AgentDef};

/// Directories holding `<agent-name>.md`. Kept alongside the skill dirs so a
/// project that already has `.agents/skills` finds `.agents/agents` without
/// being told.
const AGENT_DIRS: [&str; 4] = [
    ".enx/agents",
    ".agent/agents",
    ".agents/agents",
    ".claude/agents",
];

/// Upper bound on discovered definitions, mirroring the skill cap. A runaway
/// directory should degrade the roster, not the process.
const MAX_AGENT_ENTRIES: usize = 64;

pub fn collect(workspace: &Path, discovery: &mut Discovery) {
    // Built-ins go in first so a discovered definition can replace one by
    // name; `insert` overwrites, and scan order runs weakest to strongest.
    let mut agents: HashMap<String, AgentDef> = builtin_agents()
        .into_iter()
        .map(|a| (a.name.clone(), a))
        .collect();

    // User scope first, then project, so project wins on collision.
    if let Some(home) = user_home() {
        for suffix in AGENT_DIRS {
            scan_dir(&home.join(suffix), SkillScope::User, &mut agents, discovery);
        }
    }
    for base in walk_up(workspace) {
        for suffix in AGENT_DIRS {
            scan_dir(
                &base.join(suffix),
                SkillScope::Project,
                &mut agents,
                discovery,
            );
        }
    }

    let mut list: Vec<AgentDef> = agents.into_values().collect();
    // Stable order so the roster the router sees does not shuffle between
    // runs — a moving list makes routing non-reproducible for no reason.
    list.sort_by(|a, b| a.name.cmp(&b.name));
    discovery.agents = list;
}

fn scan_dir(
    dir: &Path,
    scope: SkillScope,
    agents: &mut HashMap<String, AgentDef>,
    discovery: &mut Discovery,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if agents.len() >= MAX_AGENT_ENTRIES {
            discovery.warnings.push(format!(
                "too many agents; ignoring the rest of {}",
                dir.display()
            ));
            return;
        }
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            discovery
                .warnings
                .push(format!("unreadable agent: {}", path.display()));
            continue;
        };
        let (mut front, body) = parse_frontmatter(&source);
        // The filename names the agent when the frontmatter does not, so a
        // bare `reviewer.md` with only a prompt still works.
        if !front.contains_key("name") {
            if let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) {
                front.insert("name".into(), stem);
            }
        }
        let Some(agent) = AgentDef::from_parts(&front, &body) else {
            discovery
                .warnings
                .push(format!("agent without a usable name: {}", path.display()));
            continue;
        };
        if agent.prompt.trim().is_empty() {
            discovery
                .warnings
                .push(format!("agent without a prompt: {}", path.display()));
            continue;
        }
        let _ = scope;
        agents.insert(agent.name.clone(), agent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new() -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "enx-agents-test-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn write(&self, rel: &str, body: &str) {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn collect_in(dir: &Dir) -> Discovery {
        let mut d = Discovery::default();
        collect(&dir.0, &mut d);
        d
    }

    fn find<'a>(d: &'a Discovery, name: &str) -> Option<&'a AgentDef> {
        d.agents.iter().find(|a| a.name == name)
    }

    /// With nothing on disk the shipped roster still has to be there, or a
    /// fresh install has no agents at all.
    #[test]
    fn built_ins_are_present_without_any_files() {
        let dir = Dir::new();
        let d = collect_in(&dir);
        assert!(find(&d, crate::agent_def::ORCHESTRATOR).is_some());
        assert!(find(&d, "fe").is_some());
        assert!(find(&d, "librarian").is_some());
    }

    #[test]
    fn a_project_file_adds_an_agent() {
        let dir = Dir::new();
        dir.write(
            ".agents/agents/reviewer.md",
            "---\nname: reviewer\ndescription: checks diffs\ntools: read, grep\n---\nYou review.",
        );
        let d = collect_in(&dir);
        let agent = find(&d, "reviewer").expect("discovered");
        assert_eq!(agent.description, "checks diffs");
        assert_eq!(agent.tools, vec!["read", "grep"]);
        assert_eq!(agent.prompt, "You review.");
    }

    /// Replacing a shipped agent is the point: a user who wants a different
    /// `fe` writes one rather than editing the binary.
    #[test]
    fn a_project_file_overrides_a_built_in() {
        let dir = Dir::new();
        dir.write(
            ".agents/agents/fe.md",
            "---\nname: fe\ndescription: my own frontend agent\ntools: read\n---\nCustom.",
        );
        let d = collect_in(&dir);
        let agent = find(&d, "fe").expect("fe exists");
        assert_eq!(agent.description, "my own frontend agent");
        assert_eq!(agent.tools, vec!["read"]);
        assert_eq!(
            d.agents.iter().filter(|a| a.name == "fe").count(),
            1,
            "the override replaces rather than duplicating"
        );
    }

    /// A bare file with only a prompt should still work; the filename is a
    /// perfectly good name.
    #[test]
    fn the_filename_names_an_agent_with_no_frontmatter_name() {
        let dir = Dir::new();
        dir.write(
            ".agents/agents/scout.md",
            "---\ndescription: looks around\ntools: read\n---\nScout.",
        );
        let d = collect_in(&dir);
        assert!(
            find(&d, "scout").is_some(),
            "agents: {:?}",
            d.agents.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
    }

    /// An agent with no prompt has nothing to say; better to report it than to
    /// let the router hand work to an empty system message.
    #[test]
    fn an_empty_prompt_is_rejected_with_a_warning() {
        let dir = Dir::new();
        dir.write(
            ".agents/agents/hollow.md",
            "---\nname: hollow\ndescription: nothing\n---\n\n",
        );
        let d = collect_in(&dir);
        assert!(find(&d, "hollow").is_none());
        assert!(
            d.warnings.iter().any(|w| w.contains("hollow")),
            "the user should be told why it was skipped: {:?}",
            d.warnings
        );
    }

    #[test]
    fn non_markdown_files_are_ignored() {
        let dir = Dir::new();
        dir.write(".agents/agents/notes.txt", "not an agent");
        dir.write(".agents/agents/config.json", "{}");
        let d = collect_in(&dir);
        assert!(d.warnings.is_empty(), "warnings: {:?}", d.warnings);
    }

    /// The roster the router sees must not shuffle between runs, or routing
    /// stops being reproducible for no reason.
    #[test]
    fn the_roster_is_ordered() {
        let dir = Dir::new();
        let first = collect_in(&dir);
        let second = collect_in(&dir);
        let names = |d: &Discovery| d.agents.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&first), names(&second));
        let mut sorted = names(&first);
        sorted.sort();
        assert_eq!(names(&first), sorted);
    }

    #[test]
    fn an_alternate_directory_is_searched() {
        let dir = Dir::new();
        dir.write(
            ".enx/agents/helper.md",
            "---\nname: helper\ndescription: helps\ntools: read\n---\nHelp.",
        );
        let d = collect_in(&dir);
        assert!(find(&d, "helper").is_some());
    }
}
