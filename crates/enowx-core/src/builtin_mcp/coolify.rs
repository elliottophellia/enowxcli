//! Coolify, through its REST API (`/api/v1`, a bearer token). Tested shapes
//! are Coolify 4.x; actions that moved from GET to POST in 4.2 try POST and
//! fall back to GET.

use anyhow::{bail, Context as _, Result};
use reqwest::{Method, StatusCode};
use serde_json::Value;

use super::{arg, redact, render, schema, Server, ToolSpec};

pub struct Coolify {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl Coolify {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let base = base_url.trim().trim_end_matches('/').to_owned();
        anyhow::ensure!(
            base.starts_with("http://") || base.starts_with("https://"),
            "the Coolify URL must start with http:// or https://"
        );
        Ok(Self {
            base: base.trim_end_matches("/api/v1").to_owned(),
            token: token.trim().to_owned(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
        })
    }

    async fn request(&self, method: Method, path: &str) -> Result<(StatusCode, Value)> {
        let url = format!("{}/api/v1{path}", self.base);
        let response = self
            .http
            .request(method, &url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("reaching Coolify at {}", self.base))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Ok((status, body))
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let (status, body) = self.request(Method::GET, path).await?;
        ok(status, body)
    }

    /// An action Coolify 4.2+ takes as POST and earlier versions as GET.
    async fn act(&self, path: &str) -> Result<Value> {
        let (status, body) = self.request(Method::POST, path).await?;
        if status == StatusCode::METHOD_NOT_ALLOWED || status == StatusCode::NOT_FOUND {
            let (status, body) = self.request(Method::GET, path).await?;
            return ok(status, body);
        }
        ok(status, body)
    }
}

fn ok(status: StatusCode, body: Value) -> Result<Value> {
    if status.is_success() {
        return Ok(body);
    }
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| body.to_string());
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
            "Coolify refused the token ({status}): {message}. Store a new one with `enx mcp install coolify`."
        ),
        _ => bail!("Coolify answered {status}: {message}"),
    }
}

/// The fields of each item worth listing, so a list stays readable.
fn project(list: Value, fields: &[&str]) -> Value {
    let items = match list {
        Value::Array(items) => items,
        Value::Object(ref map) if map.get("data").is_some_and(Value::is_array) => {
            map["data"].as_array().cloned().unwrap_or_default()
        }
        other => return other,
    };
    Value::Array(
        items
            .into_iter()
            .map(|item| {
                let picked: serde_json::Map<String, Value> = fields
                    .iter()
                    .filter_map(|field| item.get(*field).map(|v| ((*field).to_owned(), v.clone())))
                    .collect();
                Value::Object(picked)
            })
            .collect(),
    )
}

fn uuid_arg(args: &Value) -> Result<String> {
    let uuid = arg(args, "uuid")?;
    anyhow::ensure!(
        uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "`uuid` is not a Coolify uuid"
    );
    Ok(uuid.to_owned())
}

#[async_trait::async_trait]
impl Server for Coolify {
    fn tools(&self) -> Vec<ToolSpec> {
        let uuid = [("uuid", "The application's uuid, from list_applications")];
        vec![
            ToolSpec {
                name: "list_applications",
                description: "List the Coolify applications: uuid, name, status and domains.",
                input_schema: schema(&[], &[]),
            },
            ToolSpec {
                name: "get_application",
                description: "One application in full: build, domains, git source, status. Secrets are redacted.",
                input_schema: schema(&uuid, &["uuid"]),
            },
            ToolSpec {
                name: "application_logs",
                description: "The last lines of an application's container log.",
                input_schema: schema(
                    &[uuid[0], ("lines", "How many lines, default 100")],
                    &["uuid"],
                ),
            },
            ToolSpec {
                name: "deploy",
                description: "Deploy an application (or a service or database) by uuid. `force` rebuilds without cache.",
                input_schema: schema(
                    &[
                        ("uuid", "The resource's uuid"),
                        ("force", "\"true\" to rebuild without the build cache"),
                    ],
                    &["uuid"],
                ),
            },
            ToolSpec {
                name: "start_application",
                description: "Start a stopped application.",
                input_schema: schema(&uuid, &["uuid"]),
            },
            ToolSpec {
                name: "stop_application",
                description: "Stop a running application.",
                input_schema: schema(&uuid, &["uuid"]),
            },
            ToolSpec {
                name: "restart_application",
                description: "Restart an application without rebuilding it.",
                input_schema: schema(&uuid, &["uuid"]),
            },
            ToolSpec {
                name: "list_deployments",
                description: "An application's recent deployments: uuid, status, commit, when.",
                input_schema: schema(&[uuid[0], ("take", "How many, default 10")], &["uuid"]),
            },
            ToolSpec {
                name: "get_deployment",
                description: "One deployment with its build log.",
                input_schema: schema(&[("uuid", "The deployment's uuid")], &["uuid"]),
            },
            ToolSpec {
                name: "list_env_vars",
                description: "An application's environment variable names and scopes. Values are not shown.",
                input_schema: schema(&uuid, &["uuid"]),
            },
            ToolSpec {
                name: "list_servers",
                description: "The servers Coolify manages: uuid, name, IP, reachability.",
                input_schema: schema(&[], &[]),
            },
            ToolSpec {
                name: "list_databases",
                description: "The databases: uuid, name, type, status.",
                input_schema: schema(&[], &[]),
            },
            ToolSpec {
                name: "list_services",
                description: "The services (one-click stacks): uuid, name, status.",
                input_schema: schema(&[], &[]),
            },
            ToolSpec {
                name: "list_projects",
                description: "The projects and their environments.",
                input_schema: schema(&[], &[]),
            },
        ]
    }

