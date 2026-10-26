//! Hiding a bar on request, keeping every bar of an alias in step, and giving
//! a bar's row back when it has nothing to draw.
//!
//! A bar is hidden by a `<plugin>::hide` or `<plugin>::toggle` pipe message,
//! or by its `hidden` option, and shown again by `<plugin>::show` or another
//! toggle. A message's payload may name an alias, and then only bars started
//! from that alias take it. A bar that starts while the others are hidden
//! asks them (`<plugin>::ask_visibility`) and takes the first answer
//! (`<plugin>::visibility`), unless the user has already hidden or shown it.
//!
//! Everything here is pure logic; sending the messages and collapsing the
//! pane are host calls the plugin makes.

use crate::settings::live;
use std::collections::BTreeMap;
use zellij_tile::prelude::PipeSource;

/// The option that hides the bar.
pub const OPTION: &str = "hidden";
/// The payloads of an answer.
pub const HIDDEN: &str = "hidden";
pub const SHOWN: &str = "shown";

/// The pipe messages of one plugin's bars. Build them with
/// `visibility_names!`, so every plugin follows the same scheme and a bar
/// never takes another plugin's messages.
#[derive(Debug, PartialEq)]
pub struct Names {
    pub hide: &'static str,
    pub show: &'static str,
    pub toggle: &'static str,
    /// Asked by a bar when it starts, so a bar in a new tab joins the others
    /// in being hidden or shown; the payload is its alias, if it knows it.
    pub ask: &'static str,
    /// The answer, sent to the asking bar alone, with a payload of `HIDDEN`
    /// or `SHOWN`.
    pub answer: &'static str,
}

/// The visibility message names of the plugin named `$plugin`.
#[macro_export]
macro_rules! visibility_names {
    ($plugin:literal) => {
        $crate::visibility::Names {
            hide: concat!($plugin, "::hide"),
            show: concat!($plugin, "::show"),
            toggle: concat!($plugin, "::toggle"),
            ask: concat!($plugin, "::ask_visibility"),
            answer: concat!($plugin, "::visibility"),
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

/// The bar to answer, when `source` is another bar asking with `payload`
/// whether this one is hidden. Bars of other aliases stay out of it, by the
/// same rule as previews (`live::is_for`). `plugin_id` and `own_url` are this
/// bar's own.
pub fn asker(
    source: &PipeSource,
    payload: Option<&str>,
    plugin_id: Option<u32>,
    own_url: Option<&str>,
) -> Option<u32> {
    let PipeSource::Plugin(asker) = *source else {
        return None;
    };
    if Some(asker) == plugin_id {
        return None;
    }
    payload
        .is_none_or(|alias| live::is_for(alias, own_url))
        .then_some(asker)
}

/// Whether a bar is hidden, and what it has told Zellij about its row.
#[derive(Debug, Default)]
pub struct Visibility {
    /// Hidden by a message or by the `hidden` option: the bar draws nothing
    /// and gives its row back.
    hidden: bool,
    /// Whether this bar has asked the running bars if they are hidden.
    asked: bool,
    /// Whether it has its answer, or no longer needs one.
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

    /// The payload that answers another bar asking whether this one is
    /// hidden.
    pub fn answer(&self) -> &'static str {
        if self.hidden {
            HIDDEN
        } else {
            SHOWN
        }
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

    /// Whether it is time to ask the running bars, which is only ever once.
    pub fn should_ask(&mut self) -> bool {
        !std::mem::replace(&mut self.asked, true)
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
        // What the user asks for outranks what another bar said.
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

    /// Take the first answer to the bar's question. Returns whether anything
    /// changed.
    pub fn take_answer(&mut self, source: &PipeSource, payload: Option<&str>) -> bool {
        if !matches!(source, PipeSource::Plugin(_)) || self.settled {
            return false;
        }
        let hidden = match payload {
            Some(HIDDEN) => true,
            Some(SHOWN) => false,
            _ => return false,
        };
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
    fn the_answer_names_the_state() {
        let mut visibility = Visibility::default();
        assert_eq!(visibility.answer(), SHOWN);
        visibility.change(Request::Hide, None, None);
        assert_eq!(visibility.answer(), HIDDEN);
    }

    #[test]
    fn a_bar_asks_only_once() {
        let mut visibility = Visibility::default();
        assert!(visibility.should_ask());
        assert!(!visibility.should_ask());
    }
}
