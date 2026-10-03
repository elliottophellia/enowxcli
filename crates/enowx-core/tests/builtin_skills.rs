//! Skills that ship inside enx: `ui`, `code` and `writing`, the depth behind
//! the specialists' prompts, on every install whether or not any skill is on
//! disk. A project or user skill of the same name replaces one.

use std::sync::{Arc, Mutex};

use enowx_core::{
    discovery::{skills::builtin_source, Discovery, SkillScope},
    tools::{skill::SkillReadTool, Tool, ToolCtx},
};

/// Discovery reads skills from `$HOME`, which is the whole process's: tests
/// that point it at their own empty directory take turns.
static HOME: Mutex<()> = Mutex::new(());

struct Scratch {
    workspace: std::path::PathBuf,
    root: std::path::PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "enx-builtin-skills-{}-{tag}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = root.join("ws");
        // `.git` stops discovery walking up out of the workspace.
        std::fs::create_dir_all(workspace.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        Self { workspace, root }
    }

    fn discover(&self) -> Discovery {
        std::env::set_var("HOME", self.root.join("home"));
        Discovery::run(&self.workspace)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn scope_of(discovery: &Discovery, name: &str) -> Option<SkillScope> {
    discovery
        .skills
        .iter()
        .find(|skill| skill.name == name)
        .map(|skill| skill.scope)
}

#[test]
fn every_workspace_has_the_builtin_skills() {
    let _home = HOME.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new("empty");
    let discovery = scratch.discover();
    for name in [
        "ui",
        "ui-layout",
        "ui-audit",
        "ui-page-dashboard",
        "ui-part-hero",
        "ui-part-sidebar",
        "code",
        "writing",
        "brainstorm",
        "backend",
        "backend-api",
        "backend-data",
        "backend-stack-next",
        "motion",
        "motion-reveal",
        "motion-comfort",
    ] {
        let skill = discovery
            .skills
            .iter()
            .find(|skill| skill.name == name)
            .unwrap_or_else(|| panic!("`{name}` ships with enx"));
        assert_eq!(skill.scope, SkillScope::Builtin);
        assert!(
            skill.description.len() > 40,
            "a description to be chosen from: {:?}",
            skill.description
        );
    }
    let listed = discovery.system_prompt_supplement().expect("a skill list");
    for name in [
        "`ui`",
        "`ui-layout`",
        "`ui-audit`",
        "`code`",
        "`writing`",
        "`brainstorm`",
    ] {
        assert!(listed.contains(name), "{name} is offered: {listed}");
    }
}

/// The built-in is a default, not a rule: a team's own `ui` skill wins.
#[test]
fn a_project_skill_replaces_the_builtin_of_the_same_name() {
    let _home = HOME.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new("override");
    let dir = scratch.workspace.join(".agents/skills/ui");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: ui\ndescription: our design system\n---\n\nUse the house kit.\n",
    )
    .unwrap();
    let discovery = scratch.discover();
    assert_eq!(scope_of(&discovery, "ui"), Some(SkillScope::Project));
    assert_eq!(
        discovery.skills.iter().filter(|s| s.name == "ui").count(),
        1,
        "one `ui`, the project's"
    );
    assert_eq!(scope_of(&discovery, "code"), Some(SkillScope::Builtin));
}

/// Reading a built-in skill returns its text from the binary: there is no
/// file on disk to read.
#[tokio::test]
async fn a_builtin_skill_reads_from_the_binary() {
    let discovery = {
        let _home = HOME.lock().unwrap_or_else(|e| e.into_inner());
        Arc::new(Scratch::new("read").discover())
    };
    let ctx = ToolCtx {
        workspace: std::env::temp_dir(),
        shell_timeout: std::time::Duration::from_secs(5),
        cancel: tokio_util::sync::CancellationToken::new(),
        progress: None,
        call_id: String::new(),
        skills: vec!["ui".into()],
        lsp: None,
        repair: None,
        vision: false,
        cloudflare_token: None,
    };
    let tool = SkillReadTool::new(discovery.clone());
    let out = tool
        .execute(&ctx, serde_json::json!({ "name": "ui" }))
        .await
        .expect("read");
    assert!(!out.is_error, "{}", out.content);
    assert!(
        out.content.contains("# Interface design"),
        "{}",
        out.content
    );

    let off = SkillReadTool::with_disabled(discovery, vec!["ui".into()]);
    let out = off
        .execute(&ctx, serde_json::json!({ "name": "ui" }))
        .await
        .expect("read");
    assert!(out.is_error, "a disabled built-in is not offered");
}

/// A built-in skill belongs to the agents that carry it. One that does not
/// is refused it even by name, since a model can ask for a skill it was
/// never shown.
#[tokio::test]
async fn an_agent_cannot_read_a_builtin_it_does_not_carry() {
    let discovery = {
        let _home = HOME.lock().unwrap_or_else(|e| e.into_inner());
        Arc::new(Scratch::new("carry").discover())
    };
    let ctx = ToolCtx {
        workspace: std::env::temp_dir(),
        shell_timeout: std::time::Duration::from_secs(5),
        cancel: tokio_util::sync::CancellationToken::new(),
        progress: None,
        call_id: String::new(),
        skills: vec!["code".into()],
        lsp: None,
        repair: None,
        vision: false,
        cloudflare_token: None,
    };
    let tool = SkillReadTool::new(discovery);
    let ui = tool
        .execute(&ctx, serde_json::json!({ "name": "ui" }))
        .await
        .expect("read");
    assert!(ui.is_error, "`ui` is not carried: {}", ui.content);
    assert!(
        ui.content.contains("not one of your skills"),
        "{}",
        ui.content
    );
    let code = tool
        .execute(&ctx, serde_json::json!({ "name": "code" }))
        .await
        .expect("read");
    assert!(!code.is_error, "`code` is carried: {}", code.content);
}

/// Who carries what: interface work gets `ui`, anything that writes code
/// gets `code`, anything whose words people read gets `writing`, and the
/// agents with nothing to shape carry none.
#[test]
fn each_agent_carries_the_skills_for_its_work() {
    let roster = enowx_core::builtin_agents();
    let carried = |name: &str| -> Vec<String> {
        roster
            .iter()
            .find(|agent| agent.name == name)
            .unwrap_or_else(|| panic!("{name} ships"))
            .skills
            .clone()
    };
    assert!(carried("fe").iter().any(|s| s == "ui-part-hero"));
    assert!(carried("fe").iter().any(|s| s == "ui-page-dashboard"));
    assert!(carried("fe").iter().any(|s| s == "writing"));
    assert!(carried("fe").iter().any(|s| s == "ui-themes"));
    assert!(carried("mobile").iter().any(|s| s == "ui-themes"));
    for reference in [
        "ui-reference-launch",
        "ui-reference-saas",
        "ui-reference-studio",
        "ui-reference-marketplace",
    ] {
        assert!(carried("fe").iter().any(|s| s == reference), "{reference}");
        // A reference is taken for its structure: each says so, so an
        // agent does not lift another site's words or look.
        let source = builtin_source(reference).unwrap();
        assert!(
            source.contains("Take its structure and its decisions"),
            "{reference}"
        );
    }
    // The reviewer judges interface and server work alike.
    for skill in carried("fe")
        .iter()
        .chain(carried("be").iter().filter(|s| s.starts_with("backend")))
    {
        assert!(carried("review").contains(skill), "review lacks {skill}");
    }
    assert!(carried("mobile").iter().any(|s| s == "ui-part-sidebar"));
    assert!(!carried("mobile").iter().any(|s| s == "writing"));
    for skill in [
        "docs",
        "docs-readme",
        "docs-api",
        "docs-changelog",
        "writing",
    ] {
        assert!(
            carried("docs").iter().any(|s| s == skill),
            "docs lacks {skill}"
        );
    }
    // Whoever writes text a user reads carries i18n.
    for name in ["fe", "mobile", "review", "general", "be"] {
        assert!(carried(name).iter().any(|s| s == "i18n"), "{name}");
    }
    for skill in [
        "backend",
        "backend-api",
        "backend-data",
        "backend-auth",
        "backend-stack-next",
        "backend-stack-laravel",
        "code",
        "i18n",
    ] {
        assert!(carried("be").iter().any(|s| s == skill), "be lacks {skill}");
    }
    assert!(!carried("be").iter().any(|s| s.starts_with("ui")));
    assert!(!carried("be").iter().any(|s| s.starts_with("motion")));
    // Motion: every `motion*` skill, and of `ui` only what it builds on.
    let motion_family: Vec<&str> = enowx_core::discovery::skills::builtin_names()
        .filter(|name| *name == "motion" || name.starts_with("motion-"))
        .collect();
    assert!(motion_family.len() >= 10, "{motion_family:?}");
    for skill in &motion_family {
        assert!(
            carried("motion").iter().any(|s| s == skill),
            "motion lacks {skill}"
        );
        // The interface agents carry them for the motion a component brings.
        assert!(carried("fe").iter().any(|s| s == skill), "fe lacks {skill}");
        assert!(
            carried("mobile").iter().any(|s| s == skill),
            "mobile lacks {skill}"
        );
    }
    for skill in [
        "ui",
        "ui-themes",
        "ui-stack-react",
        "ui-stack-tailwind",
        "code",
    ] {
        assert!(
            carried("motion").iter().any(|s| s == skill),
            "motion lacks {skill}"
        );
    }
    assert!(!carried("motion")
        .iter()
        .any(|s| s.starts_with("ui-part-") || s.starts_with("ui-page-")));
    assert!(!carried("motion").iter().any(|s| s == "writing"));
    // Each specialist carries its own family, its root skill first among
    // them, and `code` when it writes code.
    let family_of = |root: &str| -> Vec<String> {
        enowx_core::discovery::skills::builtin_names()
            .filter(|name| *name == root || name.starts_with(&format!("{root}-")))
            .map(str::to_owned)
            .collect()
    };
    for (agent, root, code) in [
        ("fe", "frontend", true),
        ("be", "database", true),
        ("db", "database", true),
        ("devops", "devops", true),
        ("mobile", "mobile", true),
        ("systems", "systems", true),
        ("test", "testing", true),
        ("docs", "docs", false),
        ("security", "security", false),
        ("perf", "performance", true),
        ("review", "review", true),
        ("research", "research", false),
    ] {
        let members = family_of(root);
        assert!(members.len() >= 2, "the `{root}` family ships: {members:?}");
        for skill in &members {
            assert!(carried(agent).contains(skill), "{agent} lacks {skill}");
        }
        assert_eq!(
            carried(agent).iter().any(|s| s == "code"),
            code,
            "{agent} and `code`"
        );
    }
    // The reviewer judges every kind of work.
    for root in [
        "ui",
        "motion",
        "frontend",
        "backend",
        "database",
        "devops",
        "mobile",
        "systems",
        "testing",
        "docs",
        "security",
        "performance",
    ] {
        for skill in family_of(root) {
            assert!(carried("review").contains(&skill), "review lacks {skill}");
        }
    }
    // The rest carry only what their work needs.
    assert!(!carried("db")
        .iter()
        .any(|s| s.starts_with("ui") || s.starts_with("backend")));
    assert!(!carried("devops").iter().any(|s| s.starts_with("ui")));
    assert!(carried("security").iter().any(|s| s == "backend-security"));
    assert!(carried("perf").iter().any(|s| s == "frontend-performance"));
    assert!(carried("test").iter().any(|s| s == "backend-testing"));
    assert_eq!(
        carried("orchestrator"),
        [
            "brainstorm",
            "brainstorm-prd",
            "brainstorm-design",
            "brainstorm-architecture",
            "brainstorm-erd",
            "brainstorm-api",
            "brainstorm-plan",
            "orchestration",
            "model-manager",
            "motion-video",
            "rag"
        ]
    );
    assert_eq!(carried("librarian"), ["librarian", "rag"]);
    assert!(carried("compactor").is_empty(), "compactor carries none");
    for root in [
        "frontend", "backend", "database", "devops", "testing", "security",
    ] {
        assert!(
            carried("general").iter().any(|s| s == root),
            "general lacks {root}"
        );
    }
}

/// An agent written as a file names the built-in skills it carries.
#[test]
fn an_agent_file_names_its_skills() {
    let mut front = std::collections::BTreeMap::new();
    front.insert("name".to_owned(), "landing".to_owned());
    front.insert("skills".to_owned(), "ui, Writing".to_owned());
    let agent = enowx_core::AgentDef::from_parts(&front, "Build landing pages.").unwrap();
    assert_eq!(agent.skills, ["ui", "writing"]);
    front.remove("skills");
    let bare = enowx_core::AgentDef::from_parts(&front, "Build landing pages.").unwrap();
    assert!(bare.skills.is_empty(), "none unless named");
}

/// The built-ins hold to their own rules: no em dash, and nothing a
/// generated page would say about itself.
#[test]
fn the_builtin_skills_follow_their_own_rules() {
    for name in [
        "ui",
        "ui-layout",
        "ui-audit",
        "ui-page-dashboard",
        "ui-part-hero",
        "ui-part-sidebar",
        "code",
        "writing",
        "brainstorm",
        "backend",
        "backend-api",
        "backend-data",
        "backend-stack-next",
        "motion",
        "motion-timing",
        "motion-reveal",
        "motion-interface",
        "motion-drawings",
        "motion-demo",
        "motion-performance",
        "motion-comfort",
        "motion-audit",
        "motion-stacks",
    ] {
        let source = builtin_source(name).expect("built in");
        assert!(!source.contains('—'), "`{name}` has an em dash");
        assert!(
            source.starts_with(&format!("---\nname: {name}\n")),
            "`{name}` names itself"
        );
    }
}

/// The frontend specialist is told which skill to read for which work, and
/// carries the essentials itself.
#[test]
fn the_frontend_prompt_names_its_skills_and_essentials() {
    let fe = enowx_core::builtin_agents()
        .into_iter()
        .find(|agent| agent.name == "fe")
        .expect("fe ships");
    for needed in [
        "`ui` skill",
        "`ui-layout`",
        "`ui-audit`",
        "`writing`",
        "`code`",
        "One icon set",
        "tokens",
        "empty, loading and error states",
        "Never invented",
        "DESIGN.md",
        "at most six bullets",
    ] {
        assert!(fe.prompt.contains(needed), "missing {needed:?}");
    }
    assert!(!fe.prompt.contains('—'), "the prompt has an em dash");
}

/// The backend specialist is told which skill to read for which part, and
/// carries the essentials itself.
#[test]
fn the_backend_prompt_names_its_skills_and_essentials() {
    let be = enowx_core::builtin_agents()
        .into_iter()
        .find(|agent| agent.name == "be")
        .expect("be ships");
    for needed in [
        "`backend` skill",
        "`backend-stack-*`",
        "`backend-api`",
        "`backend-auth`",
        "`backend-data`",
        "`backend-testing`",
        "`i18n`",
        "THE CONTRACT",
        "conditional update",
        "a query in a",
        ".env.example",
        "never plausible guesses",
        "at most six bullets",
    ] {
        assert!(be.prompt.contains(needed), "missing {needed:?}");
    }
    assert!(!be.prompt.contains('\u{2014}'), "the prompt has an em dash");
}

/// The motion specialist is told which skill to read for which work, and
/// carries the essentials itself: the page's content decides what moves,
/// the final state is the base, reduced motion, cost, and how to check.
#[test]
fn the_motion_prompt_names_its_skills_and_essentials() {
    let roster = enowx_core::builtin_agents();
    let motion = roster
        .iter()
        .find(|agent| agent.name == "motion")
        .expect("motion ships");
    for needed in [
        "`motion` skill",
        "`motion-timing`",
        "`motion-reveal`",
        "`motion-interface`",
        "`motion-drawings`",
        "`motion-demo`",
        "`motion-performance`",
        "`motion-comfort`",
        "`motion-audit`",
        "`motion-stacks`",
        "what each part of the page says",
        "`Motion:` line",
        "Only transform and opacity",
        "one IntersectionObserver for the page",
        "prefers-reduced-motion",
        "pause control",
        "invented number",
        "`motion: true`",
        "at most six bullets",
    ] {
        assert!(motion.prompt.contains(needed), "missing {needed:?}");
    }
    assert!(
        !motion.prompt.contains('\u{2014}'),
        "the prompt has an em dash"
    );
    assert!(motion.allows("preview") && motion.allows("edit"));
    assert_eq!(enowx_core::agent_def::roster_group("motion"), "BUILD");
    assert_eq!(enowx_core::agent_def::display_name("motion"), "Motion");
    // The orchestrator knows when to send it work, and when in a build.
    let orchestrator = roster
        .iter()
        .find(|agent| agent.name == "orchestrator")
        .unwrap();
    assert!(orchestrator.prompt.contains("Motion is `motion`'s"));
    assert!(orchestrator
        .prompt
        .contains("Motion goes on markup that exists"));
    // The frontend knows what is its and what is the motion agent's.
    let fe = roster.iter().find(|agent| agent.name == "fe").unwrap();
    assert!(fe.prompt.contains("`motion-interface`") && fe.prompt.contains("`motion` agent"));
}

/// A new page's design settles how much it moves.
#[test]
fn brainstorming_asks_how_much_motion() {
    let source = builtin_source("brainstorm").unwrap();
    assert!(source.contains("how much motion"));
}

/// Every directory under `skills/` is compiled in: a skill added there and
/// left out of the list would ship nowhere.
#[test]
fn every_skill_directory_is_built_in() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let mut on_disk: Vec<String> = std::fs::read_dir(root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut built_in: Vec<String> = enowx_core::discovery::skills::builtin_names()
        .map(str::to_owned)
        .collect();
    built_in.sort();
    assert_eq!(on_disk, built_in);
    for name in &built_in {
        let source = builtin_source(name).unwrap();
        assert!(
            source.starts_with(&format!("---\nname: {name}\n")),
            "{name} names itself"
        );
        assert!(!source.contains('\u{2014}'), "{name} has an em dash");
    }
}

/// A new project with an interface is always asked which theme it gets.
#[test]
fn brainstorming_asks_for_the_theme() {
    let source = builtin_source("brainstorm").unwrap();
    assert!(source.contains("light, dark, or both with a toggle"));
}

/// Lists show `ui` alone for the fifty-odd `ui-*` parts, so turning `ui`
/// off has to turn its parts off too, or they would still be offered with
/// no row left to turn them off from.
#[test]
fn a_part_is_turned_off_with_its_parent() {
    use enowx_core::discovery::skills::builtin_parent;
    assert_eq!(builtin_parent("ui-part-hero"), Some("ui"));
    assert_eq!(builtin_parent("ui-layout"), Some("ui"));
    assert_eq!(builtin_parent("ui-stack-next"), Some("ui"));
    assert_eq!(builtin_parent("backend-stack-next"), Some("backend"));
    assert_eq!(builtin_parent("backend-api"), Some("backend"));
    assert_eq!(builtin_parent("backend"), None);
    for parent in ["ui", "code", "writing", "brainstorming"] {
        assert_eq!(builtin_parent(parent), None, "{parent}");
    }
    assert_eq!(builtin_parent("uix"), None, "a prefix is not a parent");

    let _home = HOME.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new("family");
    let discovery = scratch.discover();
    let fe = enowx_core::builtin_agents()
        .into_iter()
        .find(|agent| agent.name == "fe")
        .unwrap();
    let offered = |disabled: &[String]| -> Vec<String> {
        discovery
            .skills_for(disabled, &fe.skills)
            .map(|skill| skill.name.clone())
            .collect()
    };
    assert!(offered(&[]).iter().any(|s| s == "ui-part-hero"));
    let without_ui = offered(&["ui".to_owned()]);
    assert!(
        !without_ui.iter().any(|s| s == "ui" || s.starts_with("ui-")),
        "{without_ui:?}"
    );
    assert!(without_ui.iter().any(|s| s == "writing"), "{without_ui:?}");
    let prompt = discovery
        .system_prompt_for(&["ui".to_owned()], Some(&fe.skills))
        .unwrap_or_default();
    assert!(!prompt.contains("`ui-part-hero`"), "{prompt}");
}

/// A family is listed by its root with its parts named on one line: a
/// hundred descriptions on every call would crowd out the task, and the root
/// says which part holds what.
#[test]
fn a_family_is_listed_by_its_root() {
    let _home = HOME.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new("family");
    let discovery = scratch.discover();
    let fe = enowx_core::builtin_agents()
        .into_iter()
        .find(|agent| agent.name == "fe")
        .expect("fe ships");
    let text = discovery
        .system_prompt_for(&[], Some(&fe.skills))
        .expect("a skill list");
    for root in ["`ui`", "`frontend`", "`motion`", "`code`"] {
        assert!(
            text.contains(&format!("- {root} ")),
            "{root} is described:\n{text}"
        );
    }
    assert!(text.contains("  its parts: "), "{text}");
    assert!(text.contains("`ui-part-hero`"), "a part is named:\n{text}");
    assert!(
        !text.contains("- `ui-part-hero` "),
        "a part is not described on its own line:\n{text}"
    );
}
