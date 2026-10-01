//! A skill found on disk goes to every agent until the orchestrator binds it;
//! then only the agents it is bound to are offered it.

use enowx_core::tools::skill::{SkillBindTool, SkillReadTool};
use enowx_core::tools::{Tool, ToolCtx};
use enowx_core::Discovery;
use std::sync::Arc;

fn ctx(skills: Vec<String>) -> ToolCtx {
    ToolCtx {
        workspace: std::env::temp_dir(),
        shell_timeout: std::time::Duration::from_secs(5),
        cancel: tokio_util::sync::CancellationToken::new(),
        progress: None,
        call_id: String::new(),
        skills,
        lsp: None,
        repair: None,
        vision: false,
        cloudflare_token: None,
    }
}

fn agent<'a>(d: &'a Discovery, name: &str) -> &'a enowx_core::AgentDef {
    d.agents.iter().find(|a| a.name == name).unwrap()
}

fn offered(d: &Discovery, name: &str) -> Vec<String> {
    let carried = d.carried_by(agent(d, name));
    d.skills_for(&[], &carried)
        .filter(|s| s.scope != enowx_core::SkillScope::Builtin)
        .map(|s| s.name.clone())
        .collect()
}

#[tokio::test]
async fn orchestrator_binds_a_local_skill_to_the_agents_that_need_it() {
    let root = std::env::temp_dir().join(format!("enx-bind-{}", std::process::id()));
    let workspace = root.join("ws");
    let skill = workspace.join(".agents/skills/stripe-billing");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: stripe-billing\ndescription: Stripe subscriptions and webhooks\n---\n\nbody\n",
    )
    .unwrap();
    std::fs::create_dir_all(workspace.join(".git")).unwrap();
    // Bindings are saved in the enx home: keep this test's out of the real one.
    std::env::set_var("ENX_HOME", root.join("enx"));

    let discovery = Arc::new(Discovery::run(&workspace));
    assert!(offered(&discovery, "be").contains(&"stripe-billing".to_owned()));
    assert!(offered(&discovery, "db").contains(&"stripe-billing".to_owned()));
    let block = discovery.local_skills_block(&[]).unwrap();
    assert!(block.contains("`stripe-billing` (project)"), "{block}");
    assert!(block.contains("not bound yet"), "{block}");

    let bind = SkillBindTool::new(discovery.clone());
    let out = bind
        .execute(
            &ctx(Vec::new()),
            serde_json::json!({ "skill": "stripe-billing", "agents": ["be"] }),
        )
        .await
        .unwrap();
    assert!(!out.is_error, "{}", out.content);
    assert!(offered(&discovery, "be").contains(&"stripe-billing".to_owned()));
    assert!(!offered(&discovery, "db").contains(&"stripe-billing".to_owned()));
    assert!(discovery
        .local_skills_block(&[])
        .unwrap()
        .contains("offered to: `be`"));

    // Saved, so the next session keeps it.
    let saved = std::fs::read_to_string(root.join("enx/skill-bindings.json")).unwrap();
    assert!(saved.contains("\"stripe-billing\""), "{saved}");
    let again = Discovery::run(&workspace);
    assert!(!offered(&again, "db").contains(&"stripe-billing".to_owned()));

    // An agent it is not bound to cannot read it by name either.
    let read = SkillReadTool::new(discovery.clone());
    let refused = read
        .execute(
            &ctx(discovery.carried_by(agent(&discovery, "db"))),
            serde_json::json!({ "name": "stripe-billing" }),
        )
        .await
        .unwrap();
    assert!(refused.is_error);
    let allowed = read
        .execute(
            &ctx(discovery.carried_by(agent(&discovery, "be"))),
            serde_json::json!({ "name": "stripe-billing" }),
        )
        .await
        .unwrap();
    assert!(!allowed.is_error, "{}", allowed.content);

    // Several at once, in one call.
    let both = bind
        .execute(
            &ctx(Vec::new()),
            serde_json::json!({ "bindings": [
                { "skill": "stripe-billing", "agents": ["be", "db"] },
            ] }),
        )
        .await
        .unwrap();
    assert!(!both.is_error, "{}", both.content);
    assert!(offered(&discovery, "db").contains(&"stripe-billing".to_owned()));
    let refused_all = bind
        .execute(
            &ctx(Vec::new()),
            serde_json::json!({ "bindings": [
                { "skill": "stripe-billing", "agents": ["be"] },
                { "skill": "ui", "agents": ["be"] },
            ] }),
        )
        .await
        .unwrap();
    assert!(refused_all.is_error, "one bad binding refuses the call");
    assert!(
        offered(&discovery, "db").contains(&"stripe-billing".to_owned()),
        "and changes nothing"
    );

    // Unknown agents and built-in skills are refused; an empty list unbinds.
    let unknown = bind
        .execute(
            &ctx(Vec::new()),
            serde_json::json!({ "skill": "stripe-billing", "agents": ["nobody"] }),
        )
        .await
        .unwrap();
    assert!(unknown.is_error);
    let builtin = bind
        .execute(
            &ctx(Vec::new()),
            serde_json::json!({ "skill": "ui", "agents": ["be"] }),
        )
        .await
        .unwrap();
    assert!(builtin.is_error);
    bind.execute(
        &ctx(Vec::new()),
        serde_json::json!({ "skill": "stripe-billing", "agents": [] }),
    )
    .await
    .unwrap();
    assert!(offered(&discovery, "db").contains(&"stripe-billing".to_owned()));

    let orchestrator = agent(&discovery, enowx_core::agent_def::ORCHESTRATOR);
    assert!(orchestrator.tools.iter().any(|t| t == "skill_bind"));
    let _ = std::fs::remove_dir_all(&root);
}
