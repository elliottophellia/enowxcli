//! An agent asking the user questions, and the answers coming back.
//!
//! The agent holding the user's conversation may stop and ask: before
//! something that cannot be undone, or when a choice changes what gets built
//! and nothing in the request or the project settles it. One call carries one
//! question or a few, each with options; the interface always adds a way to
//! write an answer of one's own, and a note can go with any option chosen.
//! The turn waits for the answers, which come back as the tool's result.
//!
//! A delegated sub-agent is never offered the tool: the user cannot see it,
//! so its questions go in its report for the agent that called it to ask.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The tool's name.
pub const TOOL: &str = "ask";

/// The most questions one call may carry, and the most options one question
/// may offer. Past these it is a form to fill in, not a choice to make.
const MAX_QUESTIONS: usize = 8;
const MAX_OPTIONS: usize = 5;

/// One question, with the answers the agent offers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    // Models name the field a few ways; all mean the question.
    #[serde(
        alias = "prompt",
        alias = "text",
        alias = "q",
        alias = "title",
        alias = "query"
    )]
    pub question: String,
    /// A word or two naming the question, for moving between several.
    #[serde(default, alias = "label", alias = "name")]
    pub header: String,
    /// Offered answers. The user may always write their own instead.
    #[serde(default, alias = "choices", alias = "answers", alias = "options_list")]
    pub options: Vec<Choice>,
    /// Whether more than one option may be chosen.
    #[serde(
        default,
        alias = "multi",
        alias = "multiple_choice",
        alias = "multiselect",
        alias = "multiSelect",
        alias = "allow_multiple"
    )]
    pub multiple: bool,
}

/// An offered answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    #[serde(
        alias = "text",
        alias = "title",
        alias = "value",
        alias = "name",
        alias = "option"
    )]
    pub label: String,
    /// What choosing it means, when the label alone does not say.
    #[serde(
        default,
        alias = "detail",
        alias = "desc",
        alias = "explanation",
        alias = "hint"
    )]
    pub description: String,
}

/// The user's reply to one question.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    /// The labels of the options chosen, in the order offered.
    #[serde(default)]
    pub chosen: Vec<String>,
    /// An answer in the user's own words.
    #[serde(default)]
    pub other: String,
    /// Notes the user added, as (option label, note).
    #[serde(default)]
    pub notes: Vec<(String, String)>,
}

impl Reply {
    pub fn is_empty(&self) -> bool {
        self.chosen.is_empty() && self.other.trim().is_empty()
    }
}

/// The user's replies, one per question in the order asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub replies: Vec<Reply>,
}

/// The tool as the model sees it.
pub fn schema() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": TOOL,
            "description":
                "Ask the user and wait for the answers. Ask only what changes what \
                 you do next and cannot be settled from the request or the project, \
                 or before something that cannot be undone; otherwise choose and say \
                 what you chose. Put every question you have into one call, as a \
                 list: the user moves between them and sends them together, where \
                 separate calls make them answer and wait again each time. Give each two to five \
                 options with the one you recommend first, its label ending in \
                 \"(recommended)\". Do not add an \"Other\" option: the user can \
                 always write their own answer, and add a note to any option. Ask in \
                 the user's language.",
            "parameters": {
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_QUESTIONS,
                        "items": {
                            "type": "object",
                            "properties": {
                                "question": {
                                    "type": "string",
                                    "description": "The question, ending with a question mark"
                                },
                                "header": {
                                    "type": "string",
                                    "description": "A word or two naming it, such as \"Layout\""
                                },
                                "options": {
                                    "type": "array",
                                    "maxItems": MAX_OPTIONS,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "label": {"type": "string", "description": "A few words"},
                                            "description": {
                                                "type": "string",
                                                "description": "What choosing it means"
                                            }
                                        },
                                        "required": ["label"]
                                    }
                                },
                                "multiple": {
                                    "type": "boolean",
                                    "description": "Whether several options may be chosen"
                                }
                            },
                            "required": ["question"]
                        }
                    }
                },
                "required": ["questions"]
            }
        }
    })
}

/// The questions in a call's arguments, or what is wrong with them. A single
/// question given without the `questions` list is taken as one.
pub fn parse(args: &Value) -> Result<Vec<Question>, String> {
    // A plain string is the question itself; an object is a full question.
    // Models sometimes give a bare string where a question object is meant.
    let one = |value: &Value| -> Result<Question, String> {
        if let Some(text) = value.as_str() {
            return Ok(Question {
                question: text.to_owned(),
                ..Default::default()
            });
        }
        serde_json::from_value(value.clone()).map_err(|error| format!("{error}"))
    };
    let questions: Vec<Question> = match args.get("questions").or_else(|| args.get("question")) {
        // The list form, the normal one.
        Some(Value::Array(list)) => list
            .iter()
            .map(one)
            .collect::<Result<_, _>>()
            .map_err(|error| format!("not a list of questions: {error}"))?,
        // A single question handed under `questions`/`question`, not wrapped
        // in a list.
        Some(value) => vec![one(value).map_err(|error| format!("not a question: {error}"))?],
        // The whole argument object is the question.
        None => vec![serde_json::from_value(args.clone())
            .map_err(|error| format!("not a question: {error}"))?],
    };
    if questions.is_empty() {
        return Err("ask at least one question".into());
    }
    if questions.len() > MAX_QUESTIONS {
        return Err(format!("ask at most {MAX_QUESTIONS} questions at once"));
    }
    for question in &questions {
        if question.question.trim().is_empty() {
            return Err("a question is empty".into());
        }
        if question.options.len() > MAX_OPTIONS {
            return Err(format!("offer at most {MAX_OPTIONS} options"));
        }
        if question.options.iter().any(|o| o.label.trim().is_empty()) {
            return Err("every option needs a label".into());
        }
    }
    Ok(questions)
}

