//! The marks of generated work in interface code, found deterministically.
//!
//! The `ui-audit` skill lists what to search for; left to the model, the
//! searches were typed one grep at a time, or skipped, and a finished page
//! still carried five colours outside its tokens and an invented insurance
//! policy. These rules run the same searches in one pass, for the `ui_check`
//! tool, for the check the harness runs before an interface agent reports,
//! and for the evaluation that scores a run.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

/// How much a finding matters: dishonest or broken, then the generated look,
/// then consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "HIGH",
            Self::Medium => "MED",
            Self::Low => "LOW",
        }
    }
}

/// One mark, where it is, and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub rule: &'static str,
    pub path: String,
    pub line: usize,
    pub excerpt: String,
    pub why: &'static str,
    pub fix: &'static str,
}

/// Which files a rule reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// HTML and components: html, jsx, tsx, vue, svelte, astro, mdx.
    Markup,
    /// Stylesheets.
    Style,
    /// Markup and stylesheets.
    Both,
}

struct Rule {
    id: &'static str,
    severity: Severity,
    kind: Kind,
    pattern: &'static str,
    /// A match that contains this is not a finding (`<img` with an `alt=`).
    unless_match_has: Option<&'static str>,
    /// A file that contains this has none of this rule's findings (focus
    /// removed, but replaced with `focus-visible`).
    unless_file_has: Option<&'static str>,
    /// Reported only when there are at least this many matches across the
    /// files checked: one blur is a choice, blur everywhere is a look.
    at_least: usize,
    why: &'static str,
    fix: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        id: "invented-figure",
        severity: Severity::High,
        kind: Kind::Markup,
        pattern: r"(?i)(\b\d[\d,.]*\s*[km]?\+\s*(users|customers|clients|companies|teams|developers|downloads|reviews|happy|members|pengguna|pelanggan|klien|perusahaan|anggota)\b|\b99\.9+\s*%|\b\d+x\s+(faster|lebih cepat))",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a figure with no source reads as made up",
        fix: "remove it, or mark it [REAL FIGURE] until there is one",
    },
    Rule {
        id: "dead-link",
        severity: Severity::High,
        kind: Kind::Markup,
        pattern: r#"href=\{?["'](#|javascript:void\(0\);?)?["']"#,
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a link that goes nowhere",
        fix: "link a real page or section, or remove it",
    },
    Rule {
        id: "empty-handler",
        severity: Severity::High,
        kind: Kind::Markup,
        pattern: r"on[A-Z][A-Za-z]*=\{\s*\(\)\s*=>\s*(\{\s*\}|null|undefined)\s*\}",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a control that does nothing",
        fix: "give it real behaviour, or remove it",
    },
    Rule {
        id: "focus-removed",
        severity: Severity::High,
        kind: Kind::Both,
        pattern: r"outline:\s*(none|0)\b|\boutline-none\b",
        unless_match_has: None,
        unless_file_has: Some("focus-visible"),
        at_least: 1,
        why: "keyboard users cannot see where they are",
        fix: "keep the outline, or replace it with a visible :focus-visible style",
    },
    Rule {
        id: "image-without-alt",
        severity: Severity::High,
        kind: Kind::Markup,
        pattern: r"<(img|Image)\b[^>]*>",
        unless_match_has: Some("alt="),
        unless_file_has: None,
        at_least: 1,
        why: "an image with no text alternative",
        fix: "add alt text saying what it shows, or alt=\"\" when it is decoration",
    },
    Rule {
        id: "placeholder-content",
        severity: Severity::High,
        kind: Kind::Markup,
        pattern: r"(?i)lorem ipsum|\bjohn doe\b|\bjane doe\b|johndoe@|jane@example|\bacme (inc|corp)",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "filler that looks like real content",
        fix: "use the real content, or a placeholder that says it is one: [Name]",
    },
    Rule {
        id: "default-gradient",
        severity: Severity::Medium,
        kind: Kind::Both,
        pattern: r"(?i)(linear|radial)-gradient\([^)]*(#6366f1|#8b5cf6|#a855f7|#7c3aed|#9333ea|#ec4899|purple|violet|indigo|fuchsia)|\b(from|via|to)-(purple|violet|indigo|fuchsia)-\d{2,3}\b",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "the default blue-purple gradient, not the product's palette",
        fix: "use the product's colours from the tokens, or a solid colour",
    },
    Rule {
        id: "gradient-text",
        severity: Severity::Medium,
        kind: Kind::Both,
        pattern: r"\bbg-clip-text\b|background-clip:\s*text",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "gradient headline text, a generated default",
        fix: "a solid text colour from the tokens",
    },
    Rule {
        id: "glow",
        severity: Severity::Medium,
        kind: Kind::Both,
        pattern: r"box-shadow:\s*0\s+0\s+\d{2,}px|shadow-\[0_0_\d{2,}px|drop-shadow-\[0_0_",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a coloured glow, decoration without a purpose",
        fix: "remove it; mark focus or the one key element another way",
    },
    Rule {
        id: "glass",
        severity: Severity::Medium,
        kind: Kind::Both,
        pattern: r"backdrop-filter:\s*blur|\bbackdrop-blur(-\w+)?\b",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 3,
        why: "frosted glass on many surfaces flattens the hierarchy",
        fix: "keep it on one or two elements at most, solid surfaces elsewhere",
    },
    Rule {
        id: "buzzword",
        severity: Severity::Medium,
        kind: Kind::Markup,
        pattern: r"(?i)\b(unlock|elevate|empower|seamless(ly)?|revolutioni[sz]e|next-gen(eration)?|cutting-edge|supercharge|effortless(ly)?|game-?changing|world-class|harness the power)\b",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a word that promises and says nothing",
        fix: "say the concrete thing it stands for, or cut it",
    },
    Rule {
        id: "generic-action",
        severity: Severity::Medium,
        kind: Kind::Markup,
        pattern: r">\s*(Get [Ss]tarted|Learn [Mm]ore|Try [Nn]ow|Explore|Discover|Click [Hh]ere|Submit)\s*<",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "an action that does not say what happens",
        fix: "name the action: \"Book a session\", \"Create invoice\"",
    },
    Rule {
        id: "emoji",
        severity: Severity::Medium,
        kind: Kind::Markup,
        pattern: r"[\x{1F300}-\x{1FAFF}\x{2600}-\x{26FF}\x{2705}\x{2728}\x{274C}\x{2B50}]",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "emoji as decoration or as icons",
        fix: "remove it, or use an icon from the product's one icon set",
    },
    Rule {
        id: "em-dash",
        severity: Severity::Medium,
        kind: Kind::Markup,
        pattern: "\u{2014}",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "em dashes in interface copy read as generated",
        fix: "use a comma, a full stop, a colon or brackets",
    },
    Rule {
        id: "wide-caps-label",
        severity: Severity::Medium,
        kind: Kind::Markup,
        pattern: r#"uppercase[^"'\n]*tracking-(widest|\[0\.[2-9])|tracking-(widest|\[0\.[2-9])[^"'\n]*uppercase"#,
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a decorative uppercase label with wide letter-spacing",
        fix: "sentence case at normal spacing, or no label",
    },
    Rule {
        id: "full-screen-section",
        severity: Severity::Medium,
        kind: Kind::Both,
        pattern: r"height:\s*100vh|\bh-screen\b",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a section fixed to the screen height pushes the rest below the fold on a phone",
        fix: "let the height come from the content, or min-height with dvh",
    },
    Rule {
        id: "generic-feature-icon",
        severity: Severity::Medium,
        kind: Kind::Markup,
        pattern: r#"import\s*\{[^}]*\b(Sparkles|Rocket|Zap|Wand2?|Bot|Stars)\b[^}]*\}\s*from\s*["'](lucide-react|@heroicons|react-icons|@tabler|@phosphor-icons)"#,
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "the generic glyphs (sparkles, rocket, lightning, wand) standing in for features",
        fix: "an icon that means the feature, or none",
    },
    Rule {
        id: "colour-outside-tokens",
        severity: Severity::Low,
        kind: Kind::Style,
        pattern: r"#[0-9a-fA-F]{3,8}\b|\brgba?\(",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a colour written outside the tokens",
        fix: "define it once as a custom property and use var(--name)",
    },
    Rule {
        id: "colour-outside-tokens",
        severity: Severity::Low,
        kind: Kind::Markup,
        pattern: r"-\[#[0-9a-fA-F]{3,8}\]",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a colour written outside the theme",
        fix: "add it to the theme and use its name",
    },
    Rule {
        id: "default-font",
        severity: Severity::Low,
        kind: Kind::Both,
        pattern: r"(?i)font-family:[^;]*\b(Inter|Poppins|Space Grotesk|Geist)\b|family=(Inter|Poppins|Space\+Grotesk|Geist)\b",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 1,
        why: "a default font, which needs a reason to be the choice",
        fix: "keep it if it was chosen for the product, and say why; otherwise choose",
    },
    Rule {
        id: "pills-everywhere",
        severity: Severity::Low,
        kind: Kind::Both,
        pattern: r"\brounded-full\b|border-radius:\s*9999px",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 16,
        why: "everything pill-shaped erases the difference between parts",
        fix: "radii from a small set, fully rounded only where it means something",
    },
    Rule {
        id: "heavy-shadows",
        severity: Severity::Low,
        kind: Kind::Both,
        pattern: r"\bshadow-(xl|2xl)\b",
        unless_match_has: None,
        unless_file_has: None,
        at_least: 9,
        why: "large shadows on everything, so nothing is elevated",
        fix: "shadow only what sits above something else",
    },
];

