//! Which shell commands change things, for agents that only look and check.
//!
//! The orchestrator and `review` have `bash` to look (git status, log and
//! diff, versions) and to check (run the tests, the build), and no tool that
//! edits. This catches the ways a model changes files through a shell anyway:
//! a redirect, `sed -i`, `rm`, `git commit`, an install, an inline script
//! that writes, a dev server that never returns. It is a guard against
//! habit, not a sandbox: a determined program can still write.

/// Why `command` would change files, the repository or the machine, or
/// `None` when it only reads, builds or runs checks.
pub(crate) fn changes_files(command: &str) -> Option<String> {
    let bare = unquoted(&without_heredoc_bodies(command));
    if let Some(target) = redirect_target(&bare) {
        return Some(format!("writes to `{target}` with a redirect"));
    }
    let mut interpreted = false;
    for simple in bare.split([';', '|', '&', '\n', '(', ')', '`']) {
        let words: Vec<&str> = simple.split_whitespace().collect();
        let Some((program, args)) = command_of(&words) else {
            continue;
        };
        interpreted |= INTERPRETERS.contains(&program);
        if let Some(reason) = judge(program, args) {
            return Some(reason);
        }
    }
    // An inline script (`python3 -c`, `node -e`, a heredoc) is read whole,
    // quotes and all, for the calls that write.
    if interpreted {
        if let Some(call) = script_writes().find(command) {
            return Some(format!("writes files from a script (`{}`)", call.as_str()));
        }
    }
    None
}

const INTERPRETERS: &[&str] = &[
    "python", "python3", "node", "deno", "bun", "ruby", "perl", "php",
];

fn script_writes() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        regex::Regex::new(
            r#"open\([^)]*,\s*(mode\s*=\s*)?['"][wax]|\.write_(text|bytes)\(|\b(writeFile|appendFile|unlink|rmdir|mkdir|rename|copyFile|rm)(Sync)?\(|fs\.(write|rm|unlink|rename|mkdir|copy)|shutil\.(copy|move|rmtree)|os\.(remove|unlink|rename|replace|makedirs|mkdir|rmdir)\(|File::create|fs::write"#,
        )
        .expect("a valid pattern")
    })
}

/// Programs that change files whatever their arguments.
const WRITERS: &[&str] = &[
    "rm", "rmdir", "mv", "cp", "touch", "mkdir", "ln", "chmod", "chown", "truncate", "unlink",
    "shred", "patch", "tee", "dd", "rsync", "install",
];

/// Words that run the command after them.
const WRAPPERS: &[&str] = &[
    "sudo", "env", "command", "exec", "time", "nohup", "xargs", "nice",
];

/// The program a simple command runs and its arguments, past `FOO=bar`
/// assignments and wrappers such as `sudo` and `xargs` (with their flags).
fn command_of<'a>(words: &'a [&'a str]) -> Option<(&'a str, &'a [&'a str])> {
    let mut rest = words;
    while let Some((first, tail)) = rest.split_first() {
        let assignment = first.contains('=') && !first.starts_with('-');
        let wrapper_flag = first.starts_with('-') && rest.len() < words.len();
        if assignment || WRAPPERS.contains(first) || wrapper_flag {
            rest = tail;
        } else {
            break;
        }
    }
    let (program, args) = rest.split_first()?;
    Some((program.rsplit('/').next().unwrap_or(program), args))
}

