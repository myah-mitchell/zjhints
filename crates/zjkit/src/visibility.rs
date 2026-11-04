//! Hiding a bar on request, keeping every bar of an alias in step, and giving
//! a bar's row back when it has nothing to draw.
//!
//! A bar is hidden by a `<plugin>::hide` or `<plugin>::toggle` pipe message,
//! or by its `hidden` option, and shown again by `<plugin>::show` or another
//! toggle. A message's payload may name an alias, and then only bars started
//! from that alias take it. A bar that starts while the others are hidden
//! takes their state from a bar of the same alias in the same instance (see
//! `Visibility::adopt` and `shared`), unless the user has already hidden or
//! shown it.
//!
//! Everything here is pure logic; collapsing the pane is a host call the
//! plugin makes as it draws, and `shared` makes for it as it is hidden or
//! shown.

use crate::settings::live;
use std::collections::BTreeMap;
use zellij_tile::prelude::PipeSource;

/// The option that hides the bar.
pub const OPTION: &str = "hidden";

/// The pipe messages of one plugin's bars. Build them with
/// `visibility_names!`, so every plugin follows the same scheme and a bar
/// never takes another plugin's messages.
#[derive(Debug, PartialEq)]
pub struct Names {
    pub hide: &'static str,
    pub show: &'static str,
    pub toggle: &'static str,
}

/// The visibility message names of the plugin named `$plugin`.
#[macro_export]
macro_rules! visibility_names {
    ($plugin:literal) => {
        $crate::visibility::Names {
            hide: concat!($plugin, "::hide"),
            show: concat!($plugin, "::show"),
            toggle: concat!($plugin, "::toggle"),
        }
    };
}

/// What a hide, show or toggle message asks of the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    Hide,
    Show,
    Toggle,
}

impl Request {
    /// The request a pipe message named `name` makes, if it is one of these.
    pub fn from_message(names: &Names, name: &str) -> Option<Self> {
        if name == names.hide {
            Some(Self::Hide)
        } else if name == names.show {
            Some(Self::Show)
        } else if name == names.toggle {
            Some(Self::Toggle)
        } else {
            None
        }
    }
}

/// Whether a configuration sets the `hidden` option.
pub fn parse_hidden(configuration: &BTreeMap<String, String>) -> bool {
    configuration
        .get(OPTION)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
}

/// Whether a bar is hidden, and what it has told Zellij about its row.
#[derive(Debug, Default)]
pub struct Visibility {
    /// Hidden by a message or by the `hidden` option: the bar draws nothing
    /// and gives its row back.
    hidden: bool,
    /// Whether the state is the bar's own to keep: the user has hidden or
    /// shown it, or it has taken another bar's already.
    settled: bool,
    /// The last `zellij pipe` command a hide or show message came from. See
    /// `ends_a_cli_stream`.
    last_cli_pipe: Option<String>,
    /// What Zellij was last told about the pane's row, so the collapse is
    /// only sent when it changes rather than on every render. Starts out
    /// matching how Zellij places the pane, which is expanded.
    collapsed: bool,
}

impl Visibility {
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Follow the `hidden` option from `previous` to `configuration`. The
    /// option sets the state when it changes, on load or in a preview, and
    /// leaves a state a message set alone otherwise.
    pub fn configure(
        &mut self,
        configuration: &BTreeMap<String, String>,
        previous: &BTreeMap<String, String>,
    ) {
        let hidden = parse_hidden(configuration);
        if hidden != parse_hidden(previous) {
            self.hidden = hidden;
        }
    }

    /// Hide or show the bar, when `alias` (a message's payload) is empty or
    /// names the alias this bar was started from (`own_url`). A bar that does
    /// not know its alias yet takes every message, as it does previews.
    /// Returns whether anything changed.
    pub fn change(&mut self, request: Request, alias: Option<&str>, own_url: Option<&str>) -> bool {
        let alias = alias.map(str::trim).filter(|alias| !alias.is_empty());
        if alias.is_some_and(|alias| !live::is_for(alias, own_url)) {
            return false;
        }
        // What the user asks for outranks another bar's state.
        self.settled = true;
        let hidden = match request {
            Request::Hide => true,
            Request::Show => false,
            Request::Toggle => !self.hidden,
        };
        let changed = hidden != self.hidden;
        self.hidden = hidden;
        changed
    }

