//! Messages typed while a turn runs. They wait in a queue shown above the
//! composer and go one at a time as each turn finishes; Ctrl+Enter (or the
//! "send now" button) stops the running turn and sends at once.

use super::*;

/// A message waiting to be sent, with the images attached to it.
#[derive(Debug, Clone)]
pub(crate) struct Queued {
    pub text: String,
    pub attachments: Vec<enowx_core::message::Attachment>,
}

impl App {
    /// Enter while a turn runs: the composer's message joins the queue.
    pub(crate) fn enqueue(&mut self, text: String) {
        self.queued.push_back(Queued {
            text,
            attachments: std::mem::take(&mut self.attachments),
        });
        self.input.clear();
        self.cursor = 0;
        self.status = format!(
            "queued ({}) · sent when this turn ends · Ctrl+Enter sends now",
            self.queued.len()
        );
    }

    /// Send the first queued message now, as a new turn.
    fn send_next_queued(&mut self) {
        let Some(next) = self.queued.pop_front() else {
            return;
        };
        self.queue_paused = false;
        // The images go with the message they were attached to; any the
        // composer holds now stay there, for the message being written.
        let mut held = std::mem::replace(&mut self.attachments, next.attachments);
        self.start_turn(next.text);
        self.attachments.append(&mut held);
    }

    /// Ctrl+Enter while a turn runs: what is typed goes now, ahead of the
    /// queue; with nothing typed, the first queued message does.
    pub(crate) fn send_now(&mut self) {
        let text = self.input.trim().to_owned();
        if text.is_empty() {
            self.send_queued_now();
            return;
        }
        if text.starts_with('/') {
            return;
        }
        self.input.clear();
        self.cursor = 0;
        if self.busy {
            self.interrupt();
        }
        self.start_turn(text);
    }

    /// The button, or Ctrl+Enter with nothing typed: stop the running turn,
    /// if any, and send the first queued message at once.
    pub(crate) fn send_queued_now(&mut self) {
        if self.queued.is_empty() {
            self.status = "nothing queued".into();
            return;
        }
        if self.busy {
            self.interrupt();
        }
        self.send_next_queued();
    }

    /// Each frame: when nothing runs and no question waits, the next queued
    /// message goes, unless the user stopped the last turn.
    pub(crate) fn tick_queue(&mut self) {
        if self.queued.is_empty() || self.queue_paused || self.busy || self.question.is_some() {
            return;
        }
        self.send_next_queued();
    }

    /// Up in an empty composer: the last queued message comes back to be
    /// edited, or deleted by clearing it.
    pub(crate) fn unqueue_last(&mut self) -> bool {
        if !self.input.is_empty() {
            return false;
        }
        let Some(last) = self.queued.pop_back() else {
            return false;
        };
        self.input = last.text;
        self.cursor = self.input.len();
        self.attachments.extend(last.attachments);
        self.status = format!(
            "editing a queued message · {} still queued",
            self.queued.len()
        );
        true
    }
}