fn judge(program: &str, args: &[&str]) -> Option<String> {
    let has = |flag: &str| args.contains(&flag);
    let first_arg = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .copied()
        .unwrap_or("");

    if WRITERS.contains(&program) {
        return Some(format!("runs `{program}`"));
    }
    if matches!(program, "sed" | "perl" | "ruby")
        && args.iter().any(|a| {
            *a == "--in-place" || (a.starts_with('-') && !a.starts_with("--") && a.contains('i'))
        })
    {
        return Some(format!("edits in place with `{program} -i`"));
    }
    if program == "find" {
        if has("-delete") {
            return Some("deletes with `find -delete`".into());
        }
        if let Some(at) = args
            .iter()
            .position(|a| matches!(*a, "-exec" | "-execdir" | "-ok"))
        {
            if let Some((inner, inner_args)) = command_of(&args[at + 1..]) {
                if let Some(reason) = judge(inner, inner_args) {
                    return Some(reason);
                }
            }
        }
    }
    if let Some(flag) = ["--write", "--fix"].into_iter().find(|flag| has(flag)) {
        return Some(format!("rewrites files with `{flag}`"));
    }
    if program == "wget"
        && !args
            .iter()
            .any(|a| matches!(*a, "-O-" | "-qO-" | "--spider"))
    {
        return Some("downloads a file with `wget`".into());
    }
    if program == "curl"
        && args
            .iter()
            .any(|a| matches!(*a, "-o" | "-O" | "--output" | "--remote-name"))
    {
        return Some("downloads a file with `curl -o`".into());
    }
    if program == "unzip"
        || (program == "tar"
            && args
                .first()
                .is_some_and(|a| !a.contains('t') && (a.contains('x') || a.contains('c'))))
    {
        return Some(format!("unpacks or packs files with `{program}`"));
    }
    let checking = has("--check") || has("--diff");
    match program {
        "git" => git(args),
        "npm" | "pnpm" | "yarn" | "bun" => {
            if program == "yarn" && args.is_empty() {
                return Some("installs packages with `yarn`".into());
            }
            let script = if first_arg == "run" {
                args.iter()
                    .filter(|a| !a.starts_with('-'))
                    .nth(1)
                    .copied()
                    .unwrap_or("")
            } else {
                first_arg
            };
            if matches!(
                first_arg,
                "install"
                    | "i"
                    | "add"
                    | "remove"
                    | "rm"
                    | "uninstall"
                    | "un"
                    | "ci"
                    | "update"
                    | "up"
                    | "upgrade"
                    | "link"
                    | "unlink"
                    | "dedupe"
                    | "prune"
                    | "init"
                    | "create"
            ) {
                Some(format!("changes packages with `{program} {first_arg}`"))
            } else if matches!(script, "dev" | "start" | "serve" | "watch" | "preview") {
                Some(format!(
                    "starts a server that keeps running (`{program} {}`)",
                    args.join(" ")
                ))
            } else {
                None
            }
        }
        "pip" | "pip3" | "uv" | "poetry" | "pipenv" | "gem" | "composer" | "bundle" => matches!(
            first_arg,
            "install"
                | "uninstall"
                | "add"
                | "remove"
                | "sync"
                | "lock"
                | "update"
                | "require"
                | "pip"
        )
        .then(|| format!("changes packages with `{program} {first_arg}`")),
        "python" | "python3" if args.windows(2).any(|w| w == ["-m", "pip"]) => args
            .contains(&"install")
            .then(|| "installs packages with pip".into()),
        "python" | "python3" if args.windows(2).any(|w| w == ["-m", "http.server"]) => {
            Some("starts a server that keeps running (`http.server`)".into())
        }
        "cargo" => match first_arg {
            "add" | "remove" | "rm" | "install" | "uninstall" | "update" | "fix" | "new"
            | "init" | "publish" | "watch" => Some(format!("runs `cargo {first_arg}`")),
            "fmt" if !checking => Some("rewrites files with `cargo fmt`".into()),
            _ => None,
        },
        "rustfmt" | "black" if !checking => Some(format!("rewrites files with `{program}`")),
        "gofmt" if has("-w") => Some("rewrites files with `gofmt -w`".into()),
        "go" => matches!(first_arg, "get" | "install" | "generate" | "fmt" | "mod")
            .then(|| format!("runs `go {first_arg}`")),
        "brew" | "apt" | "apt-get" | "yum" | "dnf" | "pacman" | "port" => matches!(
            first_arg,
            "install"
                | "uninstall"
                | "remove"
                | "reinstall"
                | "upgrade"
                | "update"
                | "link"
                | "unlink"
                | "tap"
                | "cleanup"
                | "purge"
                | "-S"
                | "-R"
                | "-Syu"
        )
        .then(|| format!("changes the machine with `{program} {first_arg}`")),
        "docker" | "podman" => {
            let compose = first_arg == "compose";
            let action = if compose {
                args.iter()
                    .filter(|a| !a.starts_with('-'))
                    .nth(1)
                    .copied()
                    .unwrap_or("")
            } else {
                first_arg
            };
            matches!(
                action,
                "run"
                    | "rm"
                    | "rmi"
                    | "build"
                    | "stop"
                    | "kill"
                    | "restart"
                    | "exec"
                    | "push"
                    | "pull"
                    | "up"
                    | "down"
                    | "start"
                    | "create"
                    | "prune"
                    | "system"
                    | "volume"
            )
            .then(|| format!("changes containers with `{program} {}`", args.join(" ")))
        }
        _ => None,
    }
}