    /// Take `hidden` from another bar started from the same alias, as a bar
    /// in a new tab joins the others, unless the user has already hidden or
    /// shown this one. Returns whether anything changed.
    pub fn adopt(&mut self, hidden: bool) -> bool {
        if self.settled {
            return false;
        }
        self.settled = true;
        let changed = hidden != self.hidden;
        self.hidden = hidden;
        changed
    }

    /// Whether `source` and `payload` are the end of a `zellij pipe` stream
    /// already seen. With stdin not a terminal, as in a script, the command
    /// sends its payload, then a message per line of stdin, then one with no
    /// payload when stdin ends. Taken on its own, that last one would reach
    /// every bar rather than the alias the payload named.
    pub fn ends_a_cli_stream(&mut self, source: &PipeSource, payload: Option<&str>) -> bool {
        let PipeSource::Cli(pipe_id) = source else {
            return false;
        };
        let seen = self.last_cli_pipe.as_deref() == Some(pipe_id.as_str());
        self.last_cli_pipe = Some(pipe_id.clone());
        seen && payload.is_none()
    }

    /// What this render has to say about the pane's row, or `None` when it
    /// says the same thing the last one did and there is no point repeating
    /// it. Records the answer as the new last word.
    ///
    /// Hiding the bar always frees its row; `collapse_when_empty` is about a
    /// bar left with nothing to draw by itself.
    pub fn next_collapsed(
        &mut self,
        collapse_when_empty: bool,
        output_is_empty: bool,
    ) -> Option<bool> {
        let should_collapse = (collapse_when_empty || self.hidden) && output_is_empty;
        if self.collapsed == should_collapse {
            return None;
        }
        self.collapsed = should_collapse;
        Some(should_collapse)
    }

    /// What to tell Zellij about the row as the bar is hidden or shown away
    /// from a render, which Zellij may not call again while the pane is
    /// collapsed or on a tab nobody looks at: a hidden bar draws nothing
    /// and gives its row up, and a bar shown takes it back until it next
    /// draws. `None` when Zellij was last told so already. Records the
    /// answer as the new last word.
    pub fn row_after_hiding(&mut self) -> Option<bool> {
        if self.collapsed == self.hidden {
            return None;
        }
        self.collapsed = self.hidden;
        Some(self.hidden)
    }

    /// Zellij kept the row after all, as a prompt was up in the pane (see
    /// `shared`): the next render says again whether to collapse it.
    pub fn row_kept(&mut self) {
        self.collapsed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static NAMES: Names = crate::visibility_names!("zjtest");

    #[test]
    fn hide_show_and_toggle_messages_are_recognized() {
        let from = |name| Request::from_message(&NAMES, name);
        assert_eq!(from("zjtest::hide"), Some(Request::Hide));
        assert_eq!(from("zjtest::show"), Some(Request::Show));
        assert_eq!(from("zjtest::toggle"), Some(Request::Toggle));
        assert_eq!(from("zjtest"), None);
        // Another plugin's bars are left alone.
        assert_eq!(from("zjhints::hide"), None);
    }

    #[test]
    fn the_hidden_option_is_read_loosely() {
        let config = |value: &str| BTreeMap::from([(OPTION.to_string(), value.to_string())]);
        assert!(parse_hidden(&config(" TRUE ")));
        assert!(!parse_hidden(&config("yes")));
        assert!(!parse_hidden(&BTreeMap::new()));
    }

    #[test]
    fn the_row_is_only_reported_when_the_answer_changes() {
        let mut visibility = Visibility::default();
        assert_eq!(visibility.next_collapsed(true, false), None);
        assert_eq!(visibility.next_collapsed(true, true), Some(true));
        assert_eq!(visibility.next_collapsed(true, true), None);
        // With the option off an empty bar keeps its row.
        assert_eq!(visibility.next_collapsed(false, true), Some(false));
    }

    #[test]
    fn a_bar_takes_another_bar_s_state_once_and_never_over_the_user_s() {
        let mut visibility = Visibility::default();
        assert!(visibility.adopt(true));
        assert!(visibility.is_hidden());
        assert!(!visibility.adopt(false));
        assert!(visibility.is_hidden());

        let mut shown = Visibility::default();
        shown.change(Request::Show, None, None);
        assert!(!shown.adopt(true));
        assert!(!shown.is_hidden());
    }
}
