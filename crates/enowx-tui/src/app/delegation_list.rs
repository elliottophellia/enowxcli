//! The sidebar's list of delegations: at most `DELEGATIONS_SHOWN` at a time,
//! following the newest until the user slides it back to older ones.

use super::*;

/// How many delegations the Agents tab lists at once. A wave of parallel
/// work sends out six or eight; listed whole, they pushed the roster off the
/// card.
pub(crate) const DELEGATIONS_SHOWN: usize = 5;

impl App {
    /// The first delegation on show: the newest `DELEGATIONS_SHOWN` unless
    /// the list was slid back.
    pub(crate) fn delegation_start(&self) -> usize {
        let last = self.delegations.len().saturating_sub(DELEGATIONS_SHOWN);
        self.delegation_window.map_or(last, |start| start.min(last))
    }

    /// Slide the list by `step` delegations, older when negative. Back at the
    /// newest, it follows new ones again.
    pub(crate) fn slide_delegations(&mut self, step: isize) {
        let last = self.delegations.len().saturating_sub(DELEGATIONS_SHOWN);
        let start = (self.delegation_start() as isize + step).clamp(0, last as isize) as usize;
        self.delegation_window = (start < last).then_some(start);
    }
}