/// What the agent is told the user answered: each question, then its reply.
pub fn answer_message(questions: &[Question], answer: &Answer) -> String {
    let mut out = String::from("The user answered:");
    for (index, question) in questions.iter().enumerate() {
        let reply = answer.replies.get(index).cloned().unwrap_or_default();
        out.push_str(&format!(
            "\n{}. {}\n   ",
            index + 1,
            question.question.trim()
        ));
        if reply.is_empty() {
            out.push_str("No answer.");
            continue;
        }
        let mut parts = Vec::new();
        if !reply.chosen.is_empty() {
            parts.push(format!("Chose: {}.", reply.chosen.join(", ")));
        }
        let other = reply.other.trim();
        if !other.is_empty() {
            parts.push(format!("In their own words: {other}"));
        }
        for (option, note) in &reply.notes {
            let note = note.trim();
            if !note.is_empty() {
                parts.push(format!("Note on \"{option}\": {note}"));
            }
        }
        out.push_str(&parts.join(" "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Models label the fields a few ways and sometimes give a bare
    /// string; each still parses into a question.
    #[test]
    fn a_question_is_read_however_it_is_dressed() {
        // The `question` field under other names.
        for key in ["question", "prompt", "text", "q", "title"] {
            let parsed = parse(&json!({"questions": [{key: "Which mode?"}]})).unwrap();
            assert_eq!(parsed[0].question, "Which mode?", "key {key}");
        }
        // A bare string item.
        let parsed = parse(&json!({"questions": ["Which mode?"]})).unwrap();
        assert_eq!(parsed[0].question, "Which mode?");
        // Options and multi-select under other names.
        let parsed = parse(&json!({"questions": [{
            "prompt": "Pick pages",
            "multiSelect": true,
            "choices": [{"text": "Home", "detail": "the landing page"}]
        }]}))
        .unwrap();
        assert!(parsed[0].multiple);
        assert_eq!(parsed[0].options[0].label, "Home");
        assert_eq!(parsed[0].options[0].description, "the landing page");
        // A single question not wrapped in a list.
        let parsed = parse(&json!({"question": "Go ahead?"})).unwrap();
        assert_eq!(parsed[0].question, "Go ahead?");
        // The whole object is the question.
        let parsed = parse(&json!({"prompt": "Go ahead?"})).unwrap();
        assert_eq!(parsed[0].question, "Go ahead?");
    }

    #[test]
    fn questions_read_from_their_arguments() {
        let questions = parse(&json!({"questions": [
            {"question": "Which layout?", "header": "Layout",
             "options": [{"label": "Sidebar (recommended)"}, {"label": "Top bar", "description": "for few sections"}]},
            {"question": "Which pages?", "multiple": true,
             "options": [{"label": "Home"}, {"label": "Pricing"}]}
        ]}))
        .unwrap();
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].header, "Layout");
        assert_eq!(questions[0].options[1].description, "for few sections");
        assert!(questions[1].multiple);
    }

    #[test]
    fn a_single_question_is_accepted_on_its_own() {
        let questions = parse(&json!({"question": "Ready?"})).unwrap();
        assert_eq!(questions.len(), 1);
        assert!(questions[0].options.is_empty());
    }

    #[test]
    fn malformed_questions_say_why() {
        assert!(parse(&json!({"questions": []})).is_err());
        assert!(parse(&json!({"questions": [{"question": "  "}]})).is_err());
        assert!(
            parse(&json!({"questions": [{"question": "x?", "options": [{"label": ""}]}]})).is_err()
        );
        let many: Vec<Value> = (0..9)
            .map(|i| json!({"question": format!("{i}?")}))
            .collect();
        assert!(parse(&json!({"questions": many})).is_err());
    }

    #[test]
    fn the_answer_says_each_reply_with_its_notes() {
        let questions = parse(&json!({"questions": [
            {"question": "Which layout?"}, {"question": "Who for?"}, {"question": "Colours?"}
        ]}))
        .unwrap();
        let answer = Answer {
            replies: vec![
                Reply {
                    chosen: vec!["Sidebar".into()],
                    other: String::new(),
                    notes: vec![("Sidebar".into(), "keep it collapsible".into())],
                },
                Reply {
                    chosen: Vec::new(),
                    other: "all patients, in two languages".into(),
                    notes: Vec::new(),
                },
            ],
        };
        assert_eq!(
            answer_message(&questions, &answer),
            "The user answered:\n\
             1. Which layout?\n   Chose: Sidebar. Note on \"Sidebar\": keep it collapsible\n\
             2. Who for?\n   In their own words: all patients, in two languages\n\
             3. Colours?\n   No answer."
        );
    }
}
