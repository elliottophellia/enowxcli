//! Questions from the agent holding the conversation, answered in a panel
//! fixed above the composer.
//!
//! The turn waits on them. The panel has the keyboard: the arrows and digits
//! choose, the last row is always "Other" for an answer in the user's own
//! words, `n` writes a note on the highlighted option, and with several
//! questions ←/→ (or Tab) move between them. Enter answers one and moves on;
//! on the last it sends them all.

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use enowx_core::ask::{Answer, Question, Reply};

/// Questions the agent is waiting on, and the answers so far.
pub(crate) struct PendingQuestion {
    pub(crate) id: String,
    pub(crate) agent: String,
    pub(crate) questions: Vec<Question>,
    /// Which question is on screen.
    pub(crate) current: usize,
    /// Per question, the highlighted row: an option, or `options.len()` for
    /// the "Other" row.
    pub(crate) cursors: Vec<usize>,
    /// Per question taking one answer, the row chosen.
    pub(crate) selected: Vec<Option<usize>>,
    /// Per question taking several answers, which options are ticked.
    pub(crate) ticked: Vec<Vec<bool>>,
    /// Per question, what was typed in the "Other" row.
    pub(crate) other: Vec<String>,
    /// Per question and option, the note the user wrote on it.
    pub(crate) notes: Vec<Vec<String>>,
    /// The note being written, by option on the current question, and what
    /// it said before, for Esc to put back.
    pub(crate) editing: Option<(usize, String)>,
}

impl PendingQuestion {
    pub(crate) fn new(id: String, agent: String, questions: Vec<Question>) -> Self {
        let per_option = |fill: bool| -> Vec<Vec<bool>> {
            questions
                .iter()
                .map(|q| vec![fill; q.options.len()])
                .collect()
        };
        let ticked = per_option(false);
        let notes = questions
            .iter()
            .map(|q| vec![String::new(); q.options.len()])
            .collect();
        let count = questions.len();
        Self {
            id,
            agent,
            questions,
            current: 0,
            cursors: vec![0; count],
            selected: vec![None; count],
            ticked,
            other: vec![String::new(); count],
            notes,
            editing: None,
        }
    }

    pub(crate) fn question(&self) -> &Question {
        &self.questions[self.current]
    }

    /// The "Other" row's index on the current question.
    pub(crate) fn other_row(&self) -> usize {
        self.question().options.len()
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursors[self.current]
    }

    pub(crate) fn on_other(&self) -> bool {
        self.cursor() == self.other_row()
    }

    /// The reply to question `index` as things stand.
    pub(crate) fn reply(&self, index: usize) -> Reply {
        let question = &self.questions[index];
        let label = |option: usize| question.options[option].label.clone();
        let chosen: Vec<usize> = if question.multiple {
            self.ticked[index]
                .iter()
                .enumerate()
                .filter(|(_, on)| **on)
                .map(|(option, _)| option)
                .collect()
        } else {
            self.selected[index]
                .filter(|row| *row < question.options.len())
                .into_iter()
                .collect()
        };
        let other = if question.multiple || self.selected[index] == Some(question.options.len()) {
            self.other[index].trim().to_owned()
        } else {
            String::new()
        };
        Reply {
            notes: chosen
                .iter()
                .filter(|option| !self.notes[index][**option].trim().is_empty())
                .map(|option| (label(*option), self.notes[index][*option].trim().to_owned()))
                .collect(),
            chosen: chosen.into_iter().map(label).collect(),
            other,
        }
    }

    fn answer(&self) -> Answer {
        Answer {
            replies: (0..self.questions.len()).map(|q| self.reply(q)).collect(),
        }
    }
}

