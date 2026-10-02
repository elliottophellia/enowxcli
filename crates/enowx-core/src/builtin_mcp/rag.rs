//! Retrieval over the project's own code: chunks embedded with Voyage AI and
//! kept in Postgres with pgvector, local or in the cloud.
//!
//! Indexing walks the workspace the way git sees it (`.gitignore` honoured,
//! `.env` files never read), cuts each file into chunks of whole lines that
//! remember their line range, and embeds only the chunks whose content
//! changed since the last run. Chunks of files that were removed, or of the
//! tail of a file that got shorter, are deleted. Search combines the vector
//! match with a lexical one (reciprocal rank fusion) and reranks the
//! candidates with Voyage's reranker.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::{schema, Server, ToolSpec};

/// The embedding model: Voyage's model for code, at its default width.
pub const MODEL: &str = "voyage-code-3";
pub const DIM: usize = 1024;
/// The reranker run over the candidates.
pub const RERANK_MODEL: &str = "rerank-2.5";

const VOYAGE: &str = "https://api.voyageai.com/v1";
/// Characters per chunk, in whole lines.
const CHUNK: usize = 1500;
/// Files larger than this are not read.
const MAX_FILE: u64 = 512 * 1024;
/// Chunks per embedding request.
const BATCH: usize = 64;
/// Candidates fetched before reranking.
const RECALL: i64 = 40;

/// The non-secret half of the setup. The DSN and the Voyage key are secrets
/// and live in `auth.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RagSetup {
    /// The embedding model; `voyage-code-3` when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
}

/// The table holding every project's chunks, named by vector width so a
/// change of model never lands in a column of the wrong size.
fn table() -> String {
    format!("enx_rag_chunks_{DIM}")
}

pub struct Rag {
    dsn: String,
    voyage_key: String,
    model: String,
    http: reqwest::Client,
    db: Mutex<Option<Arc<tokio_postgres::Client>>>,
    /// The folder indexed when a call names none.
    workspace: PathBuf,
}

