//! Rules checked on the code an agent writes, before the file changes.
//!
//! A rule is a Markdown file: a frontmatter with its name, what it catches,
//! `match:` lines (regular expressions, one per line), `files:` and
//! `exclude:` globs, and a `severity`; the body says what to do instead.
//! `block` refuses the write and returns the rule; `remind` lets it through
//! and adds the rule to the result. Only the text the call adds is checked,
//! so code already in a file does not trip a rule on every edit near it.
//!
//! Rules ship with enx (`rules/`), and are read from `~/.enx/rules/` and the
//! project's `.enx/rules/`; a rule of the same name replaces the one before
//! it, and `severity: off` turns one off. A line with `enx-allow: <name>`
//! lets that line through a blocking rule, for the case the rule allows.

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Block,
    Remind,
    Off,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub description: String,
    pub severity: Severity,
    patterns: Vec<Regex>,
    files: Option<GlobSet>,
    exclude: Option<GlobSet>,
    pub body: String,
}

const BUILTIN: &[&str] = &[
    include_str!("../rules/hardcoded-secret.md"),
    include_str!("../rules/ts-no-any.md"),
    include_str!("../rules/ts-ignore.md"),
    include_str!("../rules/js-empty-catch.md"),
    include_str!("../rules/js-console-log.md"),
    include_str!("../rules/react-index-key.md"),
    include_str!("../rules/react-dangerous-html.md"),
    include_str!("../rules/js-eval.md"),
    include_str!("../rules/js-debugger.md"),
    include_str!("../rules/rs-box-leak.md"),
    include_str!("../rules/rs-lazy-static.md"),
    include_str!("../rules/rs-transmute.md"),
    include_str!("../rules/rs-dbg.md"),
    include_str!("../rules/go-ioutil.md"),
    include_str!("../rules/go-exp-slices.md"),
    include_str!("../rules/go-rand-seed.md"),
    include_str!("../rules/py-bare-except.md"),
    include_str!("../rules/py-mutable-default.md"),
    include_str!("../rules/py-utcnow.md"),
    include_str!("../rules/css-transition-all.md"),
    include_str!("../rules/css-outline-none.md"),
];

fn globs(list: &str) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let mut any = false;
    for pattern in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        // A bare `*.ts` matches in any folder.
        let pattern = if pattern.contains('/') {
            pattern.to_owned()
        } else {
            format!("**/{pattern}")
        };
        if let Ok(glob) = Glob::new(&pattern) {
            builder.add(glob);
            any = true;
        }
    }
    any.then(|| builder.build().ok()).flatten()
}

/// Read one rule file. None when it names nothing to match.
pub fn parse(source: &str) -> Option<Rule> {
    let rest = source.trim_start_matches('\u{feff}').strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let (head, body) = (&rest[..end], &rest[end + 4..]);
    let mut rule = Rule {
        name: String::new(),
        description: String::new(),
        severity: Severity::Remind,
        patterns: Vec::new(),
        files: None,
        exclude: None,
        body: body.trim().to_owned(),
    };
    for line in head.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => rule.name = value.to_owned(),
            "description" => rule.description = value.to_owned(),
            "severity" => {
                rule.severity = match value {
                    "block" => Severity::Block,
                    "off" => Severity::Off,
                    _ => Severity::Remind,
                }
            }
            "match" => {
                if let Ok(regex) = Regex::new(&format!("(?m){value}")) {
                    rule.patterns.push(regex);
                }
            }
            "files" => rule.files = globs(value),
            "exclude" => rule.exclude = globs(value),
            _ => {}
        }
    }
    (!rule.name.is_empty() && (!rule.patterns.is_empty() || rule.severity == Severity::Off))
        .then_some(rule)
}

fn read_dir(dir: &Path, into: &mut Vec<Rule>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    paths.sort();
    for path in paths {
        if let Some(rule) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| parse(&text))
        {
            into.retain(|r| r.name != rule.name);
            into.push(rule);
        }
    }
}

/// Every rule in force for `workspace`: the shipped ones, then the user's,
/// then the project's, a later one replacing an earlier one of its name.
pub fn load(workspace: &Path) -> Vec<Rule> {
    let mut rules: Vec<Rule> = BUILTIN.iter().filter_map(|s| parse(s)).collect();
    read_dir(&crate::config::home_dir().join("rules"), &mut rules);
    read_dir(&workspace.join(".enx/rules"), &mut rules);
    rules.retain(|r| r.severity != Severity::Off);
    rules
}

/// What the rules say about `added`, the text a call puts into `path`
/// (relative to the workspace).
#[derive(Debug, Default)]
pub struct Verdict {
    pub blocked: Vec<Rule>,
    pub reminders: Vec<Rule>,
}

impl Verdict {
    pub fn is_clean(&self) -> bool {
        self.blocked.is_empty() && self.reminders.is_empty()
    }
}