    async fn call(&self, tool: &str, args: &Value) -> Result<String> {
        let number = |key: &str, default: u32| {
            args.get(key)
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .map(|n| n.clamp(1, 5000) as u32)
                .unwrap_or(default)
        };
        let value = match tool {
            "list_applications" => project(
                self.get("/applications").await?,
                &[
                    "uuid",
                    "name",
                    "status",
                    "fqdn",
                    "git_repository",
                    "git_branch",
                ],
            ),
            "get_application" => {
                self.get(&format!("/applications/{}", uuid_arg(args)?))
                    .await?
            }
            "application_logs" => {
                let body = self
                    .get(&format!(
                        "/applications/{}/logs?lines={}",
                        uuid_arg(args)?,
                        number("lines", 100)
                    ))
                    .await?;
                return Ok(body
                    .get("logs")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| render(&body)));
            }
            "deploy" => {
                let force = args
                    .get("force")
                    .map(|v| v.as_bool().unwrap_or_else(|| v.as_str() == Some("true")))
                    .unwrap_or(false);
                self.act(&format!("/deploy?uuid={}&force={force}", uuid_arg(args)?))
                    .await?
            }
            "start_application" | "stop_application" | "restart_application" => {
                let action = tool.trim_end_matches("_application");
                self.act(&format!("/applications/{}/{action}", uuid_arg(args)?))
                    .await?
            }
            "list_deployments" => project(
                self.get(&format!(
                    "/deployments/applications/{}?take={}",
                    uuid_arg(args)?,
                    number("take", 10)
                ))
                .await?,
                &[
                    "deployment_uuid",
                    "status",
                    "commit",
                    "commit_message",
                    "created_at",
                    "finished_at",
                ],
            ),
            "get_deployment" => {
                self.get(&format!("/deployments/{}", uuid_arg(args)?))
                    .await?
            }
            "list_env_vars" => project(
                self.get(&format!("/applications/{}/envs", uuid_arg(args)?))
                    .await?,
                &[
                    "key",
                    "is_preview",
                    "is_buildtime",
                    "is_runtime",
                    "is_literal",
                ],
            ),
            "list_servers" => project(
                self.get("/servers").await?,
                &["uuid", "name", "ip", "is_reachable", "is_usable"],
            ),
            "list_databases" => project(
                self.get("/databases").await?,
                &["uuid", "name", "database_type", "status"],
            ),
            "list_services" => project(
                self.get("/services").await?,
                &["uuid", "name", "status", "service_type"],
            ),
            "list_projects" => self.get("/projects").await?,
            other => bail!("coolify has no tool `{other}`"),
        };
        let mut value = value;
        redact(&mut value);
        Ok(render(&value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_list_keeps_only_the_fields_worth_reading() {
        let listed = project(
            json!([{"uuid":"a1","name":"web","status":"running","private_key_id":3,"dockerfile":"FROM x"}]),
            &["uuid", "name", "status"],
        );
        assert_eq!(
            listed,
            json!([{"uuid":"a1","name":"web","status":"running"}])
        );
    }

    #[test]
    fn a_uuid_cannot_reach_another_path() {
        assert!(uuid_arg(&json!({"uuid":"abc123"})).is_ok());
        assert!(uuid_arg(&json!({"uuid":"../servers"})).is_err());
        assert!(uuid_arg(&json!({"uuid":"a?b=c"})).is_err());
    }

    #[test]
    fn the_url_is_checked_and_trimmed() {
        assert!(Coolify::new("doki.example.com", "t").is_err());
        let c = Coolify::new("https://doki.example.com/api/v1/", "t").unwrap();
        assert_eq!(c.base, "https://doki.example.com");
    }
}
