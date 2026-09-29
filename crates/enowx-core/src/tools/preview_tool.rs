use super::{resolve_existing, Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

/// Looking at a page in headless Chrome, through `crate::preview`.
pub(super) struct PreviewTool;

/// The last look at each page, with the state of the workspace it saw.
struct Looked {
    workspace: (usize, u128),
    report: String,
    at: std::time::Instant,
}

static LOOKED: std::sync::Mutex<Option<std::collections::HashMap<String, Looked>>> =
    std::sync::Mutex::new(None);

/// How many files the workspace has and when the newest changed: enough to
/// tell whether anything a page is built from changed since the last look.
/// Ignored files (`node_modules`, build output) are left out.
fn workspace_state(root: &std::path::Path) -> (usize, u128) {
    let mut count = 0;
    let mut newest = 0;
    for entry in ignore::WalkBuilder::new(root)
        .build()
        .flatten()
        .take(20_000)
    {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                count += 1;
                if let Ok(modified) = meta.modified() {
                    let at = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    newest = newest.max(at);
                }
            }
        }
    }
    (count, newest)
}

#[async_trait]
impl Tool for PreviewTool {
    fn name(&self) -> &str {
        "preview"
    }
    fn description(&self) -> &str {
        "Open a page in headless Chrome at 360, 768 and 1440px wide and measure what a \
         person would see: horizontal overflow and what causes it, text below AA \
         contrast, drawings and icons below 3:1, links to nowhere, images without alt, \
         controls without a name, touch targets under 44px, console errors, the number \
         of h1s. A page with a second theme (dark or light) is looked at in it too, at \
         1440px. Saves a screenshot per width. Give `path` for an HTML file, or `url` for a served page with `start`, \
         the command that serves it (such as \"npm run dev\"), which is started and \
         stopped for you. For a page behind a sign-in, give `login` (the sign-in page \
         and the fields to type, with a test account from the project's seed) and it \
         signs in first; without it such a page shows only the sign-in form. With \
         `motion`, it also watches how the page moves at 1440px: a timeline of what \
         animates on load (when, how long, what moves and how far) and on scroll, loops \
         that never stop, animated layout, `transition: all`, long frames, layout shift, \
         content still hidden after scrolling through the page, scroll and wheel \
         listeners, and the page again with reduced motion. Look once the page is built \
         and again after fixing what it found: not after every edit. Looking again with \
         no file changed returns the last result. Use it before you report on anything \
         with an interface, with `motion` for anything that animates."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"An HTML file in the workspace"},
            "url":{"type":"string","description":"A page to open, such as http://localhost:3000/"},
            "start":{"type":"string","description":"With url: the command that serves it, run in the workspace and stopped afterwards"},
            "motion":{"type":"boolean","description":"Also watch how the page moves, as it is and with reduced motion (adds about 20 seconds)"},
            "login":{"type":"object","description":"For a page behind a sign-in: signed in first, in the same browser","properties":{
                "url":{"type":"string","description":"The sign-in page, such as http://localhost:3000/login"},
                "fields":{"type":"object","description":"Each field by its name, id, label or placeholder, and the value to type: {\"username\": \"admin\", \"password\": \"admin123\"}","additionalProperties":{"type":"string"}},
                "submit":{"type":"string","description":"The submit button's text, when the form has more than one button"}
            },"required":["url","fields"]}
        },"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let (target, shown) = match (args["path"].as_str(), args["url"].as_str()) {
            (Some(path), _) => {
                let file = resolve_existing(&ctx.workspace, path)?;
                (crate::preview::Target::File(file), path.to_owned())
            }
            (None, Some(url)) => (crate::preview::Target::Url(url.to_owned()), url.to_owned()),
            (None, None) => return Ok(ToolOutput::error("give a path or a url")),
        };
        // The same look with nothing changed since gives the same answer:
        // say so instead of starting a browser again.
        let key = format!(
            "{shown}|{}|{}",
            args["motion"].as_bool().unwrap_or(false),
            args["login"]["url"].as_str().unwrap_or("")
        );
        let state = workspace_state(&ctx.workspace);
        if let Some(last) = LOOKED
            .lock()
            .ok()
            .and_then(|looked| {
                looked.as_ref().and_then(|l| l.get(&key)).map(|l| {
                    (l.workspace == state).then(|| (l.report.clone(), l.at.elapsed().as_secs()))
                })
            })
            .flatten()
        {
            let (report, ago) = last;
            return Ok(ToolOutput::ok(format!(
                "No file changed since the last look at {shown} ({ago}s ago), so it would show \
                 the same. That look:\n{report}"
            )));
        }
        let out_dir =
            std::env::temp_dir().join(format!("enx-preview-{}", uuid::Uuid::new_v4().simple()));
        let login: Option<crate::preview::Login> = match args.get("login") {
            Some(value) if !value.is_null() => match serde_json::from_value(value.clone()) {
                Ok(login) => Some(login),
                Err(error) => {
                    return Ok(ToolOutput::error(format!(
                        "login needs a url and fields: {error}"
                    )))
                }
            },
            _ => None,
        };
        let preview = crate::preview::preview_with(
            &ctx.workspace,
            target,
            args["start"].as_str(),
            login.as_ref(),
            args["motion"].as_bool().unwrap_or(false),
            &out_dir,
        );
        let looked = tokio::select! {
            looked = preview => looked,
            _ = ctx.cancel.cancelled() => return Ok(ToolOutput::error("stopped")),
        };
        match looked {
            Ok((reports, motion)) => {
                let mut text = crate::preview::report(&shown, &reports);
                if let Some(motion) = &motion {
                    text.push('\n');
                    text.push_str(&crate::preview::motion_report(motion));
                }
                if let Ok(mut looked) = LOOKED.lock() {
                    looked.get_or_insert_with(Default::default).insert(
                        key,
                        Looked {
                            workspace: state,
                            report: text.clone(),
                            at: std::time::Instant::now(),
                        },
                    );
                }
                Ok(ToolOutput::ok(text))
            }
            Err(error) => Ok(ToolOutput::error(format!("{error:#}"))),
        }
    }
}
