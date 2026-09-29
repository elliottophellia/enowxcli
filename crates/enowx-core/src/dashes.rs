//! Em dashes out of what agents write.
//!
//! Every prompt and skill says not to use them, and models use them anyway:
//! the em dash is one of the surest marks of generated text. So it is taken
//! out after the fact, from the agents' replies and from the text they write
//! into files, rather than left to instructions.

use std::sync::OnceLock;

use regex::Regex;

/// `text` with its em dashes replaced the way a writer would: a comma between
/// two parts of a sentence, nothing after other punctuation or at either end
/// of a line. A spaced en dash used as a dash goes the same way; an unspaced
/// one (a range, `10–20`) stays. Code in backticks is left as it is.
pub fn strip(text: &str) -> String {
    if !text.contains('—') && !text.contains(" – ") {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
            continue;
        }
        // Inline code stays: split on backticks, clean the parts outside.
        for (part, piece) in line.split('`').enumerate() {
            if part > 0 {
                out.push('`');
            }
            if part % 2 == 1 {
                out.push_str(piece);
            } else {
                out.push_str(&clean(piece));
            }
        }
    }
    out
}

fn clean(text: &str) -> String {
    static RULES: OnceLock<[(Regex, &'static str); 5]> = OnceLock::new();
    let rules = RULES.get_or_init(|| {
        let r = |p: &str| Regex::new(p).expect("a valid pattern");
        [
            // At the start or the end of the text: nothing.
            (r(r"^[ \t]*—[ \t]*"), ""),
            (r(r"[ \t]*—[ \t]*$"), ""),
            // After punctuation: a space.
            (r(r"([.!?:;,])[ \t]*—[ \t]*"), "$1 "),
            // Between two parts: a comma.
            (r(r"[ \t]*—[ \t]*"), ", "),
            (r(r"[ \t]+–[ \t]+"), ", "),
        ]
    });
    let mut text = text.to_owned();
    for (pattern, with) in rules {
        text = pattern.replace_all(&text, *with).into_owned();
    }
    text
}

/// A write tool's arguments with em dashes taken out of the text it writes:
/// `content`, `new_text`, and each of `edits[].new_text`. `old_text` stays,
/// since it has to match the file as it is.
pub fn strip_written(tool: &str, args: &mut serde_json::Value) {
    let fix = |value: &mut serde_json::Value| {
        if let Some(text) = value.as_str() {
            let cleaned = strip(text);
            if cleaned != text {
                *value = serde_json::Value::String(cleaned);
            }
        }
    };
    match tool {
        "write" => fix(&mut args["content"]),
        "edit" => fix(&mut args["new_text"]),
        "multi_edit" => {
            if let Some(edits) = args["edits"].as_array_mut() {
                for edit in edits {
                    fix(&mut edit["new_text"]);
                }
            }
        }
        "edit_lines" => {
            if let Some(edits) = args["edits"].as_array_mut() {
                for edit in edits {
                    fix(&mut edit["text"]);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::strip;

    #[test]
    fn a_dash_becomes_what_a_writer_would_use() {
        assert_eq!(strip("fast — and cheap"), "fast, and cheap");
        assert_eq!(strip("fast—and cheap"), "fast, and cheap");
        assert_eq!(strip("It works. — Mostly."), "It works. Mostly.");
        assert_eq!(strip("— a list item"), "a list item");
        assert_eq!(strip("a trailing dash —"), "a trailing dash");
        assert_eq!(strip("one – two"), "one, two");
        assert_eq!(strip("pages 10–20"), "pages 10–20", "a range stays");
        assert_eq!(strip("no dash here"), "no dash here");
    }

    #[test]
    fn code_is_left_as_it_is() {
        let text = "Use `a—b` here — then:\n```\nlet s = \"x — y\";\n```\ndone — ok";
        assert_eq!(
            strip(text),
            "Use `a—b` here, then:\n```\nlet s = \"x — y\";\n```\ndone, ok"
        );
    }

    #[test]
    fn a_write_loses_its_dashes_but_an_edit_still_matches() {
        let mut args =
            serde_json::json!({"path": "a.md", "old_text": "x — y", "new_text": "x — z"});
        super::strip_written("edit", &mut args);
        assert_eq!(args["old_text"], "x — y");
        assert_eq!(args["new_text"], "x, z");
        let mut args = serde_json::json!({"edits": [{"old_text": "a", "new_text": "b — c"}]});
        super::strip_written("multi_edit", &mut args);
        assert_eq!(args["edits"][0]["new_text"], "b, c");
    }
}
