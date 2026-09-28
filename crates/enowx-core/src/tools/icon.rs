use super::{Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

/// Real icon SVGs, by name, from Iconify's open API.
///
/// Asked to inline icons, a model typed their paths from memory: shapes that
/// were close, wrong, or not the set it named. This finds icons in the chosen
/// set by meaning and returns their exact SVG.
pub(super) struct IconTool {
    base: String,
}

impl Default for IconTool {
    fn default() -> Self {
        Self {
            base: "https://api.iconify.design".into(),
        }
    }
}

/// The most icons one call returns.
const MAX_ICONS: usize = 12;

impl IconTool {
    #[cfg(test)]
    fn with_base(base: &str) -> Self {
        Self { base: base.into() }
    }

    fn client() -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("enx")
            .build()?)
    }

    async fn search(&self, query: &str, set: Option<&str>) -> Result<ToolOutput> {
        let mut url = reqwest::Url::parse(&format!("{}/search", self.base))?;
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("limit", &MAX_ICONS.to_string());
        if let Some(set) = set.filter(|s| !s.trim().is_empty()) {
            url.query_pairs_mut().append_pair("prefix", set.trim());
        }
        let response = Self::client()?.get(url).send().await;
        let body: Value = match response {
            Ok(response) if response.status().is_success() => response.json().await?,
            Ok(response) => {
                return Ok(ToolOutput::error(format!(
                    "Iconify answered {}",
                    response.status()
                )))
            }
            Err(error) => {
                return Ok(ToolOutput::error(format!(
                    "could not reach Iconify: {error}"
                )))
            }
        };
        let names: Vec<&str> = body["icons"]
            .as_array()
            .map(|icons| {
                // Iconify returns at least 32 whatever the limit asks for.
                icons
                    .iter()
                    .filter_map(Value::as_str)
                    .take(MAX_ICONS)
                    .collect()
            })
            .unwrap_or_default();
        if names.is_empty() {
            return Ok(ToolOutput::ok(format!(
                "No icons found for \"{query}\". Try another word for the same meaning."
            )));
        }
        Ok(ToolOutput::ok(format!(
            "{}\nGet the ones you use with action \"get\".",
            names.join("\n")
        )))
    }

    async fn get(&self, names: &[String]) -> Result<ToolOutput> {
        if names.is_empty() {
            return Ok(ToolOutput::error("name the icons to get, as set:name"));
        }
        let client = Self::client()?;
        let mut out = Vec::new();
        for name in names.iter().take(MAX_ICONS) {
            let Some((set, icon)) = name.split_once(':') else {
                out.push(format!("{name}: not a set:name, such as tabler:clock"));
                continue;
            };
            let url = format!("{}/{}/{}.svg", self.base, set.trim(), icon.trim());
            let svg = match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => response.text().await?,
                Ok(response) => {
                    out.push(format!("{name}: not found ({})", response.status()));
                    continue;
                }
                Err(error) => {
                    return Ok(ToolOutput::error(format!(
                        "could not reach Iconify: {error}"
                    )))
                }
            };
            if !svg.trim_start().starts_with("<svg") {
                out.push(format!("{name}: not found"));
                continue;
            }
            out.push(format!("{name}\n{}", svg.trim()));
        }
        Ok(ToolOutput::ok(format!(
            "{}\n\nEach is 1em square and coloured with currentColor. Inline them, or as \
             <symbol>s referenced with <use>; keep the set's own stroke width.",
            out.join("\n\n")
        )))
    }
}

#[async_trait]
impl Tool for IconTool {
    fn name(&self) -> &str {
        "icon"
    }
    fn description(&self) -> &str {
        "Real icon SVGs from Iconify, so an icon is never drawn from memory. \"search\" \
         finds icons by meaning in one set (tabler, ph for Phosphor, heroicons, lucide, \
         material-symbols, ri for Remix, iconoir); \"get\" returns the exact SVG of each \
         icon named as set:name. Keep to the one set the product uses."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "action":{"type":"string","enum":["search","get"]},
            "query":{"type":"string","description":"For search: what the icon means, in English"},
            "set":{"type":"string","description":"For search: the icon set, such as tabler"},
            "names":{"type":"array","items":{"type":"string"},"description":"For get: icons as set:name"}
        },"required":["action"],"additionalProperties":false})
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        match args["action"].as_str() {
            Some("search") => {
                let query = args["query"].as_str().unwrap_or("").trim();
                if query.is_empty() {
                    return Ok(ToolOutput::error("search needs a query"));
                }
                self.search(query, args["set"].as_str()).await
            }
            Some("get") => {
                let names: Vec<String> = args["names"]
                    .as_array()
                    .map(|names| {
                        names
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                self.get(&names).await
            }
            _ => Ok(ToolOutput::error("action is search or get")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A stand-in for Iconify: a search result, one icon, and 404 otherwise.
    async fn iconify() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    let line = request.lines().next().unwrap_or("").to_owned();
                    let (status, kind, body) = if line.contains("/search?") {
                        assert!(line.contains("prefix=tabler"), "{line}");
                        (
                            "200 OK",
                            "application/json",
                            r#"{"icons":["tabler:clock","tabler:clock-hour-4"]}"#.to_owned(),
                        )
                    } else if line.contains("/tabler/clock.svg") {
                        ("200 OK", "image/svg+xml", r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" d="M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0"/></svg>"#.to_owned())
                    } else {
                        ("404 Not Found", "text/plain", "404".to_owned())
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    fn ctx() -> ToolCtx {
        ToolCtx {
            workspace: std::env::temp_dir(),
            shell_timeout: std::time::Duration::from_secs(5),
            cancel: tokio_util::sync::CancellationToken::new(),
            progress: None,
            call_id: String::new(),
            skills: Vec::new(),
        }
    }

    #[tokio::test]
    async fn icons_are_found_by_meaning_and_fetched_exactly() {
        let tool = IconTool::with_base(&iconify().await);
        let found = tool
            .execute(
                &ctx(),
                json!({"action": "search", "query": "clock", "set": "tabler"}),
            )
            .await
            .unwrap();
        assert!(
            found
                .content
                .starts_with("tabler:clock\ntabler:clock-hour-4"),
            "{}",
            found.content
        );
        let got = tool
            .execute(
                &ctx(),
                json!({"action": "get", "names": ["tabler:clock", "tabler:nope", "clock"]}),
            )
            .await
            .unwrap();
        assert!(!got.is_error);
        assert!(
            got.content.contains("tabler:clock\n<svg"),
            "{}",
            got.content
        );
        assert!(got.content.contains("stroke=\"currentColor\""));
        assert!(got.content.contains("tabler:nope: not found"));
        assert!(got.content.contains("clock: not a set:name"));
    }
}
