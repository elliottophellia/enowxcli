//! The markdown renderer end to end: every construct a model's reply uses,
//! checked for what reaches the screen — rows, prefixes, spacing, widths.
//! Styling is only asserted where it carries meaning (a table header, code).

use enowx_tui::preview_markdown;
use unicode_width::UnicodeWidthStr;

/// Rows as plain text: SGR sequences stripped, trailing spaces trimmed.
fn rows(md: &str, width: usize) -> Vec<String> {
    preview_markdown(md, width)
        .iter()
        .map(|row| strip(row).trim_end().to_string())
        .collect()
}

fn strip(styled: &str) -> String {
    let mut out = String::new();
    let mut chars = styled.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    out
}

fn find(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} not found in {rows:#?}"))
}

// -- Paragraphs and spacing -------------------------------------------------

/// A report written one field per line has to keep its lines; joined into a
/// paragraph it is unreadable.
#[test]
fn newlines_inside_a_paragraph_are_kept() {
    let got = rows("DONE: built it\nCHANGED: a.rs, b.rs\nNEXT: nothing", 80);
    assert_eq!(
        got,
        vec!["DONE: built it", "CHANGED: a.rs, b.rs", "NEXT: nothing"]
    );
}

/// Prose hard-wrapped in the source reflows instead of leaving a one-word
/// row at each source break when the panel is narrower than the source.
#[test]
fn hard_wrapped_prose_is_joined() {
    let md = "This is a sentence the model wrapped at a fixed column in its\n\
              reply, and it carries on here in lower case.";
    let got = rows(md, 50);
    // Joined, the break falls where the panel needs it: "its reply," sits
    // together rather than "its" ending a row that the source ended.
    let row = find(&got, "reply,");
    assert!(got[row].contains("its reply,"), "{got:#?}");
    assert_eq!(got.len(), 3, "{got:#?}");
}

#[test]
fn blank_lines_collapse_to_one_and_never_lead_or_trail() {
    let got = rows("\n\n\nFirst.\n\n\n\n\nSecond.\n\n\n", 40);
    assert_eq!(got, vec!["First.", "", "Second."]);
}

#[test]
fn an_empty_document_renders_nothing() {
    assert!(rows("", 40).is_empty());
    assert!(rows("\n \n\t\n", 40).is_empty());
}

#[test]
fn a_paragraph_wraps_at_spaces_and_fills_the_width() {
    let md = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
    let got = rows(md, 20);
    for row in &got {
        assert!(row.width() <= 20, "{row:?} is wider than 20");
    }
    // Every word intact, in order.
    assert_eq!(got.join(" "), md);
}

/// A token longer than the row breaks after a `/` rather than mid-name
/// where it can.
#[test]
fn a_long_path_breaks_after_a_separator() {
    let got = rows("crates/enowx-tui/src/ui/markdown/renderer.rs", 20);
    assert!(got.len() > 1, "{got:#?}");
    for row in &got[..got.len() - 1] {
        assert!(
            row.ends_with('/') || row.ends_with('-'),
            "{row:?} should end on a separator"
        );
    }
    assert_eq!(got.concat(), "crates/enowx-tui/src/ui/markdown/renderer.rs");
}

#[test]
fn mixed_wide_and_narrow_text_wraps_inside_the_width() {
    let md = "Mixed 中文字符和English words 日本語のテキスト wrap between wide characters.";
    for width in [12, 17, 25, 40] {
        for row in rows(md, width) {
            assert!(row.width() <= width, "{row:?} wider than {width}");
        }
    }
}

// -- Headings ---------------------------------------------------------------

#[test]
fn headings_get_one_blank_row_each_side() {
    let got = rows("Intro.\n# Title\nBody.\n## Section\n\n\nMore.", 40);
    assert_eq!(
        got,
        vec!["Intro.", "", "Title", "", "Body.", "", "Section", "", "More."]
    );
}

#[test]
fn setext_headings_are_headings() {
    let got = rows("Title\n=====\n\nSub\n---\nBody", 40);
    assert_eq!(got, vec!["Title", "", "Sub", "", "Body"]);
}

#[test]
fn a_heading_wraps_rather_than_overflowing() {
    let md = "## A heading that is far too long for a narrow panel to hold";
    for row in rows(md, 16) {
        assert!(row.width() <= 16, "{row:?}");
    }
}

// -- Lists ------------------------------------------------------------------

#[test]
fn nested_lists_indent_under_their_parent_text() {
    let got = rows("- one\n  - two\n    - three\n- four", 40);
    assert_eq!(got, vec!["• one", "  ◦ two", "    • three", "• four"]);
}

#[test]
fn a_wrapped_item_continues_under_its_text() {
    let got = rows("- a bullet with enough words to wrap", 16);
    assert_eq!(got[0], "• a bullet with");
    for row in &got[1..] {
        assert!(row.starts_with("  ") && !row.starts_with("   "), "{row:?}");
    }
}

