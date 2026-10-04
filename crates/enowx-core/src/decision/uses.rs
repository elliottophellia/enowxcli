//! The judgements the harness hands to the decision model, each with its
//! questions, its threshold, and what it does when the model is unsure.
//!
//! Every use returns a [`Ruling`] whose verdict is safe to act on as it
//! stands: in shadow mode, below the threshold, late or failed, it is what
//! runs without a decision model, except the shell gate, where an answer
//! that is not sure asks the user.

use serde_json::{json, Value};

use super::{Decider, Question, Ruling, Use};

/// Text longer than this is cut before it is sent: the state needs the
/// gist, not every line, and less of the user's data leaves the machine.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head} …")
}

// ---------------------------------------------------------------- intent

/// What a request asks of the lead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// The user asked to think it through first.
    Brainstorm,
    /// The user asked for the thing: go to work.
    Build,
}

/// Brainstorm or build, for a new message to the lead. `None` leaves it to
/// the lead's own reading.
pub async fn intent(decider: &Decider, message: &str) -> Ruling<Option<Intent>> {
    let u = Use::Intent;
    let questions = [
        Question::boolean(
            "wants_brainstorm",
            "The user explicitly asks to brainstorm, explore options or ideas, or plan before \
             anything is built, rather than asking for the thing itself.",
        ),
        Question::choice(
            "scope",
            "How big is the work asked for?",
            &[
                (
                    "tiny",
                    "A one-line change, a question, or a single small file.",
                ),
                ("small", "A small page, script or fix touching a few files."),
                ("feature", "A feature inside an existing project."),
                ("product", "A whole new product or app with several parts."),
            ],
        ),
        Question::score(
            "ambiguity",
            "How unclear is what the user wants built?",
            &[
                "Fully clear",
                "Small details open, settled by convention",
                "Some choices open",
                "Purpose unclear",
                "Cannot tell what is wanted",
            ],
        ),
    ];
    let state = json!({ "message": clip(message, 4000) });
    let decision = match decider.consult(u, state, &questions).await {
        None => return Ruling::plain(None),
        Some(Err(record)) => {
            return Ruling {
                verdict: None,
                record: Some(*record),
            }
        }
        Some(Ok(decision)) => decision,
    };
    let threshold = decider.threshold(u);
    let wants = decision.yes("wants_brainstorm");
    // A large product whose purpose cannot be read is the one case the lead
    // may still brainstorm unasked: leave that to it.
    let unreadable_product = decision
        .choice("scope")
        .is_some_and(|(scope, p)| scope == "product" && p >= threshold)
        && decision.score("ambiguity").is_some_and(|a| a >= 3.0);
    let found = match wants {
        Some(p) if p >= threshold => Some((Intent::Brainstorm, p)),
        Some(p) if p <= 1.0 - threshold && !unreadable_product => Some((Intent::Build, 1.0 - p)),
        _ => None,
    };
    let action = match found {
        Some((Intent::Brainstorm, _)) => "brainstorm",
        Some((Intent::Build, _)) => "build directly",
        None => "lead decides",
    };
    let record = decider.conclude(u, &decision, found.map(|f| f.1), found.is_some(), action);
    Ruling {
        verdict: found.filter(|_| !decider.shadow()).map(|f| f.0),
        record: Some(record),
    }
}

/// What the lead is told about the request's intent.
pub fn intent_note(intent: Intent) -> &'static str {
    match intent {
        Intent::Brainstorm => {
            "\n\n## Decision for this message\nThe user asked to think it through first: read \
             the brainstorm skill and run it before building."
        }
        Intent::Build => {
            "\n\n## Decision for this message\nThis is a request to build, not to brainstorm. \
             Do not run a brainstorm: go to work, filling open details from the request, the \
             code and convention, and state your assumptions in one line."
        }
    }
}

// ---------------------------------------------------------------- routing

/// Who a request is for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Route {
    /// The one specialist the work is for.
    pub agent: Option<String>,
    /// Whether it needs a security review.
    pub security: Option<bool>,
}

