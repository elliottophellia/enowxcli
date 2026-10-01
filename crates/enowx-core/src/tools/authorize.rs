//! `authorize_target`: prove control of a domain through Cloudflare before
//! any authorized testing of it.
//!
//! The difference between a penetration test and an attack is authorization,
//! and a claim typed in a chat is not authorization. This tool asks for
//! verifiable evidence instead: a Cloudflare API token that controls the
//! domain's DNS zone. Holding the zone in your Cloudflare account means you
//! control the domain, the same proof a certificate authority or a bug-bounty
//! program accepts. The tool calls the Cloudflare API, confirms the token
//! controls a zone the target falls under, resolves the host's addresses, and
//! records the verified scope so the security team may test it. A target the
//! token does not control is refused. It never authorizes anything
//! destructive, and it tests nothing itself.

use super::{Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct AuthorizeTargetTool;

/// A domain verified for this process, with the evidence behind it.
#[derive(Debug, Clone)]
pub struct Authorized {
    pub host: String,
    pub zone: String,
    pub zone_id: String,
    pub ips: Vec<String>,
}

/// The domains verified in this process. Written only by a successful
/// Cloudflare check, read by whoever needs to know a host is in scope.
static AUTHORIZED: std::sync::Mutex<Vec<Authorized>> = std::sync::Mutex::new(Vec::new());

/// Record a verified domain.
fn remember(auth: Authorized) {
    if let Ok(mut all) = AUTHORIZED.lock() {
        all.retain(|a| a.host != auth.host);
        all.push(auth);
    }
}

/// The verified scope so far, for a report or a gate.
pub fn authorized_scope() -> Vec<Authorized> {
    AUTHORIZED.lock().map(|a| a.clone()).unwrap_or_default()
}

#[cfg(test)]
pub fn clear_authorized() {
    if let Ok(mut all) = AUTHORIZED.lock() {
        all.clear();
    }
}

/// The host part of a target, lowercased, without scheme, port or path.
fn host_of(target: &str) -> String {
    let t = target.trim();
    let t = t.rsplit("://").next().unwrap_or(t);
    let t = t.split('/').next().unwrap_or(t);
    let t = t.rsplit('@').next().unwrap_or(t);
    // Strip a port, but keep an IPv6 literal's colons inside brackets.
    let t = if let Some(rest) = t.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        t.split(':').next().unwrap_or(t)
    };
    t.trim_end_matches('.').to_ascii_lowercase()
}

/// The zone from `zones` that `host` falls under: the zone itself, or a
/// subdomain of it. The most specific match wins, so a host under a
/// delegated subzone is attributed to that subzone.
pub fn zone_for<'a>(host: &str, zones: &'a [(String, String)]) -> Option<&'a (String, String)> {
    zones
        .iter()
        .filter(|(name, _)| host == name || host.ends_with(&format!(".{name}")))
        .max_by_key(|(name, _)| name.len())
}

#[async_trait]
impl Tool for AuthorizeTargetTool {
    fn name(&self) -> &str {
        "authorize_target"
    }
    fn description(&self) -> &str {
        "Prove you are authorized to test a domain, before any active testing of it, by \
         verifying a connected Cloudflare account controls its DNS zone. Controlling the zone \
         is proof you control the domain. Give the target (a domain or URL); the tool checks \
         the Cloudflare API, and on success records the domain, its subdomains and the \
         addresses it resolves to as the verified scope the security team may test. A domain \
         the token does not control is refused. This authorizes non-destructive testing of \
         what you demonstrably own; it never authorizes anything that damages a service."
    }
    fn parameters(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "target":{"type":"string","description":"The domain or URL to authorize, e.g. example.com or https://app.example.com"}
            },
            "required":["target"],
            "additionalProperties":false
        })
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let target = args["target"].as_str().unwrap_or("").trim();
        if target.is_empty() {
            return Ok(ToolOutput::error("give a `target`: a domain or URL"));
        }
        let host = host_of(target);
        if host.is_empty() {
            return Ok(ToolOutput::error(format!(
                "could not read a host from `{target}`"
            )));
        }
        let Some(token) = ctx.cloudflare_token.clone() else {
            return Ok(ToolOutput::error(
                "no Cloudflare account is connected, so control of this domain cannot be \
                 verified and testing it is not authorized. Connect one with `enx auth login \
                 cloudflare` (a token with at least Zone:Read over the zones you want to \
                 test), then try again. With no verifiable proof of control, decline the \
                 request and offer to audit the source code or test a local copy instead.",
            ));
        };

        let zones = match fetch_zones(&token, &ctx.cancel).await {
            Ok(zones) => zones,
            Err(error) => {
                return Ok(ToolOutput::error(format!(
                    "could not check the Cloudflare account ({error}). Without a successful \
                     check the domain is not authorized; do not test it."
                )))
            }
        };
        if zones.is_empty() {
            return Ok(ToolOutput::error(
                "the Cloudflare token reads no zones, so it proves control of no domain. Use \
                 a token with Zone:Read over the zone you want to test.",
            ));
        }

        let Some((zone, zone_id)) = zone_for(&host, &zones).cloned() else {
            let names: Vec<&str> = zones.iter().map(|(n, _)| n.as_str()).take(10).collect();
            return Ok(ToolOutput::error(format!(
                "the connected Cloudflare account does not control `{host}`. It controls: {}. \
                 Testing a domain you have not shown control of is not authorized, so decline \
                 and offer a source-code audit or a local copy instead.",
                names.join(", ")
            )));
        };

        let ips = resolve(&host).await;
        remember(Authorized {
            host: host.clone(),
            zone: zone.clone(),
            zone_id: zone_id.clone(),
            ips: ips.clone(),
        });

        let mut out = format!(
            "Authorized. The connected Cloudflare account controls the zone `{zone}` (id \
             {zone_id}), which proves control of `{host}`. Verified scope for this \
             assessment: `{host}` and its subdomains"
        );
        if ips.is_empty() {
            out.push_str(".\nThe host did not resolve to an address just now; test it by name.");
        } else {
            out.push_str(&format!(
                ", resolving to {}.\nThose addresses are in scope, but an address can be \
                 shared hosting: if one serves a site that is not this domain's, it is not \
                 authorized, so test by hostname and confirm the server is this domain's \
                 before testing it by address.",
                ips.join(", ")
            ));
        }
        out.push_str(
            "\nThis authorizes non-destructive testing only: no payload that destroys or \
             alters data, no denial of service, nothing that degrades the service. Stay \
             within this scope.",
        );
        Ok(ToolOutput::ok(out))
    }
}