#[test]
fn ordered_numbers_right_align_and_keep_their_start() {
    let got = rows("8. eight\n9. nine\n10. ten\n11. eleven", 40);
    assert_eq!(got, vec![" 8. eight", " 9. nine", "10. ten", "11. eleven"]);
}

#[test]
fn an_ordered_item_continuation_lines_up_with_its_text() {
    let got = rows("1. first line\n   second line\n2. next", 40);
    assert_eq!(got, vec!["1. first line", "   second line", "2. next"]);
}

#[test]
fn task_items_show_their_state() {
    let got = rows("- [x] done\n- [ ] open", 40);
    assert_eq!(got, vec!["☑ done", "☐ open"]);
}

#[test]
fn a_loose_list_keeps_its_blank_rows_and_a_tight_one_does_not() {
    assert_eq!(rows("- a\n- b", 40), vec!["• a", "• b"]);
    assert_eq!(rows("- a\n\n- b", 40), vec!["• a", "", "• b"]);
}

#[test]
fn code_inside_a_list_item_is_indented_with_it() {
    let got = rows("- run:\n  ```sh\n  cargo test\n  ```\n- done", 40);
    let code = find(&got, "cargo test");
    assert!(got[code].starts_with("  │ cargo test"), "{got:#?}");
    assert_eq!(got.last().map(String::as_str), Some("• done"));
}

// -- Quotes -----------------------------------------------------------------

#[test]
fn every_quoted_row_carries_the_bar() {
    let md = "> a quotation long enough to wrap onto more rows\n>\n> second paragraph";
    let got = rows(md, 20);
    assert!(got.len() >= 4, "{got:#?}");
    for row in &got {
        assert!(row.starts_with('│'), "{row:?} lost the bar");
    }
}

#[test]
fn a_nested_quote_has_two_bars() {
    let got = rows("> outer\n>\n> > inner", 40);
    assert_eq!(got[find(&got, "inner")], "│ │ inner");
}

#[test]
fn an_alert_names_itself() {
    let got = rows("> [!WARNING]\n> Mind the gap.", 40);
    assert_eq!(got, vec!["│ Warning", "│ Mind the gap."]);
}

// -- Code -------------------------------------------------------------------

#[test]
fn a_fenced_block_is_labelled_with_its_language() {
    let got = rows("```rust\nfn main() {}\n```", 40);
    assert_eq!(got, vec!["│ rust", "│ fn main() {}"]);
}

#[test]
fn tilde_fences_and_indented_blocks_are_code_too() {
    assert_eq!(rows("~~~\nplain\n~~~", 40), vec!["│ plain"]);
    assert_eq!(
        rows("Text.\n\n    indented", 40),
        vec!["Text.", "", "│ indented"]
    );
}

/// A fence's info string can carry more than the language.
#[test]
fn only_the_first_word_of_the_info_string_is_the_label() {
    let got = rows("```js title=app.js\nlet a = 1;\n```", 40);
    assert_eq!(got[0], "│ js");
}

#[test]
fn a_long_code_line_wraps_without_losing_characters() {
    let line = "let value = compute(alpha, beta, gamma, delta, epsilon);";
    let got = rows(&format!("```\n{line}\n```"), 24);
    let rebuilt: String = got
        .iter()
        .map(|row| row.trim_start_matches("│ ").trim_start_matches("│↳"))
        .collect();
    assert_eq!(rebuilt, line);
    for row in &got {
        assert!(row.width() <= 24, "{row:?}");
    }
}

#[test]
fn tabs_in_code_become_spaces() {
    let got = rows("```\n\tindented\n```", 40);
    assert_eq!(got, vec!["│     indented"]);
}

/// While a reply streams, the closing fence has not arrived yet.
#[test]
fn an_unterminated_fence_is_still_code() {
    let got = rows("Look:\n\n```python\nprint('hi')", 40);
    assert_eq!(got, vec!["Look:", "", "│ python", "│ print('hi')"]);
}

/// Blank lines inside a block belong to the code and are kept.
#[test]
fn blank_lines_inside_code_are_kept() {
    let got = rows("```\na\n\n\nb\n```", 40);
    assert_eq!(got, vec!["│ a", "│", "│", "│ b"]);
}

// -- Tables -----------------------------------------------------------------

const TABLE: &str = "| Name | Role | Cost |\n| :--- | :--: | ---: |\n| router | picks the agent | $0.01 |\n| fe | pages | $1.20 |";

#[test]
fn a_table_is_boxed_with_a_rule_under_its_header() {
    let got = rows(TABLE, 60);
    assert_eq!(
        got,
        vec![
            "╭────────┬─────────────────┬───────╮",
            "│ Name   │      Role       │  Cost │",
            "├────────┼─────────────────┼───────┤",
            "│ router │ picks the agent │ $0.01 │",
            "│ fe     │      pages      │ $1.20 │",
            "╰────────┴─────────────────┴───────╯",
        ]
    );
}