impl Rag {
    pub fn new(dsn: &str, voyage_key: &str, setup: &RagSetup) -> Result<Self> {
        anyhow::ensure!(
            dsn.starts_with("postgres://") || dsn.starts_with("postgresql://"),
            "the database must be a postgres:// connection string"
        );
        let workspace = std::env::var_os("ENX_WORKSPACE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        Ok(Self {
            dsn: dsn.trim().to_owned(),
            voyage_key: voyage_key.trim().to_owned(),
            model: if setup.model.trim().is_empty() {
                MODEL.to_owned()
            } else {
                setup.model.trim().to_owned()
            },
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()?,
            db: Mutex::new(None),
            workspace,
        })
    }

    /// A connection, opened once and reopened after it drops. TLS follows
    /// the DSN's `sslmode`: `prefer` (the default) uses it when the server
    /// offers it, `require` insists, `disable` never tries.
    async fn db(&self) -> Result<Arc<tokio_postgres::Client>> {
        let mut slot = self.db.lock().await;
        if let Some(client) = slot.as_ref() {
            if !client.is_closed() {
                return Ok(client.clone());
            }
        }
        let config: tokio_postgres::Config =
            self.dsn.parse().context("the database connection string")?;
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connector = tokio_postgres_rustls::MakeRustlsConnect::new(tls);
        let (client, connection) =
            tokio::time::timeout(Duration::from_secs(20), config.connect(connector))
                .await
                .context("no answer from the database within 20 seconds")?
                .context("connecting to the database")?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let client = Arc::new(client);
        prepare(&client).await?;
        *slot = Some(client.clone());
        Ok(client)
    }

    /// Embed `inputs` as documents or as a query.
    async fn embed(&self, inputs: &[String], query: bool) -> Result<Vec<Vec<f32>>> {
        #[derive(Deserialize)]
        struct Item {
            embedding: Vec<f32>,
            index: usize,
        }
        #[derive(Deserialize)]
        struct Reply {
            data: Vec<Item>,
        }
        let response = self
            .http
            .post(format!("{VOYAGE}/embeddings"))
            .bearer_auth(&self.voyage_key)
            .json(&json!({
                "input": inputs,
                "model": self.model,
                "input_type": if query { "query" } else { "document" },
                "output_dimension": DIM,
            }))
            .send()
            .await
            .context("reaching Voyage AI")?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            if status.as_u16() == 401 || status.as_u16() == 403 {
                bail!("Voyage AI refused the API key ({status}); set it again with `c` on rag in /mcp");
            }
            bail!(
                "Voyage AI answered {status}: {}",
                body.chars().take(300).collect::<String>()
            );
        }
        let reply: Reply = serde_json::from_str(&body).context("reading Voyage AI's reply")?;
        let mut vectors = vec![Vec::new(); inputs.len()];
        for item in reply.data {
            if let Some(slot) = vectors.get_mut(item.index) {
                *slot = item.embedding;
            }
        }
        anyhow::ensure!(
            vectors.iter().all(|v| v.len() == DIM),
            "Voyage AI returned vectors of the wrong width"
        );
        Ok(vectors)
    }

    /// The candidates in the reranker's order, or `None` when it fails (the
    /// fused order is used then).
    async fn rerank(
        &self,
        query: &str,
        documents: &[String],
        top: usize,
    ) -> Option<Vec<(usize, f32)>> {
        #[derive(Deserialize)]
        struct Item {
            index: usize,
            relevance_score: f32,
        }
        #[derive(Deserialize)]
        struct Reply {
            data: Vec<Item>,
        }
        let response = self
            .http
            .post(format!("{VOYAGE}/rerank"))
            .bearer_auth(&self.voyage_key)
            .json(&json!({
                "query": query,
                "documents": documents,
                "model": RERANK_MODEL,
                "top_k": top,
            }))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let reply: Reply = response.json().await.ok()?;
        Some(
            reply
                .data
                .into_iter()
                .map(|i| (i.index, i.relevance_score))
                .collect(),
        )
    }

    fn root(&self, args: &Value) -> Result<PathBuf> {
        let root = match args
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
        {
            Some(path) => PathBuf::from(path),
            None => self.workspace.clone(),
        };
        let root = std::fs::canonicalize(&root)
            .with_context(|| format!("no folder at {}", root.display()))?;
        anyhow::ensure!(root.is_dir(), "{} is not a folder", root.display());
        Ok(root)
    }

    async fn index(&self, args: &Value) -> Result<String> {
        let root = self.root(args)?;
        let project = project_id(&root);
        let db = self.db().await?;
        let chunks = tokio::task::spawn_blocking({
            let root = root.clone();
            move || collect(&root)
        })
        .await??;
        let files = chunks
            .iter()
            .map(|c| c.file.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len();

        let existing: HashMap<String, String> = db
            .query(
                &format!(
                    "SELECT id, content_hash FROM {} WHERE project_id = $1",
                    table()
                ),
                &[&project],
            )
            .await?
            .into_iter()
            .map(|row| -> Result<(String, String)> { Ok((row.try_get(0)?, row.try_get(1)?)) })
            .collect::<Result<_>>()?;

        let wanted: std::collections::HashSet<&str> =
            chunks.iter().map(|c| c.id.as_str()).collect();
        let changed: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| existing.get(&c.id) != Some(&c.hash))
            .collect();

        let mut embedded = 0;
        for batch in changed.chunks(BATCH) {
            let inputs: Vec<String> = batch.iter().map(|c| c.text()).collect();
            let vectors = self.embed(&inputs, false).await?;
            for (chunk, vector) in batch.iter().zip(vectors) {
                db.execute(
                    &format!(
                        "INSERT INTO {} (project_id, id, source_file, start_line, end_line, content, content_hash, embedding, indexed_at)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8::text::vector, now())
                         ON CONFLICT (project_id, id) DO UPDATE SET
                           source_file = EXCLUDED.source_file, start_line = EXCLUDED.start_line,
                           end_line = EXCLUDED.end_line, content = EXCLUDED.content,
                           content_hash = EXCLUDED.content_hash, embedding = EXCLUDED.embedding,
                           indexed_at = now()",
                        table()
                    ),
                    &[
                        &project,
                        &chunk.id,
                        &chunk.file,
                        &(chunk.start as i32),
                        &(chunk.end as i32),
                        &chunk.text(),
                        &chunk.hash,
                        &vector_literal(&vector),
                    ],
                )
                .await?;
                embedded += 1;
            }
        }

        // Chunks no file produces any more: removed files, and the tail of a
        // file that got shorter.
        let stale: Vec<String> = existing
            .keys()
            .filter(|id| !wanted.contains(id.as_str()))
            .cloned()
            .collect();
        if !stale.is_empty() {
            db.execute(
                &format!(
                    "DELETE FROM {} WHERE project_id = $1 AND id = ANY($2)",
                    table()
                ),
                &[&project, &stale],
            )
            .await?;
        }
        Ok(format!(
            "Indexed {} ({project}): {files} files, {} chunks; embedded {embedded} new or changed, removed {} stale, {} unchanged.",
            root.display(),
            chunks.len(),
            stale.len(),
            chunks.len() - embedded,
        ))
    }

    async fn search(&self, args: &Value) -> Result<String> {
        let query = super::arg(args, "query")?.to_owned();
        let limit = args
            .get("limit")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(8)
            .clamp(1, 30) as usize;
        let root = self.root(args)?;
        let project = project_id(&root);
        let db = self.db().await?;
        let vector = self
            .embed(std::slice::from_ref(&query), true)
            .await?
            .pop()
            .context("no embedding for the query")?;
        let rows = db
            .query(
                &format!(
                    "WITH dense AS (
                       SELECT id, row_number() OVER (ORDER BY embedding <=> $1::text::vector) AS rank
                       FROM {t} WHERE project_id = $2
                       ORDER BY embedding <=> $1::text::vector LIMIT $3),
                     lexical AS (
                       SELECT id, row_number() OVER (ORDER BY ts_rank(content_tsv, plainto_tsquery('simple', $4)) DESC) AS rank
                       FROM {t} WHERE project_id = $2 AND content_tsv @@ plainto_tsquery('simple', $4)
                       LIMIT $3)
                     SELECT c.source_file, c.start_line, c.end_line, c.content,
                            (COALESCE(1.0 / (60 + d.rank), 0) + COALESCE(1.0 / (60 + l.rank), 0))::float8 AS score
                     FROM dense d FULL OUTER JOIN lexical l ON d.id = l.id
                     JOIN {t} c ON c.project_id = $2 AND c.id = COALESCE(d.id, l.id)
                     ORDER BY score DESC LIMIT $3",
                    t = table()
                ),
                &[&vector_literal(&vector), &project, &RECALL, &query],
            )
            .await?;
        if rows.is_empty() {
            let indexed: i64 = db
                .query_one(
                    &format!("SELECT count(*) FROM {} WHERE project_id = $1", table()),
                    &[&project],
                )
                .await?
                .try_get(0)?;
            return Ok(if indexed == 0 {
                format!("{} is not indexed yet. Call `index` first.", root.display())
            } else {
                format!("Nothing in {} matches that.", root.display())
            });
        }
        // Read with `try_get`: a column of an unexpected type is an error the
        // caller sees, not a panic that takes the server down.
        let hits: Vec<(String, i32, i32, String, f64)> = rows
            .into_iter()
            .map(|r| -> Result<_> {
                Ok((
                    r.try_get(0)?,
                    r.try_get(1)?,
                    r.try_get(2)?,
                    r.try_get(3)?,
                    r.try_get(4)?,
                ))
            })
            .collect::<Result<_>>()?;
        let documents: Vec<String> = hits.iter().map(|h| h.3.clone()).collect();
        let order: Vec<(usize, f32)> = match self.rerank(&query, &documents, limit).await {
            Some(order) => order,
            None => (0..hits.len().min(limit))
                .map(|i| (i, hits[i].4 as f32))
                .collect(),
        };
        let mut out = String::new();
        for (index, score) in order {
            let Some((file, start, end, content, _)) = hits.get(index) else {
                continue;
            };
            // The stored text leads with "File: …" for the embedding; the
            // reader gets the location as a heading instead.
            let body = content
                .split_once("\n\n")
                .map_or(content.as_str(), |(_, b)| b);
            out.push_str(&format!(
                "## {file}:{start}-{end}  (score {score:.3})\n```\n{}\n```\n\n",
                body.trim_end()
            ));
        }
        Ok(out.trim_end().to_owned())
    }

    async fn status(&self, args: &Value) -> Result<String> {
        let root = self.root(args)?;
        let project = project_id(&root);
        let db = self.db().await?;
        let row = db
            .query_one(
                &format!(
                    "SELECT count(*), count(DISTINCT source_file), max(indexed_at)::text FROM {} WHERE project_id = $1",
                    table()
                ),
                &[&project],
            )
            .await?;
        let (chunks, files, last): (i64, i64, Option<String>) =
            (row.try_get(0)?, row.try_get(1)?, row.try_get(2)?);
        Ok(render(&json!({
            "folder": root.display().to_string(),
            "project": project,
            "chunks": chunks,
            "files": files,
            "last_indexed": last,
            "model": self.model,
            "reranker": RERANK_MODEL,
        })))
    }

    async fn forget(&self, args: &Value) -> Result<String> {
        let root = self.root(args)?;
        let project = project_id(&root);
        let db = self.db().await?;
        let removed = db
            .execute(
                &format!("DELETE FROM {} WHERE project_id = $1", table()),
                &[&project],
            )
            .await?;
        Ok(format!("Removed {removed} chunks of {}.", root.display()))
    }
}

