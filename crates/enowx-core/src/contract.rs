//! The contract between agents working at the same time: a file one agent is
//! editing is closed to the others until that agent finishes.
//!
//! A simpler form of the file claims in succubus, which holds them across
//! sessions and processes. Here they live for one run, in memory, across the
//! agents of that run: the one the user talks to and the sub-agents it
//! delegated to, several of which may be at work at once.
//!
//! An agent claims a file by editing it (`write`, `edit`, `multi_edit`), and
//! holds it until its run ends. Another agent that tries to edit it is
//! refused, and told who has it. An agent waiting on its sub-agents does not
//! block them: its claims pass to the delegate that needs the file.

use std::collections::HashMap;
use std::sync::Mutex;

/// Who holds a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The session doing the editing.
    pub session: String,
    /// The agent in it, as its id (`fe`).
    pub agent: String,
}

/// The files claimed in one run, and how its sessions descend from each
/// other.
#[derive(Default)]
pub struct Board {
    claims: Mutex<HashMap<String, Claim>>,
    /// Session id to its parent's, for the sessions of this run.
    parents: Mutex<HashMap<String, String>>,
}

impl Board {
    /// Record that `child` was delegated from `parent`.
    pub fn branch(&self, child: &str, parent: &str) {
        self.parents
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(child.to_owned(), parent.to_owned());
    }

    /// Claim `path` for `claim.session`, or say who holds it. A claim held by
    /// the session itself, or by one of its ancestors (which is waiting on
    /// it), is taken over.
    pub fn claim(&self, path: &str, claim: Claim) -> Result<(), Claim> {
        let path = normal(path);
        let mut claims = self.claims.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(holder) = claims.get(&path) {
            if holder.session != claim.session && !self.descends(&claim.session, &holder.session) {
                return Err(holder.clone());
            }
        }
        claims.insert(path, claim);
        Ok(())
    }

    /// Free every file `session` holds: it has finished.
    pub fn release(&self, session: &str) {
        self.claims
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, holder| holder.session != session);
    }

    /// Free everything: the run is over, and no agent is at work.
    pub fn clear(&self) {
        self.claims
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.parents
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// The files claimed now, with their holders, sorted by path.
    pub fn held(&self) -> Vec<(String, Claim)> {
        let mut held: Vec<(String, Claim)> = self
            .claims
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(path, claim)| (path.clone(), claim.clone()))
            .collect();
        held.sort_by(|a, b| a.0.cmp(&b.0));
        held
    }

    /// Whether `session` was delegated, directly or not, from `ancestor`.
    fn descends(&self, session: &str, ancestor: &str) -> bool {
        let parents = self.parents.lock().unwrap_or_else(|e| e.into_inner());
        let mut at = session;
        // Bounded, in case a record ever loops.
        for _ in 0..64 {
            match parents.get(at) {
                Some(parent) if parent == ancestor => return true,
                Some(parent) => at = parent,
                None => return false,
            }
        }
        false
    }
}

/// One spelling per file: `./src/a.ts` and `src/a.ts` are the same claim.
fn normal(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The refusal an agent gets for a file another agent holds.
pub fn refusal(path: &str, holder: &Claim) -> String {
    format!(
        "`{path}` is being edited by {} ({}), another agent working alongside you, and \
         stays closed until it finishes. Work on your other files first. If you cannot \
         finish without this one, leave it and say so in your report.",
        crate::agent_def::display_name(&holder.agent),
        holder.agent
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn by(session: &str, agent: &str) -> Claim {
        Claim {
            session: session.into(),
            agent: agent.into(),
        }
    }

    #[test]
    fn a_file_being_edited_is_closed_to_other_agents() {
        let board = Board::default();
        board.branch("fe-1", "main");
        board.branch("fe-2", "main");
        assert!(board.claim("src/header.tsx", by("fe-1", "fe")).is_ok());
        assert!(
            board.claim("./src/header.tsx", by("fe-1", "fe")).is_ok(),
            "its own again"
        );
        let refused = board.claim("src/header.tsx", by("fe-2", "fe")).unwrap_err();
        assert_eq!(refused.session, "fe-1");
        assert!(board.claim("src/footer.tsx", by("fe-2", "fe")).is_ok());

        board.release("fe-1");
        assert!(
            board.claim("src/header.tsx", by("fe-2", "fe")).is_ok(),
            "free once done"
        );
    }

    #[test]
    fn a_delegate_takes_over_what_its_waiting_parent_holds() {
        let board = Board::default();
        board.branch("be-1", "main");
        board.branch("test-1", "be-1");
        assert!(board.claim("api.rs", by("main", "general")).is_ok());
        assert!(
            board.claim("api.rs", by("test-1", "test")).is_ok(),
            "a grandchild"
        );
        // The parent does not get it back while its delegate holds it.
        assert!(board.claim("api.rs", by("be-1", "be")).is_err());
        // A sibling line never takes over.
        board.branch("db-1", "main");
        assert!(board.claim("api.rs", by("db-1", "db")).is_err());
    }

    #[test]
    fn the_refusal_names_the_holder() {
        let message = refusal("src/a.ts", &by("x", "fe"));
        assert!(message.contains("Frontend (fe)"), "{message}");
        assert!(
            message.contains("Work on your other files first"),
            "{message}"
        );
    }
}
