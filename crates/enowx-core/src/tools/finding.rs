//! `report_finding`: one confirmed security finding, recorded in the same
//! shape every time.
//!
//! A pentest turns up findings across several specialists, and a report is
//! only as good as the consistency of what goes into it. This tool takes the
//! parts of a finding — what it is, how bad, where, how to reproduce it, what
//! it lets an attacker do, and how to fix it — checks the severity and that
//! the parts that make a finding actionable are present, and returns it as one
//! block. The block flows into the agent's report, and the security lead
//! consolidates the blocks its specialists hand back. Nothing is stored or
//! sent anywhere: the tool shapes a finding, it does not run anything.

use super::{Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct ReportFindingTool;

/// The severities a finding may carry, worst first.
const SEVERITIES: &[&str] = &["critical", "high", "medium", "low", "info"];

#[async_trait]
impl Tool for ReportFindingTool {
    fn name(&self) -> &str {
        "report_finding"
    }
    fn description(&self) -> &str {
        "Record one confirmed security finding in a consistent shape, during an authorized \
         assessment. Report a finding only once you have confirmed it: a suspicion is not a \
         vulnerability. Give the class of problem and where it is, not a working exploit, and \
         never a real secret's value (report it by its place and kind). The finding is \
         returned formatted for the report; nothing is stored or sent."
    }
    fn parameters(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"Short name of the issue, e.g. \"Reflected XSS in search\""},
                "severity":{"type":"string","enum":SEVERITIES,"description":"Ranked by what an attacker gains and how reachable the path is, not by a scanner score"},
                "location":{"type":"string","description":"Where it is: URL, endpoint, parameter, or file and line"},
                "cwe":{"type":"string","description":"CWE id if known, e.g. CWE-79 (optional)"},
                "reproduction":{"type":"string","description":"The exact steps to reproduce it, with the benign proof used to confirm it"},
                "impact":{"type":"string","description":"What an attacker can actually do with it"},
                "remediation":{"type":"string","description":"A specific, implementable fix"},
                "confidence":{"type":"string","enum":["confirmed","firm","tentative"],"description":"How sure you are (default confirmed)"}
            },
            "required":["title","severity","location","reproduction","impact","remediation"],
            "additionalProperties":false
        })
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let get = |key: &str| args[key].as_str().unwrap_or("").trim().to_owned();
        let title = get("title");
        let severity = get("severity").to_ascii_lowercase();
        let location = get("location");
        let reproduction = get("reproduction");
        let impact = get("impact");
        let remediation = get("remediation");

        for (key, value) in [
            ("title", &title),
            ("severity", &severity),
            ("location", &location),
            ("reproduction", &reproduction),
            ("impact", &impact),
            ("remediation", &remediation),
        ] {
            if value.is_empty() {
                return Ok(ToolOutput::error(format!(
                    "a finding needs `{key}`: {} are all required",
                    "title, severity, location, reproduction, impact and remediation"
                )));
            }
        }
        if !SEVERITIES.contains(&severity.as_str()) {
            return Ok(ToolOutput::error(format!(
                "severity must be one of {}",
                SEVERITIES.join(", ")
            )));
        }
        let confidence = match get("confidence").to_ascii_lowercase().as_str() {
            "" | "confirmed" => "confirmed",
            "firm" => "firm",
            "tentative" => "tentative",
            other => {
                return Ok(ToolOutput::error(format!(
                    "confidence must be confirmed, firm or tentative, not `{other}`"
                )))
            }
        };

        let cwe = get("cwe");
        let mut block = format!(
            "[{}] {title}\n  Where: {location}\n",
            severity.to_ascii_uppercase()
        );
        if !cwe.is_empty() {
            block.push_str(&format!("  CWE: {cwe}\n"));
        }
        block.push_str(&format!(
            "  Reproduce: {reproduction}\n  Impact: {impact}\n  Fix: {remediation}\n  \
             Confidence: {confidence}"
        ));
        Ok(ToolOutput::ok(block))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ToolCtx {
        ToolCtx {
            workspace: std::env::temp_dir(),
            shell_timeout: std::time::Duration::from_secs(5),
            cancel: tokio_util::sync::CancellationToken::new(),
            progress: None,
            call_id: String::new(),
            skills: Vec::new(),
            lsp: None,
            repair: None,
            vision: false,
        }
    }

    #[tokio::test]
    async fn a_finding_is_formatted_and_checked() {
        let tool = ReportFindingTool;
        let out = tool
            .execute(
                &ctx(),
                json!({
                    "title":"Reflected XSS in search",
                    "severity":"high",
                    "location":"/search?q=",
                    "cwe":"CWE-79",
                    "reproduction":"Send q=<svg onload=...>; it renders unescaped.",
                    "impact":"Run script in a victim's session.",
                    "remediation":"Contextually encode output; set CSP."
                }),
            )
            .await
            .unwrap();
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("[HIGH] Reflected XSS"));
        assert!(out.content.contains("CWE-79"));
        assert!(out.content.contains("Confidence: confirmed"));

        let missing = tool
            .execute(
                &ctx(),
                json!({"title":"x","severity":"high","location":"/","reproduction":"r","impact":"i"}),
            )
            .await
            .unwrap();
        assert!(missing.is_error, "remediation is required");

        let bad = tool
            .execute(
                &ctx(),
                json!({"title":"x","severity":"spicy","location":"/","reproduction":"r","impact":"i","remediation":"f"}),
            )
            .await
            .unwrap();
        assert!(bad.is_error, "severity must be from the set");
    }
}