pub fn check(rules: &[Rule], path: &Path, added: &str) -> Verdict {
    let mut verdict = Verdict::default();
    if added.trim().is_empty() {
        return verdict;
    }
    for rule in rules {
        if rule.files.as_ref().is_some_and(|set| !set.is_match(path))
            || rule.exclude.as_ref().is_some_and(|set| set.is_match(path))
        {
            continue;
        }
        let allow = format!("enx-allow: {}", rule.name);
        let hit = rule.patterns.iter().any(|regex| {
            regex.find_iter(added).any(|found| {
                // The line it is on may carry the allowance.
                let start = added[..found.start()].rfind('\n').map_or(0, |i| i + 1);
                let end = added[found.end()..]
                    .find('\n')
                    .map_or(added.len(), |i| found.end() + i);
                !added[start..end].contains(&allow)
            })
        });
        if hit {
            match rule.severity {
                Severity::Block => verdict.blocked.push(rule.clone()),
                Severity::Remind => verdict.reminders.push(rule.clone()),
                Severity::Off => {}
            }
        }
    }
    verdict
}

/// The lines of `new` that `old` did not have, as a multiset: what a call
/// adds to a file.
pub fn added_lines(old: &str, new: &str) -> String {
    let mut have: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for line in old.lines() {
        *have.entry(line).or_default() += 1;
    }
    let mut out = String::new();
    for line in new.lines() {
        match have.get_mut(line) {
            Some(n) if *n > 0 => *n -= 1,
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// How a refusal reads to the model.
pub fn refusal(rules: &[Rule]) -> String {
    let mut out = String::from("Not changed: the new code breaks a rule.\n");
    for rule in rules {
        out.push_str(&format!(
            "\nRule `{}`: {}\n{}\n",
            rule.name,
            rule.description,
            short(&rule.body)
        ));
    }
    out.push_str(
        "\nChange the code. In the rare case the rule allows, put `enx-allow: <rule name>` \
         in a comment on that line.",
    );
    out
}

/// How reminders read after a change that went through.
pub fn reminder(rules: &[Rule]) -> String {
    let mut out = String::from("\n\nRules\n");
    for rule in rules {
        out.push_str(&format!(
            "`{}`: {}\n{}\n",
            rule.name,
            rule.description,
            short(&rule.body)
        ));
    }
    out.push_str("Fix it now unless you have a reason the rule does not apply here.");
    out
}

fn short(body: &str) -> String {
    const MAX: usize = 900;
    if body.len() <= MAX {
        return body.to_owned();
    }
    let cut = body[..MAX].rfind('\n').unwrap_or(MAX);
    format!("{}\n…", &body[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin() -> Vec<Rule> {
        BUILTIN.iter().filter_map(|s| parse(s)).collect()
    }

    #[test]
    fn every_shipped_rule_parses() {
        let rules = builtin();
        assert_eq!(rules.len(), BUILTIN.len());
        for rule in &rules {
            assert!(
                !rule.description.is_empty() && !rule.body.is_empty(),
                "{}",
                rule.name
            );
        }
    }

    #[test]
    fn a_rule_applies_to_its_files_and_only_to_added_text() {
        let rules = builtin();
        let names = |v: &Verdict| -> Vec<String> {
            v.blocked
                .iter()
                .chain(&v.reminders)
                .map(|r| r.name.clone())
                .collect()
        };
        let ts = check(
            &rules,
            Path::new("src/api.ts"),
            "const data: any = await res.json();\n",
        );
        assert_eq!(names(&ts), ["ts-no-any"]);
        let dts = check(&rules, Path::new("src/env.d.ts"), "declare const x: any;\n");
        assert!(dts.is_clean(), "declaration files are excluded");
        let rs = check(&rules, Path::new("src/lib.rs"), "let data: any = 1;\n");
        assert!(rs.is_clean(), "a TypeScript rule on a Rust file");
        let old = "const a: any = 1;\n";
        let new = "const a: any = 1;\nconst b = 2;\n";
        assert!(check(&rules, Path::new("a.ts"), &added_lines(old, new)).is_clean());
    }

    #[test]
    fn a_secret_is_blocked_unless_allowed() {
        let rules = builtin();
        let key = "const key = \"sk-live-abcdefghijklmnopqrstuvwxyz123456\";\n";
        let verdict = check(&rules, Path::new("src/pay.ts"), key);
        assert_eq!(verdict.blocked.len(), 1);
        assert!(refusal(&verdict.blocked).contains("hardcoded-secret"));
        let allowed = "const key = \"sk-live-abcdefghijklmnopqrstuvwxyz123456\"; // enx-allow: hardcoded-secret\n";
        assert!(check(&rules, Path::new("src/pay.ts"), allowed)
            .blocked
            .is_empty());
    }

    #[test]
    fn a_project_rule_replaces_or_turns_off_a_shipped_one() {
        let mut rules = builtin();
        let off = parse("---\nname: ts-no-any\nseverity: off\n---\n").unwrap();
        rules.retain(|r| r.name != off.name);
        rules.push(off);
        rules.retain(|r| r.severity != Severity::Off);
        assert!(check(&rules, Path::new("a.ts"), "let x: any;\n").is_clean());
        let custom = parse(
            "---\nname: no-moment\ndescription: moment is not used here\nseverity: block\nmatch: from ['\"]moment['\"]\nfiles: *.ts\n---\nUse date-fns.\n",
        )
        .unwrap();
        let verdict = check(
            &[custom],
            Path::new("src/a.ts"),
            "import m from 'moment';\n",
        );
        assert_eq!(verdict.blocked.len(), 1);
    }
}