/// Which specialist a new message to the lead is for, among `agents`
/// (name and what it does). `None` leaves the choice to the lead.
pub async fn routing(
    decider: &Decider,
    message: &str,
    agents: &[(String, String)],
) -> Ruling<Option<Route>> {
    let u = Use::Routing;
    if agents.is_empty() {
        return Ruling::plain(None);
    }
    let mut options: Vec<(String, String)> = agents
        .iter()
        .take(250)
        .map(|(name, what)| (name.clone(), clip(what, 160)))
        .collect();
    options.push((
        "none".into(),
        "No single specialist: several kinds of work, or only a question to answer.".into(),
    ));
    let questions = [
        Question::choice_owned(
            "specialist",
            "Which one specialist should do the work this message asks for?",
            options,
        ),
        Question::boolean(
            "needs_security_review",
            "The work touches sign-in, payments, secrets, user data or anything reachable from \
             the internet, so it needs a security review.",
        ),
    ];
    let state = json!({ "message": clip(message, 4000) });
    let decision = match decider.consult(u, state, &questions).await {
        None => return Ruling::plain(None),
        Some(Err(record)) => {
            return Ruling {
                verdict: None,
                record: Some(*record),
            }
        }
        Some(Ok(decision)) => decision,
    };
    let threshold = decider.threshold(u);
    let chosen = decision
        .choice("specialist")
        .filter(|(key, p)| *key != "none" && *p >= threshold);
    let security = decision.yes("needs_security_review").and_then(|p| {
        if p >= threshold {
            Some(true)
        } else if p <= 1.0 - threshold {
            Some(false)
        } else {
            None
        }
    });
    let route = Route {
        agent: chosen.map(|(key, _)| key.to_owned()),
        security,
    };
    let found = (route.agent.is_some() || route.security.is_some()).then_some(route);
    let action = match &found {
        Some(Route {
            agent: Some(agent), ..
        }) => format!("hint {agent}"),
        Some(_) => "hint security only".into(),
        None => "lead decides".into(),
    };
    let record = decider.conclude(
        u,
        &decision,
        chosen.map(|(_, p)| p),
        found.is_some(),
        action,
    );
    Ruling {
        verdict: found.filter(|_| !decider.shadow()),
        record: Some(record),
    }
}

/// What the lead is told about who the work is for.
pub fn route_note(route: &Route) -> String {
    let mut note = String::from("\n\n## Routing for this message\n");
    if let Some(agent) = &route.agent {
        note.push_str(&format!(
            "This reads as `{agent}` work: delegate it to `{agent}` directly, without \
             deliberating over who, unless something in the message says otherwise. "
        ));
    }
    match route.security {
        Some(true) => note.push_str("It needs a `security` review in the check wave."),
        Some(false) => note.push_str("It needs no security review."),
        None => {}
    }
    note
}

// ---------------------------------------------------------------- ask

/// Whether a question an agent wants to put to the user should be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskVerdict {
    /// Ask it.
    Allow,
    /// Not the user's to answer: the agent decides.
    Refuse,
    /// The model could not tell. The caller asks once per turn, then
    /// refuses.
    Unsure,
}

/// What an agent is told when its question is not the user's.
pub const ASK_REFUSED: &str = "Not asked: this can be settled from the request, the code and \
convention, so it is yours to decide. Decide, write the assumption in one line, and carry on.";

/// Judge a call to `ask`. `request` is what the user asked for; `questions`
/// the questions the agent wants to put.
pub async fn ask(decider: &Decider, request: &str, questions: &Value) -> Ruling<AskVerdict> {
    let u = Use::Ask;
    let asked = [
        Question::boolean(
            "belongs_to_user",
            "The agent's questions are the user's to answer: taste nothing in the request \
             hints at, money, something that cannot be undone, or credentials. Answer no when \
             they can be settled from the request, the project's code or common convention.",
        ),
        Question::choice(
            "category",
            "What kind of decision are the questions about?",
            &[
                ("taste", "Look, tone or style with no hint in the request."),
                ("cost", "Spending money or paid services."),
                (
                    "destructive",
                    "Deleting, overwriting or anything that cannot be undone.",
                ),
                ("credential", "Keys, passwords, accounts."),
                (
                    "derivable",
                    "Settled by the request, the code or convention.",
                ),
            ],
        ),
    ];
    let state = json!({
        "user_request": clip(request, 3000),
        "agent_questions": clip(&questions.to_string(), 3000),
    });
    let decision = match decider.consult(u, state, &asked).await {
        None => return Ruling::plain(AskVerdict::Allow),
        Some(Err(record)) => {
            return Ruling {
                verdict: AskVerdict::Allow,
                record: Some(*record),
            }
        }
        Some(Ok(decision)) => decision,
    };
    let threshold = decider.threshold(u);
    let (verdict, p) = match decision.yes("belongs_to_user") {
        Some(p) if p >= threshold => (AskVerdict::Allow, Some(p)),
        Some(p) if p <= 1.0 - threshold => (AskVerdict::Refuse, Some(1.0 - p)),
        _ => (AskVerdict::Unsure, None),
    };
    let action = match verdict {
        AskVerdict::Allow => "ask the user",
        AskVerdict::Refuse => "agent decides",
        AskVerdict::Unsure => "ask once",
    };
    let record = decider.conclude(u, &decision, p, verdict != AskVerdict::Unsure, action);
    Ruling {
        verdict: if decider.shadow() {
            AskVerdict::Allow
        } else {
            verdict
        },
        record: Some(record),
    }
}