fn render(value: &Value) -> String {
    super::render(value)
}

/// The vector extension, the table and its indexes, created when missing. A
/// managed database where the extension already exists but this role may
/// not create it is fine.
async fn prepare(db: &tokio_postgres::Client) -> Result<()> {
    if let Err(error) = db
        .batch_execute("CREATE EXTENSION IF NOT EXISTS vector")
        .await
    {
        let present = db
            .query_opt("SELECT 1 FROM pg_extension WHERE extname = 'vector'", &[])
            .await?
            .is_some();
        if !present {
            bail!(
                "pgvector is not installed in this database and this role cannot add it: {error}"
            );
        }
    }
    let t = table();
    db.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {t} (
           project_id TEXT NOT NULL,
           id TEXT NOT NULL,
           source_file TEXT NOT NULL,
           start_line INTEGER NOT NULL,
           end_line INTEGER NOT NULL,
           content TEXT NOT NULL,
           content_hash TEXT NOT NULL,
           embedding vector({DIM}) NOT NULL,
           content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple', content)) STORED,
           indexed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
           PRIMARY KEY (project_id, id));
         CREATE INDEX IF NOT EXISTS {t}_project ON {t} (project_id);
         CREATE INDEX IF NOT EXISTS {t}_embedding ON {t} USING hnsw (embedding vector_cosine_ops);
         CREATE INDEX IF NOT EXISTS {t}_tsv ON {t} USING GIN (content_tsv);"
    ))
    .await
    .context("creating the RAG table")?;
    Ok(())
}