/// The git subcommands that change the working tree, the index, the
/// history or a remote.
fn git(args: &[&str]) -> Option<String> {
    // `-C <path>` and `-c <key=value>` take a value before the subcommand.
    let mut rest = args;
    while let Some((first, tail)) = rest.split_first() {
        if matches!(*first, "-C" | "-c") {
            rest = tail.get(1..).unwrap_or(&[]);
        } else if first.starts_with('-') {
            rest = tail;
        } else {
            break;
        }
    }
    let (sub, sub_args) = rest.split_first()?;
    let names = sub_args.iter().any(|a| !a.starts_with('-'));
    let changes = match *sub {
        "add" | "am" | "apply" | "checkout" | "cherry-pick" | "clean" | "clone" | "commit"
        | "init" | "merge" | "mv" | "pull" | "push" | "rebase" | "reset" | "restore" | "revert"
        | "rm" | "switch" | "worktree" | "gc" | "prune" => true,
        "stash" => !matches!(sub_args.first(), Some(&"list") | Some(&"show")),
        "tag" => names && !sub_args.iter().any(|a| matches!(*a, "-l" | "--list")),
        "branch" => {
            names
                || sub_args.iter().any(|a| {
                    matches!(
                        *a,
                        "-d" | "-D" | "-m" | "-M" | "-c" | "-C" | "--delete" | "--move"
                    )
                })
        }
        _ => false,
    };
    changes.then(|| format!("changes the repository with `git {sub}`"))
}

/// `command` with the contents of quoted strings blanked, so `echo "a > b"`
/// and `grep "rm -rf"` read as what they are.
fn unquoted(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in command.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                    out.push(c);
                    continue;
                }
                out.push(if c == '\n' { '\n' } else { ' ' });
            }
            None => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '\'' || c == '"' {
                    quote = Some(c);
                }
                out.push(c);
            }
        }
    }
    out
}

/// `command` without the bodies of its heredocs, which are input, not
/// shell: `cat <<EOF` followed by HTML is not a redirect.
fn without_heredoc_bodies(command: &str) -> String {
    let mut out = Vec::new();
    let mut terminator: Option<String> = None;
    for line in command.lines() {
        if let Some(end) = &terminator {
            if line.trim() == end {
                terminator = None;
            }
            continue;
        }
        out.push(line);
        if let Some(at) = line.find("<<") {
            let word: String = line[at + 2..]
                .trim_start_matches(['-', '~'])
                .trim_start()
                .trim_start_matches(['\'', '"'])
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !word.is_empty() {
                terminator = Some(word);
            }
        }
    }
    out.join("\n")
}