/// Icon packages whose imports are counted, one entry per library.
const ICON_LIBRARIES: &[(&str, &str)] = &[
    ("lucide-react", "Lucide"),
    ("react-icons", "react-icons"),
    ("@heroicons/", "Heroicons"),
    ("@tabler/icons", "Tabler"),
    ("@phosphor-icons/", "Phosphor"),
    ("react-feather", "Feather"),
    ("@radix-ui/react-icons", "Radix icons"),
];

/// Directories never checked: dependencies, build output, git's store.
const SKIPPED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    ".next",
    "out",
    "coverage",
    ".svelte-kit",
    ".nuxt",
    "target",
];

/// The most a check reports, so its own output stays small.
const MAX_REPORTED: usize = 60;

fn kind_of(path: &Path) -> Option<Kind> {
    let name = path.file_name()?.to_str()?;
    if name.contains(".min.") {
        return None;
    }
    match path.extension()?.to_str()? {
        "html" | "htm" | "jsx" | "tsx" | "vue" | "svelte" | "astro" | "mdx" => Some(Kind::Markup),
        "css" | "scss" | "sass" | "less" => Some(Kind::Style),
        _ => None,
    }
}

/// Whether a path is interface code the check reads.
pub fn is_interface_file(path: &str) -> bool {
    kind_of(Path::new(path)).is_some()
}