/// Zone names the token can read, each with its id, across every page.
async fn fetch_zones(
    token: &str,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Vec<(String, String)>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let mut zones = Vec::new();
    let mut page = 1;
    loop {
        let request = client
            .get("https://api.cloudflare.com/client/v4/zones")
            .query(&[("per_page", "50"), ("page", &page.to_string())])
            .bearer_auth(token)
            .send();
        let response = tokio::select! {
            r = request => r?,
            _ = cancel.cancelled() => anyhow::bail!("cancelled"),
        };
        let status = response.status();
        let body: Value = response.json().await?;
        if !status.is_success() || body["success"] == Value::Bool(false) {
            let message = body["errors"][0]["message"]
                .as_str()
                .unwrap_or("the Cloudflare API refused the token");
            anyhow::bail!("{message}");
        }
        let result = body["result"].as_array().cloned().unwrap_or_default();
        for zone in &result {
            if let (Some(name), Some(id)) = (zone["name"].as_str(), zone["id"].as_str()) {
                zones.push((name.to_ascii_lowercase(), id.to_owned()));
            }
        }
        let total = body["result_info"]["total_pages"].as_u64().unwrap_or(1);
        if page as u64 >= total || result.is_empty() {
            break;
        }
        page += 1;
    }
    Ok(zones)
}

/// The addresses a host resolves to now, best effort.
async fn resolve(host: &str) -> Vec<String> {
    tokio::net::lookup_host(format!("{host}:0"))
        .await
        .map(|addrs| {
            let mut ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
            ips.sort();
            ips.dedup();
            ips
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_read_from_any_target_shape() {
        assert_eq!(
            host_of("https://app.example.com/path?q=1"),
            "app.example.com"
        );
        assert_eq!(host_of("example.com"), "example.com");
        assert_eq!(host_of("http://example.com:8080"), "example.com");
        assert_eq!(host_of("user@example.com"), "example.com");
        assert_eq!(host_of("EXAMPLE.com."), "example.com");
    }

    #[test]
    fn a_zone_matches_the_domain_and_its_subdomains_most_specific_first() {
        let zones = vec![
            ("example.com".to_owned(), "z1".to_owned()),
            ("sub.example.com".to_owned(), "z2".to_owned()),
            ("other.org".to_owned(), "z3".to_owned()),
        ];
        assert_eq!(zone_for("example.com", &zones).unwrap().1, "z1");
        assert_eq!(zone_for("api.example.com", &zones).unwrap().1, "z1");
        // A delegated subzone wins over its parent.
        assert_eq!(zone_for("a.sub.example.com", &zones).unwrap().1, "z2");
        assert_eq!(zone_for("other.org", &zones).unwrap().1, "z3");
        // Not controlled, and no partial-label match.
        assert!(zone_for("notexample.com", &zones).is_none());
        assert!(zone_for("example.com.evil.com", &zones).is_none());
        assert!(zone_for("elsewhere.net", &zones).is_none());
    }

    #[tokio::test]
    async fn without_a_token_it_refuses_and_says_how_to_connect() {
        clear_authorized();
        let ctx = ToolCtx {
            workspace: std::env::temp_dir(),
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
        let out = AuthorizeTargetTool
            .execute(&ctx, json!({"target": "mastumbas.id"}))
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(
            out.content.contains("enx auth login cloudflare"),
            "{}",
            out.content
        );
        assert!(authorized_scope().is_empty());
    }
}