#[test]
fn a_table_header_is_bold() {
    let styled = preview_markdown(TABLE, 60).join("\n");
    let header = styled
        .lines()
        .find(|row| strip(row).contains("Name"))
        .expect("header row");
    assert!(
        header.contains("\u{1b}[1m") || header.contains(";1m") || header.contains("[1;"),
        "the header row should be bold: {header:?}"
    );
}

/// Cells wrap inside their column rather than being cut with `…`, and a
/// table with wrapped rows gets a rule between rows.
#[test]
fn cells_wrap_instead_of_truncating() {
    let md = "| Key | Meaning |\n| --- | --- |\n| a | the first entry in a list that has several |\n| b | short |";
    let got = rows(md, 30);
    let text = got.join("\n");
    assert!(!text.contains('…'), "{got:#?}");
    for word in ["first", "entry", "several", "short"] {
        assert!(text.contains(word), "{word} lost: {got:#?}");
    }
    for row in &got {
        assert_eq!(row.width(), got[0].width(), "ragged table row {row:?}");
        assert!(row.width() <= 30, "{row:?}");
    }
    assert!(
        got.iter().filter(|row| row.starts_with('├')).count() >= 2,
        "wrapped rows need a rule between them: {got:#?}"
    );
}

/// Too narrow to keep words whole in columns: rows become `header: value`.
#[test]
fn a_table_too_wide_for_the_panel_stacks() {
    let got = rows(TABLE, 22);
    assert!(got.iter().all(|row| row.width() <= 22), "{got:#?}");
    assert!(got.contains(&"Name: router".to_string()), "{got:#?}");
    assert!(got.contains(&"Cost: $1.20".to_string()), "{got:#?}");
    assert!(!got.iter().any(|row| row.contains('│')), "{got:#?}");
}

#[test]
fn inline_markdown_in_cells_is_rendered() {
    let got = rows("| a | b |\n| - | - |\n| **bold** | `code` |", 40);
    let text = got.join("\n");
    assert!(text.contains("bold") && text.contains("code"), "{got:#?}");
    assert!(!text.contains("**") && !text.contains('`'), "{got:#?}");
}

// -- Everything else --------------------------------------------------------

#[test]
fn a_rule_spans_the_width() {
    let got = rows("above\n\n---\n\nbelow", 30);
    assert_eq!(got[2], "─".repeat(30));
}

#[test]
fn images_show_their_alt_text() {
    assert_eq!(
        rows("See ![the diagram](d.png).", 40),
        vec!["See the diagram."]
    );
    assert_eq!(rows("See ![](d.png).", 40), vec!["See image."]);
}

#[test]
fn html_comments_are_hidden_and_formatting_tags_dropped() {
    let got = rows(
        "Press <kbd>Ctrl</kbd>+<kbd>P</kbd>.\n\n<!-- hidden -->\n\nAfter.",
        40,
    );
    assert_eq!(got, vec!["Press Ctrl+P.", "", "After."]);
}

#[test]
fn a_br_tag_breaks_the_line() {
    assert_eq!(rows("one<br>two", 40), vec!["one", "two"]);
}

/// Generics written without backticks parse as HTML; the text still shows.
#[test]
fn angle_brackets_in_prose_survive() {
    let got = rows("Returns Vec<String> or Option<u8>.", 40);
    assert_eq!(got, vec!["Returns Vec<String> or Option<u8>."]);
}

/// An escape byte reaching the terminal would be interpreted as a command.
#[test]
fn control_characters_never_reach_the_screen() {
    let styled = preview_markdown("red \u{1b}[31mtext\u{7} here", 40).join("");
    let visible = strip(&styled);
    assert!(!visible.contains('\u{7}'), "{visible:?}");
    // The only escapes left are the renderer's own colour sequences.
    assert_eq!(visible, "red [31mtext here");
}

/// Every row of every construct fits, at every width a panel can have.
#[test]
fn no_row_is_ever_wider_than_the_panel() {
    let docs = [
        TABLE,
        "- a list item\n  - nested with a long line of words to wrap\n    1. deeper\n       ```rust\n       let x = 1;\n       ```",
        "> quote\n> > nested quote with a longer line of text\n> > - and a list inside it",
        "# A heading\n\nText with `inline code that is quite long` and a [link label](u).",
        "| 名前 | 説明 |\n| --- | --- |\n| 日本語 | とても長い説明文です |",
        "```\n日本語のテキストはとても長いです😀😀😀\n```",
        "Emoji 🚀🌟✨ run 🚀🌟✨🚀🌟✨🚀🌟✨ on",
        "1. [ ] task in an ordered list with enough text to wrap around",
        "---\n\n***",
    ];
    for doc in docs {
        for width in 10..=90 {
            for row in rows(doc, width) {
                assert!(
                    row.width() <= width,
                    "width {width}: {row:?} ({} cols) from {doc:?}",
                    row.width()
                );
            }
        }
    }
}
