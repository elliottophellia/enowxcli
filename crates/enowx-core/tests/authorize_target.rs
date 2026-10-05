//! `authorize_target` verifies control of a domain against the Cloudflare API:
//! a zone the token controls is authorized, one it does not is refused.

use enowx_core::tools::{Tool, ToolCtx, ToolOutput};
use std::time::Duration;

fn ctx(token: Option<&str>) -> ToolCtx {
    ToolCtx {
        workspace: std::env::temp_dir(),
        shell_timeout: Duration::from_secs(5),
        cancel: tokio_util::sync::CancellationToken::new(),
        progress: None,
        call_id: String::new(),
        skills: Vec::new(),
        lsp: None,
        repair: None,
        vision: false,
        cloudflare_token: token.map(str::to_owned),
    }
}

#[tokio::test]
async fn no_token_refuses_and_points_at_the_connect_command() {
    let out: ToolOutput = enowx_core::tools::authorize::AuthorizeTargetTool
        .execute(&ctx(None), serde_json::json!({"target": "mastumbas.id"}))
        .await
        .unwrap();
    assert!(out.is_error);
    assert!(out.content.contains("enowx auth login cloudflare"));
    assert!(out.content.contains("not authorized"));
}

#[tokio::test]
async fn an_empty_target_is_rejected() {
    let out = enowx_core::tools::authorize::AuthorizeTargetTool
        .execute(&ctx(Some("t")), serde_json::json!({"target": "  "}))
        .await
        .unwrap();
    assert!(out.is_error);
}