fn compiled() -> &'static Vec<Regex> {
    static COMPILED: OnceLock<Vec<Regex>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        RULES
            .iter()
            .map(|rule| Regex::new(rule.pattern).expect("a valid rule"))
            .collect()
    })
}

/// Every interface file under `root`, or `root` itself when it is one.
pub fn interface_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        return kind_of(root)
            .map(|_| root.to_path_buf())
            .into_iter()
            .collect();
    }
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .filter_entry(|entry| {
            !entry
                .file_name()
                .to_str()
                .is_some_and(|name| SKIPPED_DIRS.contains(&name))
        })
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| entry.into_path())
        .filter(|path| kind_of(path).is_some())
        .filter(|path| std::fs::metadata(path).is_ok_and(|m| m.len() <= 1024 * 1024))
        .collect()
}

/// Check `files`, naming them relative to `workspace`. Findings are ordered
/// by severity, then by file and line.
pub fn check(workspace: &Path, files: &[PathBuf]) -> Vec<Finding> {
    let regexes = compiled();
    let mut by_rule: Vec<Vec<Finding>> = vec![Vec::new(); RULES.len()];
    let mut icon_sets: Vec<(&str, String, usize)> = Vec::new();
    for path in files {
        let Some(kind) = kind_of(path) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let shown = path
            .strip_prefix(workspace)
            .unwrap_or(path)
            .display()
            .to_string();
        for (index, rule) in RULES.iter().enumerate() {
            let reads = match (rule.kind, kind) {
                (Kind::Both, _) => true,
                (want, have) => want == have,
            };
            if !reads
                || rule
                    .unless_file_has
                    .is_some_and(|marker| text.contains(marker))
            {
                continue;
            }
            let lines: Vec<&str> = text.lines().collect();
            for (line_index, line) in lines.iter().enumerate() {
                for found in regexes[index].find_iter(line) {
                    if rule.id == "colour-outside-tokens"
                        && kind == Kind::Style
                        && defines_token(line, found.start())
                    {
                        continue;
                    }
                    if rule.id == "full-screen-section"
                        && frames_the_page(&lines, line_index, found.start(), kind)
                    {
                        continue;
                    }
                    if rule
                        .unless_match_has
                        .is_some_and(|marker| found.as_str().contains(marker))
                    {
                        continue;
                    }
                    by_rule[index].push(Finding {
                        severity: rule.severity,
                        rule: rule.id,
                        path: shown.clone(),
                        line: line_index + 1,
                        excerpt: excerpt(line, found.start()),
                        why: rule.why,
                        fix: rule.fix,
                    });
                    break;
                }
            }
        }
        if kind == Kind::Markup {
            for (line_index, line) in text.lines().enumerate() {
                if !line.contains("import") && !line.contains("require(") {
                    continue;
                }
                for (package, name) in ICON_LIBRARIES {
                    if line.contains(package) {
                        icon_sets.push((name, shown.clone(), line_index + 1));
                    }
                }
            }
        }
    }
    let mut findings: Vec<Finding> = Vec::new();
    for (index, found) in by_rule.into_iter().enumerate() {
        if found.len() >= RULES[index].at_least {
            findings.extend(found);
        }
    }
    let mut libraries: Vec<&str> = icon_sets.iter().map(|(name, _, _)| *name).collect();
    libraries.sort_unstable();
    libraries.dedup();
    if libraries.len() > 1 {
        let (_, path, line) = icon_sets[0].clone();
        findings.push(Finding {
            severity: Severity::Medium,
            rule: "several-icon-sets",
            path,
            line,
            excerpt: libraries.join(", "),
            why: "more than one icon library, so the icons do not match",
            fix: "keep one icon set for the whole product",
        });
    }
    findings.sort_by(|a, b| (a.severity, &a.path, a.line).cmp(&(b.severity, &b.path, b.line)));
    findings
}

