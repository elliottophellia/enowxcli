//! The orchestrator writes the plan documents, and only those.

use enowx_core::tools::files::plan_doc_path;
use enowx_core::{ToolCtx, ToolRegistry};

#[test]
fn each_document_has_its_place() {
    assert_eq!(plan_doc_path("prd", None).unwrap(), "docs/plan/PRD.md");
    assert_eq!(plan_doc_path("design", Some("x")).unwrap(), "DESIGN.md");
    assert_eq!(
        plan_doc_path("erd", Some("Invoices & Payments")).unwrap(),
        "docs/plan/ERD-invoices-payments.md"
    );
    assert!(plan_doc_path("src/main.rs", None).is_none());
}

#[tokio::test]
async fn it_writes_a_plan_document_and_refuses_anything_else() {
    let root = std::env::temp_dir().join(format!("enx-plan-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let ctx = ToolCtx {
        workspace: root.clone(),
        shell_timeout: std::time::Duration::from_secs(5),
        cancel: tokio_util::sync::CancellationToken::new(),
        progress: None,
        call_id: String::new(),
        skills: Vec::new(),
        lsp: None,
        repair: None,
        vision: false,
        cloudflare_token: None,
    };
    let registry = ToolRegistry::default();
    let allowed = vec!["plan_write".to_owned()];
    let out = registry
        .execute_for_agent(
            &allowed,
            &ctx,
            "plan_write",
            serde_json::json!({ "doc": "prd", "content": "# Shop: PRD\n" }),
        )
        .await;
    assert!(!out.is_error, "{}", out.content);
    assert_eq!(
        std::fs::read_to_string(root.join("docs/plan/PRD.md")).unwrap(),
        "# Shop: PRD\n"
    );
    let refused = registry
        .execute_for_agent(
            &allowed,
            &ctx,
            "plan_write",
            serde_json::json!({ "doc": "../src/main", "content": "x" }),
        )
        .await;
    assert!(refused.is_error);
    let _ = std::fs::remove_dir_all(&root);
}
