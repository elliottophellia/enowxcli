//! Dokploy, through its API (`/api/<router>.<procedure>`, an `x-api-key`
//! header): queries are GET with their input in the query string, actions
//! POST with a JSON body.

use anyhow::{bail, Context as _, Result};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};

use super::{arg, redact, render, schema, Server, ToolSpec};

pub struct Dokploy {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl Dokploy {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let base = base_url.trim().trim_end_matches('/').to_owned();
        anyhow::ensure!(
            base.starts_with("http://") || base.starts_with("https://"),
            "the Dokploy URL must start with http:// or https://"
        );
        Ok(Self {
            base: base.trim_end_matches("/api").to_owned(),
            token: token.trim().to_owned(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
        })
    }

    async fn query(&self, procedure: &str, input: &[(&str, String)]) -> Result<Value> {
        let request = self
            .http
            .request(Method::GET, format!("{}/api/{procedure}", self.base))
            .query(input);
        self.send(request).await
    }

    async fn action(&self, procedure: &str, body: Value) -> Result<Value> {
        let request = self
            .http
            .request(Method::POST, format!("{}/api/{procedure}", self.base))
            .json(&body);
        self.send(request).await
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value> {
        let response = request
            .header("x-api-key", &self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("reaching Dokploy at {}", self.base))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        if status.is_success() {
            // An action answers `true`, or nothing.
            return Ok(if body == Value::String(String::new()) {
                json!({ "ok": true })
            } else {
                body
            });
        }
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| body.to_string());
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "Dokploy refused the API key ({status}): {message}. Store a new one with `enowx mcp install dokploy`."
            ),
            _ => bail!("Dokploy answered {status}: {message}"),
        }
    }
}