/// Whether the match at `at` sits in a custom property's declaration
/// (`--ink: #15302a`), where a colour literal belongs.
fn defines_token(line: &str, at: usize) -> bool {
    let before = &line[..at];
    let start = before.rfind([';', '{']).map_or(0, |i| i + 1);
    before[start..].trim_start().starts_with("--")
}

/// Whether the match at `at` sizes the page's frame (the body, the root
/// element, `main`, an app shell) rather than a section: an application
/// filling the screen is not a hero pushing the page below the fold.
fn frames_the_page(lines: &[&str], line_index: usize, at: usize, kind: Kind) -> bool {
    static FRAME: OnceLock<Regex> = OnceLock::new();
    let frame = FRAME.get_or_init(|| {
        Regex::new(
            r#"(?i)(^|[\s,>+~(<])(html|body|main|:root)\b|#(root|app|__next)\b|[.#"'\s-](app|layout|shell|frame|wrapper)([\s"'{,.:>#-]|$)"#,
        )
        .expect("a valid pattern")
    });
    let line = &lines[line_index][..at];
    // In markup, the element the class is on: from its `<` to the match.
    if kind == Kind::Markup {
        if let Some(open) = line.rfind('<') {
            return frame.is_match(&line[open..]);
        }
    }
    // In a stylesheet or a `<style>` block, the selector before the nearest
    // `{` above the declaration.
    let mut before = line;
    let mut index = line_index;
    let selector = loop {
        if let Some(open) = before.rfind('{') {
            let head = &before[..open];
            let start = head.rfind(['}', ';', '>']).map_or(0, |i| i + 1);
            break head[start..].trim();
        }
        if before.contains('}') || index == 0 {
            return false;
        }
        index -= 1;
        before = lines[index];
    };
    // Only the last compound of each selector in a list says what is sized.
    selector
        .split(',')
        .any(|part| frame.is_match(part.split_whitespace().last().unwrap_or("")))
}

fn excerpt(line: &str, at: usize) -> String {
    const WIDTH: usize = 100;
    let chars: Vec<char> = line.chars().collect();
    let at = line[..at].chars().count();
    let start = at.saturating_sub(WIDTH / 3);
    let end = (start + WIDTH).min(chars.len());
    let mut shown: String = chars[start..end]
        .iter()
        .collect::<String>()
        .trim()
        .to_owned();
    if start > 0 {
        shown.insert(0, '…');
    }
    if end < chars.len() {
        shown.push('…');
    }
    shown
}