/// A vector as pgvector's text form, every float at full precision.
fn vector_literal(vector: &[f32]) -> String {
    let mut out = String::with_capacity(vector.len() * 12);
    out.push('[');
    for (i, value) in vector.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

/// The id a folder is indexed under: its name, and a short hash of its full
/// path so two checkouts named alike do not share chunks.
pub fn project_id(root: &Path) -> String {
    let name: String = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "root".into())
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{name}-{:08x}", stable_hash(&root.to_string_lossy()) as u32)
}

/// A hash that is the same on every run and every machine.
fn stable_hash(text: &str) -> u64 {
    // FNV-1a: tiny, stable, and change detection needs nothing stronger.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// One chunk of one file.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub id: String,
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub body: String,
    pub hash: String,
}

impl Chunk {
    /// What is embedded and stored: the location, then the lines.
    fn text(&self) -> String {
        format!(
            "File: {} (lines {}-{})\n\n{}",
            self.file, self.start, self.end, self.body
        )
    }
}

/// Files worth indexing: source, config and prose. Lockfiles, minified
/// bundles and anything that looks like a secret are left out.
fn wanted(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.starts_with(".env")
        || name.ends_with(".lock")
        || name == "package-lock.json"
        || name == "pnpm-lock.yaml"
        || name.contains(".min.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
    {
        return false;
    }
    if matches!(
        name.as_str(),
        "dockerfile" | "makefile" | "justfile" | "readme"
    ) {
        return true;
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "rs" | "go"
            | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "vue"
            | "svelte"
            | "astro"
            | "java"
            | "kt"
            | "kts"
            | "swift"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "hpp"
            | "cs"
            | "rb"
            | "php"
            | "scala"
            | "dart"
            | "lua"
            | "ex"
            | "exs"
            | "erl"
            | "hs"
            | "ml"
            | "clj"
            | "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "html"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
            | "xml"
            | "proto"
            | "graphql"
            | "gql"
            | "tf"
            | "md"
            | "mdx"
            | "txt"
            | "rst"
    )
}

/// Every chunk of every wanted file under `root`, in the order git would see
/// the files (`.gitignore` and hidden files skipped).
pub fn collect(root: &Path) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let walk = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .build();
    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    for entry in walk.flatten() {
        let path = entry.path();
        if !entry.file_type().is_some_and(|t| t.is_file()) || !wanted(path) {
            continue;
        }
        if entry
            .metadata()
            .map(|m| m.len() > MAX_FILE || m.len() == 0)
            .unwrap_or(true)
        {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(rel, path.to_path_buf());
    }
    for (rel, path) in files {
        // Not UTF-8, or binary: not text to search.
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if text.contains('\0') {
            continue;
        }
        chunks.extend(chunk_file(&rel, &text));
    }
    Ok(chunks)
}

/// A file cut into chunks of whole lines, each at most `CHUNK` characters;
/// a single longer line is split on character boundaries.
pub fn chunk_file(rel: &str, text: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut body = String::new();
    let mut start = 1;
    let push = |body: &mut String, start: usize, end: usize, chunks: &mut Vec<Chunk>| {
        if body.trim().is_empty() {
            body.clear();
            return;
        }
        let index = chunks.len();
        let hash = format!("{:016x}", stable_hash(body));
        chunks.push(Chunk {
            id: format!("{rel}#{index}"),
            file: rel.to_owned(),
            start,
            end,
            body: std::mem::take(body),
            hash,
        });
    };
    for (n, line) in text.lines().enumerate() {
        let line_no = n + 1;
        if !body.is_empty() && body.chars().count() + line.chars().count() + 1 > CHUNK {
            push(&mut body, start, line_no - 1, &mut chunks);
            start = line_no;
        }
        if line.chars().count() > CHUNK {
            // One line longer than a chunk: its own chunks, cut on chars.
            let chars: Vec<char> = line.chars().collect();
            for part in chars.chunks(CHUNK) {
                body.push_str(&part.iter().collect::<String>());
                push(&mut body, line_no, line_no, &mut chunks);
            }
            start = line_no + 1;
            continue;
        }
        if body.is_empty() {
            start = line_no;
        }
        body.push_str(line);
        body.push('\n');
    }
    let last = text.lines().count().max(start);
    push(&mut body, start, last, &mut chunks);
    chunks
}

#[async_trait::async_trait]
impl Server for Rag {
    fn tools(&self) -> Vec<ToolSpec> {
        let path = ("path", "The folder, absolute; the workspace when left out");
        vec![
            ToolSpec {
                name: "index",
                description: "Index the project for search: embed every source file, in chunks with their line ranges. Only new or changed chunks are embedded, and chunks of removed code are dropped, so running it again after edits is cheap. Run it once before the first search, and after large changes.",
                input_schema: schema(&[path], &[]),
            },
            ToolSpec {
                name: "search",
                description: "Find the code that answers a question, by meaning and by words: \"where are refunds issued\", \"how is the session token checked\". Returns the best chunks with their file and line range, best first. Use it before reading files one by one in a large project.",
                input_schema: schema(
                    &[
                        ("query", "What you are looking for, in plain words"),
                        ("limit", "How many chunks, default 8, at most 30"),
                        path,
                    ],
                    &["query"],
                ),
            },
            ToolSpec {
                name: "status",
                description: "Whether the project is indexed: chunks, files, when it was last indexed, and the model.",
                input_schema: schema(&[path], &[]),
            },
            ToolSpec {
                name: "forget",
                description: "Delete the project's index.",
                input_schema: schema(&[path], &[]),
            },
        ]
    }

    async fn call(&self, tool: &str, args: &Value) -> Result<String> {
        match tool {
            "index" => self.index(args).await,
            "search" => self.search(args).await,
            "status" => self.status(args).await,
            "forget" => self.forget(args).await,
            other => bail!("rag has no tool `{other}`"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_keep_whole_lines_and_their_range() {
        let text: String = (1..=400)
            .map(|n| format!("line number {n} of the file\n"))
            .collect();
        let chunks = chunk_file("src/a.rs", &text);
        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].start, 1);
        for pair in chunks.windows(2) {
            assert_eq!(pair[1].start, pair[0].end + 1, "no line lost or repeated");
        }
        assert_eq!(chunks.last().unwrap().end, 400);
        assert!(chunks.iter().all(|c| c.body.chars().count() <= CHUNK + 1));
        assert!(chunks[0].text().starts_with("File: src/a.rs (lines 1-"));
    }

    #[test]
    fn a_long_line_is_split_on_characters() {
        let line: String = "é".repeat(CHUNK * 2 + 10);
        let chunks = chunk_file("a.txt", &format!("{line}\n"));
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|c| c.start == 1 && c.end == 1));
    }

    #[test]
    fn the_same_content_hashes_the_same() {
        let a = chunk_file("a.rs", "fn main() {}\n");
        let b = chunk_file("a.rs", "fn main() {}\n");
        let c = chunk_file("a.rs", "fn main() { todo!() }\n");
        assert_eq!(a[0].hash, b[0].hash);
        assert_ne!(a[0].hash, c[0].hash);
    }

    #[test]
    fn secrets_lockfiles_and_bundles_are_not_indexed() {
        for path in [
            ".env",
            ".env.local",
            "Cargo.lock",
            "package-lock.json",
            "app.min.js",
            "id.pem",
        ] {
            assert!(!wanted(Path::new(path)), "{path}");
        }
        for path in [
            "src/main.rs",
            "web/App.tsx",
            "README.md",
            "Dockerfile",
            "schema.sql",
        ] {
            assert!(wanted(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn the_walk_honours_gitignore() {
        let dir = std::env::temp_dir().join(format!("enx-rag-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("build")).unwrap();
        // `ignore` honours .gitignore inside a git repository.
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".gitignore"), "build/\n").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(dir.join("build/out.js"), "var a = 1;\n").unwrap();
        std::fs::write(dir.join(".env"), "SECRET=1\n").unwrap();
        let chunks = collect(&dir).unwrap();
        let files: Vec<&str> = chunks.iter().map(|c| c.file.as_str()).collect();
        assert_eq!(files, vec!["src/lib.rs"], "{files:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_vector_keeps_its_precision() {
        let literal = vector_literal(&[0.1, -2.5e-7, 1.0]);
        assert_eq!(literal, "[0.1,-0.00000025,1]");
    }

    #[test]
    fn two_checkouts_with_one_name_are_two_projects() {
        assert_ne!(
            project_id(Path::new("/a/shop")),
            project_id(Path::new("/b/shop"))
        );
        assert!(project_id(Path::new("/a/shop")).starts_with("shop-"));
    }
}