// ---------------------------------------------------------------- tool results

/// Results this short are carried whole without asking: the judgement would
/// cost more than the text.
pub const ALWAYS_KEEP_UNDER: usize = 400;

/// How a tool result is carried into later turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Verbatim.
    Full,
    /// Its opening lines and how much was left out.
    Summary,
    /// Its two ends, where commands put what matters.
    Excerpt,
}

/// Judge a tool result. Errors and short results are always kept whole.
pub async fn tool_result(
    decider: &Decider,
    task: &str,
    tool: &str,
    arguments: &str,
    content: &str,
    is_error: bool,
) -> Ruling<Keep> {
    let u = Use::ToolResults;
    if is_error || content.len() <= ALWAYS_KEEP_UNDER {
        return Ruling::plain(Keep::Full);
    }
    let questions = [
        Question::choice(
            "relevance",
            "How much does the agent need this result to finish the task?",
            &[
                ("essential", "It must refer back to the details: code to edit, an error to fix, values to use."),
                ("useful", "The outcome matters, the detail mostly does not."),
                ("noise", "It only confirms the tool ran, or repeats the arguments."),
            ],
        ),
        Question::choice(
            "keep",
            "How should the result be carried into the agent's later turns?",
            &[
                ("full", "Whole: the agent will need its details."),
                ("summary", "Its first lines are enough."),
                ("excerpt", "Its beginning and end are enough, as with a command's output."),
            ],
        ),
    ];
    let state = json!({
        "task": clip(task, 1000),
        "tool": tool,
        "arguments": clip(arguments, 400),
        "result": clip(content, 2000),
        "result_length": content.len(),
    });
    let decision = match decider.consult(u, state, &questions).await {
        None => return Ruling::plain(Keep::Full),
        Some(Err(record)) => {
            return Ruling {
                verdict: Keep::Full,
                record: Some(*record),
            }
        }
        Some(Ok(decision)) => decision,
    };
    let threshold = decider.threshold(u);
    let cut = decision
        .choice("keep")
        .filter(|(key, p)| *key != "full" && *p >= threshold)
        // Cutting something the model itself called essential is the one
        // mistake that loses information; never do it.
        .filter(|_| decision.choice("relevance").map(|(r, _)| r) != Some("essential"));
    let keep = match cut.map(|(key, _)| key) {
        Some("summary") => Keep::Summary,
        Some("excerpt") => Keep::Excerpt,
        _ => Keep::Full,
    };
    let action = match keep {
        Keep::Full => format!("keep {tool} whole"),
        Keep::Summary => format!("summarise {tool}"),
        Keep::Excerpt => format!("excerpt {tool}"),
    };
    let record = decider.conclude(u, &decision, cut.map(|(_, p)| p), cut.is_some(), action);
    Ruling {
        verdict: if decider.shadow() { Keep::Full } else { keep },
        record: Some(record),
    }
}

/// The ends kept by an excerpt, together.
const EXCERPT_TO: usize = 600;

/// Lines kept by a summary.
const SUMMARY_LINES: usize = 12;