/// The findings as the agent and the user read them.
pub fn report(findings: &[Finding], files: usize) -> String {
    if findings.is_empty() {
        return format!("No marks of generated work found in {files} interface files.");
    }
    let count = |severity| findings.iter().filter(|f| f.severity == severity).count();
    let mut out = format!(
        "{} findings in {files} interface files: {} high, {} medium, {} low.\n",
        findings.len(),
        count(Severity::High),
        count(Severity::Medium),
        count(Severity::Low)
    );
    for (index, finding) in findings.iter().take(MAX_REPORTED).enumerate() {
        out.push_str(&format!(
            "{}. {} {} {}:{}  {}\n   {}. Fix: {}.\n",
            index + 1,
            finding.severity.label(),
            finding.rule,
            finding.path,
            finding.line,
            finding.excerpt,
            finding.why,
            finding.fix
        ));
    }
    if findings.len() > MAX_REPORTED {
        out.push_str(&format!("and {} more.\n", findings.len() - MAX_REPORTED));
    }
    out.push_str(
        "Each is a candidate: keep one that serves a stated purpose (the brand's own \
         colour, a real figure with a source) and say why; fix the rest.",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(files: &[(&str, &str)]) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("enx-ui-check-{}", uuid::Uuid::new_v4()));
        let mut paths = Vec::new();
        for (name, body) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, body).unwrap();
            paths.push(path);
        }
        (root, paths)
    }

    fn rules(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn filling_the_screen_is_fine_for_the_frame_and_not_for_a_section() {
        let css = ".app {\n  display: grid;\n  min-height: 100vh;\n}\nhtml, body { min-height: 100vh; }\n\
                   .shell > main { height: 100vh; }\n.hero {\n  min-height: 100vh;\n}\n\
                   section.intro { height: 100vh; }\n";
        let jsx = "<body className=\"min-h-screen antialiased\">\n\
                   <div className=\"app-shell min-h-screen\">\n\
                   <section className=\"h-screen flex items-center\">\n";
        let html = "<style>\n#root { min-height: 100vh; }\n.banner { height: 100vh; }\n</style>\n";
        let (root, files) = scratch(&[
            ("styles.css", css),
            ("app/layout.tsx", jsx),
            ("index.html", html),
        ]);
        let found: Vec<(String, usize)> = check(&root, &files)
            .into_iter()
            .filter(|f| f.rule == "full-screen-section")
            .map(|f| (f.path, f.line))
            .collect();
        assert_eq!(
            found,
            [
                ("app/layout.tsx".to_owned(), 3),
                ("index.html".to_owned(), 3),
                ("styles.css".to_owned(), 8),
                ("styles.css".to_owned(), 10),
            ],
            "the section, the banner, the hero and the intro, not the frames"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_generated_page_is_caught() {
        let page = r##"<section class="h-screen bg-gradient-to-r from-purple-600 to-pink-500">
  <p class="uppercase tracking-widest">Features</p>
  <h1 class="bg-clip-text">Unlock seamless productivity 🚀</h1>
  <p>Trusted by 10,000+ users — and 99.9% uptime.</p>
  <a href="#">Get Started</a>
  <img src="hero.png">
  <button onClick={() => {}}>Get Started</button>
</section>"##;
        let (root, files) = scratch(&[("app/page.tsx", page)]);
        let found = check(&root, &files);
        let _ = std::fs::remove_dir_all(&root);
        let ids = rules(&found);
        for id in [
            "invented-figure",
            "dead-link",
            "empty-handler",
            "image-without-alt",
            "default-gradient",
            "gradient-text",
            "buzzword",
            "generic-action",
            "emoji",
            "em-dash",
            "wide-caps-label",
            "full-screen-section",
        ] {
            assert!(ids.contains(&id), "{id} missing from {ids:?}");
        }
        assert_eq!(found[0].severity, Severity::High, "highest first");
        assert!(found.iter().all(|f| f.path == "app/page.tsx"));
    }

    #[test]
    fn a_plain_page_passes() {
        let page = r#"<main>
  <h1>Fisioterapi untuk nyeri punggung dan cedera olahraga</h1>
  <p>Kami periksa penyebabnya dulu, lalu menyusun latihan untuk di rumah.</p>
  <a href="https://wa.me/6280000000000">Book via WhatsApp</a>
  <img src="room.jpg" alt="Ruang terapi dengan dua bed">
</main>"#;
        let css = ":root {\n  --ink: #15302a;\n  --paper: #f6f2e9;\n}\nbody { color: var(--ink); }\n:focus-visible { outline: 2px solid var(--ink); }\n";
        let (root, files) = scratch(&[("index.html", page), ("styles.css", css)]);
        let found = check(&root, &files);
        let _ = std::fs::remove_dir_all(&root);
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn colours_outside_the_tokens_are_low() {
        let css = ":root { --ink: #111; }\n.card { color: #333; background: rgba(0,0,0,.5); }\n";
        let (root, files) = scratch(&[("a.css", css)]);
        let found = check(&root, &files);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(rules(&found), ["colour-outside-tokens"]);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].severity, Severity::Low);
    }

    #[test]
    fn focus_removed_is_fine_with_a_replacement() {
        let (root, files) = scratch(&[
            ("a.css", "button { outline: none; }\n"),
            (
                "b.css",
                "button { outline: none; }\nbutton:focus-visible { outline: 2px solid; }\n",
            ),
        ]);
        let found = check(&root, &files);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(rules(&found), ["focus-removed"]);
        assert_eq!(found[0].path, "a.css");
    }

    #[test]
    fn overuse_rules_wait_for_many() {
        let one = "<div class=\"backdrop-blur\">x</div>\n";
        let (root, files) = scratch(&[("a.tsx", one)]);
        assert!(check(&root, &files).is_empty(), "one blur is a choice");
        let _ = std::fs::remove_dir_all(&root);
        let many = one.repeat(3);
        let (root, files) = scratch(&[("a.tsx", &many)]);
        let found = check(&root, &files);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(rules(&found), ["glass", "glass", "glass"]);
    }

    #[test]
    fn two_icon_libraries_are_one_finding() {
        let a = "import { Clock } from 'lucide-react';\n";
        let b = "import { FaBeer } from 'react-icons/fa';\n";
        let (root, files) = scratch(&[("a.tsx", a), ("b.tsx", b)]);
        let found = check(&root, &files);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(rules(&found), ["several-icon-sets"]);
        assert!(found[0].excerpt.contains("Lucide") && found[0].excerpt.contains("react-icons"));
    }

    #[test]
    fn dependencies_and_build_output_are_not_checked() {
        let bad = "<a href=\"#\">Get Started</a>\n";
        let (root, _) = scratch(&[
            ("node_modules/x/index.html", bad),
            (".next/server/page.html", bad),
            ("src/app.tsx", "<main>ok</main>\n"),
        ]);
        let files = interface_files(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(files.len(), 1, "{files:?}");
    }

    #[test]
    fn the_report_lists_findings_by_priority() {
        let (root, files) = scratch(&[("a.html", "<a href=\"#\">Learn more</a>\n")]);
        let found = check(&root, &files);
        let text = report(&found, files.len());
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            text.starts_with("2 findings in 1 interface files: 1 high, 1 medium, 0 low."),
            "{text}"
        );
        assert!(text.contains("1. HIGH dead-link a.html:1"), "{text}");
        assert!(report(&[], 3).starts_with("No marks of generated work found in 3"));
    }
}

/// Marks the message the harness adds when an interface agent is about to
/// finish with findings in what it changed.
const GATE_MARKER: &str = "[ui_check]";
const GATE_INSTRUCTION: &str = "Before you finish: fix these in the files you changed, or say \
     for each one why it stays. Then report again.";

/// The message that sends an agent back to its findings.
pub fn gate_message(report: &str) -> String {
    format!("{GATE_MARKER} The harness checked the interface files you changed.\n{report}\n\n{GATE_INSTRUCTION}")
}

/// The report inside a gate message, or None for any other message.
pub fn parse_gate_message(content: &str) -> Option<&str> {
    let rest = content.strip_prefix(GATE_MARKER)?;
    let rest = rest.split_once('\n').map_or("", |(_, body)| body);
    Some(
        rest.strip_suffix(GATE_INSTRUCTION)
            .unwrap_or(rest)
            .trim_end(),
    )
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    #[test]
    fn a_gate_message_carries_its_report() {
        let message = gate_message("2 findings…");
        assert_eq!(parse_gate_message(&message), Some("2 findings…"));
        assert_eq!(parse_gate_message("a user's own words"), None);
    }
}
