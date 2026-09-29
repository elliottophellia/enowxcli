//! A written file goes to its language server and the errors come back.
//! Runs against gopls when it is installed, and skips otherwise.

use enowx_core::lsp::{Lsp, Wait};

#[tokio::test]
async fn gopls_reports_a_type_error_and_then_that_it_is_fixed() {
    if which::which("gopls").is_err() || which::which("go").is_err() {
        eprintln!("gopls is not installed; skipped");
        return;
    }
    let root = std::env::temp_dir().join(format!("enx-lsp-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("go.mod"), "module demo\n\ngo 1.22\n").unwrap();
    let file = root.join("main.go");
    let broken = "package main\n\nfunc main() {\n\tvar n int = \"four\"\n\t_ = n\n}\n";
    std::fs::write(&file, broken).unwrap();

    let lsp = Lsp::new(root.clone());
    let report = lsp.check(&file, broken, Wait::Full).await.unwrap();
    assert!(report.contains("gopls: 1 error in main.go"), "{report}");
    assert!(report.contains("main.go:4:"), "{report}");

    let fixed = "package main\n\nfunc main() {\n\tvar n int = 4\n\t_ = n\n}\n";
    std::fs::write(&file, fixed).unwrap();
    let report = lsp.check(&file, fixed, Wait::Edit).await.unwrap();
    assert!(report.contains("no errors or warnings"), "{report}");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_missing_server_is_named_once_with_its_install_command() {
    if which::which("pyright-langserver").is_ok() {
        return;
    }
    let root = std::env::temp_dir().join(format!("enx-lsp-missing-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("app.py");
    std::fs::write(&file, "x: int = 1\n").unwrap();
    let lsp = Lsp::new(root.clone());
    let first = lsp
        .check(&file, "x: int = 1\n", Wait::Edit)
        .await
        .unwrap_or_default();
    assert!(first.contains("npm install -g pyright"), "{first}");
    let second = lsp
        .check(&file, "x: int = 1\n", Wait::Edit)
        .await
        .unwrap_or_default();
    assert!(!second.contains("pyright is not installed"), "{second}");
    let _ = std::fs::remove_dir_all(&root);
}