/// Projects with their environments and what each runs, by id and name only.
fn summarize_projects(projects: Value) -> Value {
    let Value::Array(projects) = projects else {
        return projects;
    };
    let pick = |items: Option<&Value>, id: &str| -> Vec<Value> {
        items
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        json!({
                            id: item.get(id),
                            "name": item.get("name"),
                            "status": item.get("applicationStatus").or_else(|| item.get("composeStatus")),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let resources = |holder: &Value| {
        json!({
            "applications": pick(holder.get("applications"), "applicationId"),
            "compose": pick(holder.get("compose"), "composeId"),
            "postgres": pick(holder.get("postgres"), "postgresId"),
            "mysql": pick(holder.get("mysql"), "mysqlId"),
            "mariadb": pick(holder.get("mariadb"), "mariadbId"),
            "mongo": pick(holder.get("mongo"), "mongoId"),
            "redis": pick(holder.get("redis"), "redisId"),
        })
    };
    Value::Array(
        projects
            .iter()
            .map(|project| {
                let environments: Vec<Value> = project
                    .get("environments")
                    .and_then(Value::as_array)
                    .map(|envs| {
                        envs.iter()
                            .map(|env| {
                                let mut summary = resources(env);
                                summary["environmentId"] =
                                    env.get("environmentId").cloned().unwrap_or(Value::Null);
                                summary["name"] = env.get("name").cloned().unwrap_or(Value::Null);
                                summary
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut summary = json!({
                    "projectId": project.get("projectId"),
                    "name": project.get("name"),
                    "environments": environments,
                });
                // Older Dokploy keeps resources on the project itself.
                if project.get("applications").is_some() {
                    summary["resources"] = resources(project);
                }
                summary
            })
            .collect(),
    )
}

/// Dokploy keeps environment variables as one `KEY=VALUE` text per resource;
/// the names are shown, the values are not.
fn hide_env_values(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                if matches!(key.as_str(), "env" | "buildArgs" | "previewEnv") {
                    if let Value::String(text) = inner {
                        *text = text
                            .lines()
                            .filter_map(|line| {
                                line.split_once('=')
                                    .map(|(name, _)| format!("{name}=[hidden]"))
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                } else {
                    hide_env_values(inner);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(hide_env_values),
        _ => {}
    }
}

fn id_arg(args: &Value, key: &str) -> Result<String> {
    let id = arg(args, key)?;
    anyhow::ensure!(
        id.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "`{key}` is not a Dokploy id"
    );
    Ok(id.to_owned())
}

#[async_trait::async_trait]
impl Server for Dokploy {
    fn tools(&self) -> Vec<ToolSpec> {
        let app = [("applicationId", "The application's id, from list_projects")];
        let compose = [("composeId", "The compose stack's id, from list_projects")];
        vec![
            ToolSpec {
                name: "list_projects",
                description: "Every project with its environments, and the applications, compose stacks and databases in each: ids, names, status.",
                input_schema: schema(&[], &[]),
            },
            ToolSpec {
                name: "get_application",
                description: "One application in full: source, build, domains, status. Secrets are redacted.",
                input_schema: schema(&app, &["applicationId"]),
            },
            ToolSpec {
                name: "application_logs",
                description: "The last lines of an application's container log.",
                input_schema: schema(
                    &[
                        app[0],
                        ("tail", "How many lines, default 100"),
                        ("since", "all, or a span like 10m, 2h, 1d"),
                    ],
                    &["applicationId"],
                ),
            },
            ToolSpec {
                name: "deploy_application",
                description: "Build and deploy an application.",
                input_schema: schema(&app, &["applicationId"]),
            },
            ToolSpec {
                name: "redeploy_application",
                description: "Redeploy an application from its current source.",
                input_schema: schema(&app, &["applicationId"]),
            },
            ToolSpec {
                name: "start_application",
                description: "Start a stopped application.",
                input_schema: schema(&app, &["applicationId"]),
            },
            ToolSpec {
                name: "stop_application",
                description: "Stop a running application.",
                input_schema: schema(&app, &["applicationId"]),
            },
            ToolSpec {
                name: "get_compose",
                description: "One compose stack in full. Secrets are redacted.",
                input_schema: schema(&compose, &["composeId"]),
            },
            ToolSpec {
                name: "deploy_compose",
                description: "Deploy a compose stack.",
                input_schema: schema(&compose, &["composeId"]),
            },
            ToolSpec {
                name: "start_compose",
                description: "Start a stopped compose stack.",
                input_schema: schema(&compose, &["composeId"]),
            },
            ToolSpec {
                name: "stop_compose",
                description: "Stop a running compose stack.",
                input_schema: schema(&compose, &["composeId"]),
            },
            ToolSpec {
                name: "list_deployments",
                description: "Recent deployments of an application or a compose stack (give one of the two ids).",
                input_schema: schema(&[app[0], compose[0]], &[]),
            },
            ToolSpec {
                name: "list_servers",
                description: "The remote servers Dokploy deploys to.",
                input_schema: schema(&[], &[]),
            },
            ToolSpec {
                name: "list_containers",
                description: "The Docker containers on the Dokploy host, or on one of its servers.",
                input_schema: schema(&[("serverId", "A server's id; leave out for the Dokploy host")], &[]),
            },
        ]
    }

    async fn call(&self, tool: &str, args: &Value) -> Result<String> {
        let mut value = match tool {
            "list_projects" => summarize_projects(self.query("project.all", &[]).await?),
            "get_application" => {
                let id = id_arg(args, "applicationId")?;
                self.query("application.one", &[("applicationId", id)])
                    .await?
            }
            "application_logs" => {
                let id = id_arg(args, "applicationId")?;
                let tail = args
                    .get("tail")
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(100)
                    .clamp(1, 10_000);
                let since = args.get("since").and_then(Value::as_str).unwrap_or("all");
                let body = self
                    .query(
                        "application.readLogs",
                        &[
                            ("applicationId", id),
                            ("tail", tail.to_string()),
                            ("since", since.to_owned()),
                        ],
                    )
                    .await?;
                return Ok(body
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| render(&body)));
            }
            "deploy_application"
            | "redeploy_application"
            | "start_application"
            | "stop_application" => {
                let procedure = format!("application.{}", tool.trim_end_matches("_application"));
                self.action(
                    &procedure,
                    json!({ "applicationId": id_arg(args, "applicationId")? }),
                )
                .await?
            }
            "get_compose" => {
                let id = id_arg(args, "composeId")?;
                self.query("compose.one", &[("composeId", id)]).await?
            }
            "deploy_compose" | "start_compose" | "stop_compose" => {
                let procedure = format!("compose.{}", tool.trim_end_matches("_compose"));
                self.action(
                    &procedure,
                    json!({ "composeId": id_arg(args, "composeId")? }),
                )
                .await?
            }
            "list_deployments" => {
                if args.get("applicationId").is_some() {
                    let id = id_arg(args, "applicationId")?;
                    self.query("deployment.all", &[("applicationId", id)])
                        .await?
                } else if args.get("composeId").is_some() {
                    let id = id_arg(args, "composeId")?;
                    self.query("deployment.allByCompose", &[("composeId", id)])
                        .await?
                } else {
                    bail!("give an `applicationId` or a `composeId`");
                }
            }
            "list_servers" => self.query("server.all", &[]).await?,
            "list_containers" => {
                let mut input = Vec::new();
                if args.get("serverId").is_some() {
                    input.push(("serverId", id_arg(args, "serverId")?));
                }
                self.query("docker.getContainers", &input).await?
            }
            other => bail!("dokploy has no tool `{other}`"),
        };
        redact(&mut value);
        hide_env_values(&mut value);
        Ok(render(&value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_are_summarized_to_ids_and_names() {
        let summary = summarize_projects(json!([{
            "projectId": "p1", "name": "shop", "env": "SECRET=1",
            "environments": [{
                "environmentId": "e1", "name": "production",
                "applications": [{"applicationId": "a1", "name": "web", "applicationStatus": "done", "env": "X=1"}],
                "compose": [{"composeId": "c1", "name": "stack", "composeStatus": "idle"}]
            }]
        }]));
        let env = &summary[0]["environments"][0];
        assert_eq!(summary[0]["projectId"], "p1");
        assert_eq!(
            env["applications"][0],
            json!({"applicationId":"a1","name":"web","status":"done"})
        );
        assert_eq!(env["compose"][0]["composeId"], "c1");
        assert!(!summary.to_string().contains("SECRET"));
    }

    #[test]
    fn environment_values_are_hidden() {
        let mut app = json!({"name":"web","env":"DB_URL=postgres://u:p@h/db\nPORT=3000","buildArgs":"NPM_TOKEN=abc"});
        hide_env_values(&mut app);
        assert_eq!(app["env"], "DB_URL=[hidden]\nPORT=[hidden]");
        assert_eq!(app["buildArgs"], "NPM_TOKEN=[hidden]");
        assert_eq!(app["name"], "web");
    }

    #[test]
    fn an_id_cannot_carry_anything_else() {
        assert!(id_arg(&json!({"applicationId":"Ab_1-x"}), "applicationId").is_ok());
        assert!(id_arg(&json!({"applicationId":"a&b"}), "applicationId").is_err());
    }
}