/// `content` as `keep` carries it. What was left out is said plainly, so
/// the model does not read the cut as the real output.
pub fn apply_keep(keep: Keep, content: &str) -> String {
    match keep {
        Keep::Full => content.to_owned(),
        Keep::Excerpt => {
            if content.len() <= EXCERPT_TO {
                return content.to_owned();
            }
            let half = EXCERPT_TO / 2;
            let head = floor(content, half);
            let tail = ceil(content, content.len() - half);
            format!(
                "{}\n… [{} characters left out of context; run the tool again to see them] …\n{}",
                &content[..head],
                tail - head,
                &content[tail..]
            )
        }
        Keep::Summary => {
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() <= SUMMARY_LINES {
                return apply_keep(Keep::Excerpt, content);
            }
            let kept = lines[..SUMMARY_LINES].join("\n");
            let kept = if kept.len() > EXCERPT_TO * 2 {
                kept[..floor(&kept, EXCERPT_TO * 2)].to_owned()
            } else {
                kept
            };
            format!(
                "{kept}\n… [{} more lines, {} characters in all, left out of context; run the \
                 tool again to see them]",
                lines.len() - SUMMARY_LINES,
                content.len()
            )
        }
    }
}

fn floor(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

// ---------------------------------------------------------------- shell

/// Whether a shell command may run as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellVerdict {
    Run,
    /// Ask the user first, saying why.
    Confirm(String),
}

/// Judge a shell command before it runs. Only a confident "it only reads"
/// or "it only writes inside the project" runs without the user's word; a
/// risky answer, or one the model is not sure of, asks. A late or failed
/// judgement runs the command as before.
pub async fn shell(decider: &Decider, command: &str) -> Ruling<ShellVerdict> {
    let u = Use::Shell;
    let questions = [Question::choice(
        "risk",
        "What is the worst this shell command does?",
        &[
            ("read_only", "Only reads, lists, builds or runs tests."),
            (
                "local_write",
                "Changes files inside the project, and nothing that cannot be redone.",
            ),
            (
                "destructive",
                "Deletes or overwrites data, rewrites git history, or cannot be undone.",
            ),
            (
                "network",
                "Sends data out, publishes, pushes, or installs from the network.",
            ),
            (
                "production",
                "Touches a live server, a production database or deployment.",
            ),
        ],
    )];
    let state = json!({ "command": clip(command, 2000) });
    let decision = match decider.consult(u, state, &questions).await {
        None => return Ruling::plain(ShellVerdict::Run),
        Some(Err(record)) => {
            return Ruling {
                verdict: ShellVerdict::Run,
                record: Some(*record),
            }
        }
        Some(Ok(decision)) => decision,
    };
    let threshold = decider.threshold(u);
    let risk = decision.choice("risk");
    let (verdict, p) = match risk {
        Some((key, p)) if p >= threshold && matches!(key, "read_only" | "local_write") => {
            (ShellVerdict::Run, Some(p))
        }
        Some((key, p)) if p >= threshold => (
            ShellVerdict::Confirm(format!("it looks {} ({p:.2})", key.replace('_', " "))),
            Some(p),
        ),
        Some((key, p)) => (
            ShellVerdict::Confirm(format!(
                "the decision model is not sure what it does (maybe {}, {p:.2})",
                key.replace('_', " ")
            )),
            None,
        ),
        None => (
            ShellVerdict::Confirm("the decision model could not judge it".into()),
            None,
        ),
    };
    let action = match &verdict {
        ShellVerdict::Run => "run".to_owned(),
        ShellVerdict::Confirm(_) => "confirm with the user".to_owned(),
    };
    // Asking because the model was unsure is still acting on the decision:
    // that is the rule for this gate.
    let record = decider.conclude(u, &decision, p, true, action);
    Ruling {
        verdict: if decider.shadow() {
            ShellVerdict::Run
        } else {
            verdict
        },
        record: Some(record),
    }
}

/// The question put to the user before a risky command, in `ask`'s form.
pub fn shell_question(command: &str, reason: &str) -> Value {
    json!({
        "questions": [{
            "question": format!(
                "Run this command? The decision model stopped it because {reason}.\n\n{}",
                clip(command, 600)
            ),
            "header": "Run command",
            "options": [
                {"label": "Run it", "description": "Run the command as it is."},
                {"label": "Don't run it", "description": "The agent is told it was not run."}
            ]
        }]
    })
}

/// What an agent is told when the user declined a command, or no one could
/// be asked.
pub const SHELL_DECLINED: &str = "Not run: the user did not approve this command. Do not run it \
another way; carry on without it, or say what you needed it for.";
pub const SHELL_NO_ONE: &str = "Not run: this command needs the user's approval and there is no \
user to ask here. Put it in your report, with why it is needed, for the agent that called you.";