/// The file an output redirect writes to, skipping `2>&1`, `>&2` and
/// `/dev/null`.
fn redirect_target(bare: &str) -> Option<String> {
    let chars: Vec<char> = bare.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '>' {
            at += 1;
            continue;
        }
        let mut next = at + 1;
        if chars.get(next) == Some(&'>') || chars.get(next) == Some(&'|') {
            next += 1;
        }
        match chars.get(next) {
            // `>&2`, `2>&1`: a descriptor, not a file.
            Some('&') => {
                at = next + 1;
                continue;
            }
            // `>(...)`: process substitution.
            Some('(') => {
                at = next + 1;
                continue;
            }
            _ => {}
        }
        let target: String = chars[next..]
            .iter()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| !c.is_whitespace() && !matches!(c, ';' | '|' | '&' | ')' | '<' | '>'))
            .collect();
        let harmless = target.is_empty() || target.starts_with("/dev/");
        if !harmless {
            return Some(target);
        }
        at = next;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::changes_files;

    #[test]
    fn looking_and_checking_run() {
        for command in [
            "git status",
            "git log --oneline -5",
            "git diff --stat HEAD~3",
            "git show HEAD:src/lib.rs | head -40",
            "git branch -a",
            "git -C crates/app stash list",
            "git tag --list",
            "ls -la src",
            "cat package.json",
            "wc -l src/*.rs",
            "find . -name '*.rs' -not -path './target/*' | head",
            "cargo test -q 2>&1 | tail -20",
            "cargo build --release",
            "cargo fmt --check",
            "npm test",
            "npm run build",
            "pnpm lint",
            "node --version && python3 --version",
            "grep -rn \"fn main\" src 2>/dev/null",
            "echo \"a > b\"",
            "rg foo > /dev/null && echo found",
            "sed -n '1,20p' src/main.rs",
            "awk '$3 > 100 {print $1}' data.txt",
            "cat <<EOF\n<div>hi</div>\nEOF",
            "ps aux | grep -c node",
            "lsof -i :3000",
            "du -sh target",
            "docker ps",
            "grep -rn \"rm -rf\" scripts",
            "grep -rn \"writeFileSync(\" src",
            "python3 -c \"import json; print(json.load(open('package.json'))['version'])\"",
            "node -e \"console.log(require('./package.json').version)\"",
            "curl -s https://example.com/health",
            "tar -tzf release.tgz",
        ] {
            assert_eq!(changes_files(command), None, "{command}");
        }
    }

    #[test]
    fn changes_are_refused_with_the_reason() {
        for (command, reason) in [
            ("echo hi > notes.txt", "`notes.txt` with a redirect"),
            ("cat >> README.md <<'EOF'\nmore\nEOF", "`README.md`"),
            ("cargo test > out.log 2>&1", "`out.log`"),
            ("printf x | tee out.txt", "`tee`"),
            ("rm -rf dist", "`rm`"),
            ("cd src && mv a.rs b.rs", "`mv`"),
            ("mkdir -p src/new", "`mkdir`"),
            ("sed -i 's/a/b/' f.txt", "sed -i"),
            ("sed -i '' 's/a/b/' f.txt", "sed -i"),
            ("perl -pi -e 's/a/b/' f.txt", "perl -i"),
            ("find . -name '*.tmp' -delete", "find -delete"),
            ("find . -name '*.log' -exec rm {} +", "`rm`"),
            ("ls *.bak | xargs rm", "`rm`"),
            ("git commit -am 'fix'", "git commit"),
            ("git push origin main", "git push"),
            ("git checkout -- src/main.rs", "git checkout"),
            ("git stash", "git stash"),
            ("git reset --hard HEAD~1", "git reset"),
            ("git branch new-feature", "git branch"),
            ("npm install", "npm install"),
            ("npm i lodash", "npm i"),
            ("pnpm add zod", "pnpm add"),
            ("yarn", "yarn"),
            ("pip install requests", "pip install"),
            ("python3 -m pip install requests", "pip"),
            ("cargo add serde", "cargo add"),
            ("cargo fmt", "cargo fmt"),
            ("brew install jq", "brew install"),
            ("npx prettier --write .", "--write"),
            ("npx eslint --fix src", "--fix"),
            ("npm run dev", "server"),
            ("pnpm dev", "server"),
            ("yarn start", "server"),
            ("python3 -m http.server 8000", "server"),
            ("python3 -c \"open('x.txt', 'w').write('y')\"", "script"),
            (
                "node -e \"require('fs').writeFileSync('x', 'y')\"",
                "script",
            ),
            ("docker compose up -d", "docker"),
            (
                "python3 - <<'EOF'\nfrom pathlib import Path\nPath('x').write_text('y')\nEOF",
                "script",
            ),
            ("wget https://example.com/a.zip", "wget"),
            ("curl -o a.zip https://example.com/a.zip", "curl"),
            ("tar -xzf a.tgz", "tar"),
        ] {
            let found = changes_files(command);
            assert!(
                found.as_deref().is_some_and(|r| r.contains(reason)),
                "{command}: {found:?}"
            );
        }
    }
}