impl App {
    /// Keys while questions wait. True when the panel used the key; false
    /// lets it through, which for Esc stops the turn.
    pub(crate) fn question_key(&mut self, key: KeyEvent) -> bool {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        let Some(pending) = self.question.as_mut() else {
            return false;
        };
        let current = pending.current;

        // Writing a note: the keys are the note's until Enter or Esc.
        if let Some((option, before)) = pending.editing.clone() {
            let note = &mut pending.notes[current][option];
            match key.code {
                KeyCode::Char(c) => note.push(c),
                KeyCode::Backspace => {
                    note.pop();
                }
                KeyCode::Enter => pending.editing = None,
                KeyCode::Esc => {
                    *note = before;
                    pending.editing = None;
                }
                _ => {}
            }
            return true;
        }

        let rows = pending.other_row() + 1;
        let count = pending.questions.len();
        match key.code {
            KeyCode::Up => {
                pending.cursors[current] = (pending.cursor() + rows - 1) % rows;
                true
            }
            KeyCode::Down => {
                pending.cursors[current] = (pending.cursor() + 1) % rows;
                true
            }
            KeyCode::Left | KeyCode::BackTab => {
                pending.current = current.saturating_sub(1);
                true
            }
            KeyCode::Right | KeyCode::Tab => {
                pending.current = (current + 1).min(count - 1);
                true
            }
            KeyCode::Enter => {
                self.confirm_current();
                true
            }
            KeyCode::Esc => false,
            // On the "Other" row, typing is the answer.
            KeyCode::Backspace if pending.on_other() => {
                pending.other[current].pop();
                true
            }
            KeyCode::Char(c) if pending.on_other() => {
                pending.other[current].push(c);
                true
            }
            KeyCode::Char(' ') if pending.question().multiple => {
                let option = pending.cursor();
                pending.ticked[current][option] = !pending.ticked[current][option];
                true
            }
            KeyCode::Char('n') => {
                let option = pending.cursor();
                let before = pending.notes[current][option].clone();
                pending.editing = Some((option, before));
                true
            }
            KeyCode::Char(digit @ '1'..='9') => {
                let row = digit as usize - '1' as usize;
                if row < rows {
                    self.pick_row(row);
                }
                true
            }
            // Nothing else types while the panel has the keyboard: a stray
            // letter must not land in the composer behind it.
            KeyCode::Char(_) | KeyCode::Backspace => true,
            _ => false,
        }
    }

    /// A row picked by its number or a click. An option answers a question
    /// taking one answer, or is ticked in one taking several; the "Other" row
    /// takes the highlight so typing fills it.
    pub(crate) fn pick_row(&mut self, row: usize) {
        let Some(pending) = self.question.as_mut() else {
            return;
        };
        let current = pending.current;
        if row > pending.other_row() {
            return;
        }
        pending.cursors[current] = row;
        if row == pending.other_row() {
            return;
        }
        if pending.question().multiple {
            pending.ticked[current][row] = !pending.ticked[current][row];
        } else {
            self.confirm_current();
        }
    }

    /// Enter on the question on screen: take its answer, then move to the
    /// next question, or send them all from the last.
    fn confirm_current(&mut self) {
        let Some(pending) = self.question.as_mut() else {
            return;
        };
        let current = pending.current;
        let row = pending.cursor();
        let question = pending.question().clone();
        if row == pending.other_row() {
            if pending.other[current].trim().is_empty() {
                self.status = "type your answer, or choose an option".into();
                return;
            }
            if !question.multiple {
                pending.selected[current] = Some(row);
            }
        } else if question.multiple {
            // Enter with nothing ticked means the highlighted one.
            if !pending.ticked[current].iter().any(|on| *on) {
                pending.ticked[current][row] = true;
            }
        } else {
            pending.selected[current] = Some(row);
        }
        if current + 1 < pending.questions.len() {
            pending.current = current + 1;
        } else {
            let answer = pending.answer();
            self.answer_question(answer);
        }
    }

    /// Pasted text goes where the panel is typing: the note being written,
    /// or the "Other" row. True when it went somewhere.
    pub(crate) fn paste_into_question(&mut self, text: &str) -> bool {
        let Some(pending) = self.question.as_mut() else {
            return false;
        };
        let clean: String = text
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .filter(|c| !c.is_control())
            .collect();
        let current = pending.current;
        if let Some((option, _)) = pending.editing {
            pending.notes[current][option].push_str(&clean);
        } else if pending.on_other() {
            pending.other[current].push_str(&clean);
        }
        true
    }

    /// Send the answers to the agent waiting on them.
    pub(crate) fn answer_question(&mut self, answer: Answer) {
        let Some(pending) = self.question.take() else {
            return;
        };
        self.agent.answer(&pending.id, answer.clone());
        self.last_answer = Some(answer);
        self.question_rows.clear();
        self.set_activity(Activity::Waiting);
    }
}
