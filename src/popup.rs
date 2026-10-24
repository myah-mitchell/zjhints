//! The popup of a hover preview: this plugin running as a read-only list of
//! the entries a hint's menu would offer, opened by the bar below the hint.
//! It tells the bar when it opens and closes, and closes itself when the
//! pointer leaves it, a click lands on it, or it is never hovered.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use crate::mouse::{HOVER_CLOSED_MESSAGE, HOVER_CLOSE_MESSAGE, HOVER_OPENED_MESSAGE};

/// How long a popup waits to be hovered before it closes itself, which is
/// what keeps one from outliving a pointer that left the bar elsewhere.
const HOVER_IDLE_SECONDS: f64 = 3.0;

// What marks an instance as a preview popup rather than the bar, and what it
// shows. Named apart from the options, so a user option that reads like a
// switch for the feature cannot turn a bar into a popup.
const CONFIG_HOVER_PREVIEW: &str = "zjhints_internal_preview";
const CONFIG_HOVER_PREVIEW_TAG: &str = "zjhints_internal_preview_tag";
const CONFIG_HOVER_PREVIEW_ENTRIES: &str = "zjhints_internal_preview_entries";

/// The configuration keys only a popup is opened with, which the settings
/// screen refuses to add.
pub const INTERNAL_OPTIONS: &[&str] = &[
    CONFIG_HOVER_PREVIEW,
    CONFIG_HOVER_PREVIEW_TAG,
    CONFIG_HOVER_PREVIEW_ENTRIES,
];

/// The configuration a popup named `tag` is opened with.
pub fn configuration(
    tag: &str,
    entries: &[String],
    wide_ambiguous: bool,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (CONFIG_HOVER_PREVIEW.to_string(), "true".to_string()),
        (CONFIG_HOVER_PREVIEW_TAG.to_string(), tag.to_string()),
        (
            CONFIG_HOVER_PREVIEW_ENTRIES.to_string(),
            encode_entries(entries),
        ),
        (
            crate::CONFIG_AMBIGUOUS_WIDTH.to_string(),
            if wide_ambiguous { "2" } else { "1" }.to_string(),
        ),
    ])
}

/// Entries as one configuration value: each followed by a newline, with
/// backslashes and newlines inside an entry escaped.
fn encode_entries(entries: &[String]) -> String {
    entries
        .iter()
        .map(|entry| {
            let escaped = entry.replace('\\', "\\\\").replace('\n', "\\n");
            format!("{escaped}\n")
        })
        .collect()
}

fn decode_entries(encoded: &str) -> Vec<String> {
    encoded
        .split_terminator('\n')
        .map(|escaped| {
            let mut entry = String::new();
            let mut chars = escaped.chars();
            while let Some(ch) = chars.next() {
                if ch != '\\' {
                    entry.push(ch);
                    continue;
                }
                match chars.next() {
                    Some('n') => entry.push('\n'),
                    Some(other) => entry.push(other),
                    None => entry.push('\\'),
                }
            }
            entry
        })
        .collect()
}

/// The columns and rows a popup of `entries` needs: the entries inside a
/// frame with a space of padding on either side.
pub fn size(entries: &[String], wide_ambiguous: bool) -> (usize, usize) {
    let widest = entries
        .iter()
        .map(|entry| crate::calculate_visible_length(&crate::show_controls(entry), wide_ambiguous))
        .max()
        .unwrap_or(0);
    (widest + FRAME_COLUMNS, entries.len() + FRAME_ROWS)
}

/// A border and a space on each side, a border above and below.
const FRAME_COLUMNS: usize = 4;
const FRAME_ROWS: usize = 2;

#[derive(Debug, PartialEq)]
enum PopupAction {
    Stay,
    Close,
}

#[derive(Debug)]
pub struct PreviewPopup {
    tag: String,
    /// The plugin id of the bar that opened it.
    owner: Option<u32>,
    entries: Vec<String>,
    wide_ambiguous: bool,
    /// Whether the pointer has been over the popup.
    hovered: bool,
}

impl PreviewPopup {
    /// The popup `configuration` asks for, if it is a popup's.
    pub fn from_configuration(configuration: &BTreeMap<String, String>) -> Option<Self> {
        if configuration.get(CONFIG_HOVER_PREVIEW).map(String::as_str) != Some("true") {
            return None;
        }
        let tag = configuration
            .get(CONFIG_HOVER_PREVIEW_TAG)
            .cloned()
            .unwrap_or_default();
        Some(Self {
            owner: crate::mouse::parse_preview_tag(&tag).map(|(owner, _)| owner),
            tag,
            // Entries are plain text, and whoever opened the popup chose
            // them, so a control character in one is shown, not obeyed.
            entries: configuration
                .get(CONFIG_HOVER_PREVIEW_ENTRIES)
                .map(|encoded| {
                    decode_entries(encoded)
                        .iter()
                        .map(|entry| crate::show_controls(entry).into_owned())
                        .collect()
                })
                .unwrap_or_default(),
            wide_ambiguous: crate::parse_wide_ambiguous(configuration),
            hovered: false,
        })
    }

    pub fn load(&self) {
        subscribe(&[
            EventType::Mouse,
            EventType::Timer,
            EventType::PermissionRequestResult,
            EventType::BeforeClose,
        ]);
        // Messaging the bar needs a permission. The popup shares the bar's
        // location, and so its cached grants, which a request replaces: the
        // bar's whole set is granted from the cache, without a prompt.
        request_permission(&crate::bar_permissions(true));
        set_timeout(HOVER_IDLE_SECONDS);
    }

    pub fn update(&mut self, event: Event) -> bool {
        let action = match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.tell_owner(HOVER_OPENED_MESSAGE);
                PopupAction::Stay
            }
            // However the popup closes, so the bar can show it again.
            Event::BeforeClose => {
                self.tell_owner(HOVER_CLOSED_MESSAGE);
                PopupAction::Stay
            }
            Event::Mouse(mouse) => self.on_mouse(&mouse),
            Event::Timer(_) => self.on_timer(),
            _ => PopupAction::Stay,
        };
        if action == PopupAction::Close {
            close_self();
        }
        false
    }

    pub fn pipe(&self, message: &PipeMessage) {
        if message.name == HOVER_CLOSE_MESSAGE && self.is_from_owner(&message.source) {
            close_self();
        }
    }

    pub fn render(&self, rows: usize, cols: usize) {
        for (row, line) in self.lines(rows, cols).into_iter().enumerate() {
            print_text_with_coordinates(Text::from(&line), 0, row, None, None);
        }
    }

    fn tell_owner(&self, name: &str) {
        if let Some(owner) = self.owner {
            pipe_message_to_plugin(
                MessageToPlugin::new(name)
                    .with_destination_plugin_id(owner)
                    .with_payload(self.tag.clone()),
            );
        }
    }

    /// The pointer leaving the popup closes it, and so does a click, which
    /// has nothing to choose here.
    fn on_mouse(&mut self, mouse: &Mouse) -> PopupAction {
        match mouse {
            Mouse::Hover(line, _) if *line < 0 => PopupAction::Close,
            Mouse::Hover(..) => {
                self.hovered = true;
                PopupAction::Stay
            }
            Mouse::LeftClick(..) | Mouse::RightClick(..) => PopupAction::Close,
            _ => PopupAction::Stay,
        }
    }

    /// A popup nobody hovered by the time `HOVER_IDLE_SECONDS` are up has
    /// been left behind. One the pointer is over closes when it leaves.
    fn on_timer(&self) -> PopupAction {
        if self.hovered {
            PopupAction::Stay
        } else {
            PopupAction::Close
        }
    }

    /// Any plugin or `zellij pipe` can send a close message, so only the
    /// bar's own counts.
    fn is_from_owner(&self, source: &PipeSource) -> bool {
        matches!(
            (source, self.owner),
            (PipeSource::Plugin(sender), Some(owner)) if *sender == owner
        )
    }

    /// The popup's rows for a pane of `rows` by `cols`: the entries in a
    /// frame, or as many as fit with `…` standing for the rest.
    fn lines(&self, rows: usize, cols: usize) -> Vec<String> {
        if rows <= FRAME_ROWS || cols <= FRAME_COLUMNS {
            return vec![];
        }
        let inner = cols - FRAME_COLUMNS;
        let room = rows - FRAME_ROWS;
        let cut = self.entries.len() > room;
        let shown = if cut { room - 1 } else { self.entries.len() };
        let border = "─".repeat(cols - 2);
        let mut lines = vec![format!("┌{border}┐")];
        for entry in self.entries.iter().take(shown) {
            lines.push(self.framed(entry, inner));
        }
        if cut {
            lines.push(self.framed("…", inner));
        }
        lines.push(format!("└{border}┘"));
        lines
    }

    fn framed(&self, entry: &str, inner: usize) -> String {
        let text = crate::truncate_ansi_string(entry, "…", inner, self.wide_ambiguous);
        let padding =
            inner.saturating_sub(crate::calculate_visible_length(&text, self.wide_ambiguous));
        format!("│ {text}{} │", " ".repeat(padding))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(labels: &[&str]) -> Vec<String> {
        labels.iter().map(|label| label.to_string()).collect()
    }

    fn popup(labels: &[&str]) -> PreviewPopup {
        PreviewPopup::from_configuration(&configuration("7-2", &entries(labels), false))
            .expect("a popup")
    }

    #[test]
    fn entries_survive_being_carried_in_configuration() {
        let original = entries(&[
            "h←  move focus left",
            "",
            "back\\slash",
            "two\nlines",
            "\\n is not a newline",
            "trailing\\",
        ]);
        assert_eq!(decode_entries(&encode_entries(&original)), original);
        assert_eq!(decode_entries(&encode_entries(&[])), Vec::<String>::new());
        assert_eq!(
            decode_entries(&encode_entries(&entries(&[""]))),
            entries(&[""])
        );
        let popup = PreviewPopup::from_configuration(&configuration("7-2", &original, true))
            .expect("a popup");
        // The newline arrives, and the popup shows it escaped.
        let mut shown = original.clone();
        shown[3] = "two\\nlines".to_string();
        assert_eq!(popup.entries, shown);
        assert_eq!(popup.tag, "7-2");
        assert_eq!(popup.owner, Some(7));
        assert!(popup.wide_ambiguous);
        let narrow = PreviewPopup::from_configuration(&configuration("7-2", &original, false))
            .expect("a popup");
        assert!(!narrow.wide_ambiguous);
    }

    #[test]
    fn only_a_popup_configuration_makes_a_popup() {
        assert!(PreviewPopup::from_configuration(&BTreeMap::new()).is_none());
        let bar = BTreeMap::from([("mouse".to_string(), "true".to_string())]);
        assert!(PreviewPopup::from_configuration(&bar).is_none());
        let off = BTreeMap::from([(CONFIG_HOVER_PREVIEW.to_string(), "false".to_string())]);
        assert!(PreviewPopup::from_configuration(&off).is_none());
        // An option a user might set to turn the feature on leaves a bar.
        let user = BTreeMap::from([("hover_preview".to_string(), "true".to_string())]);
        assert!(PreviewPopup::from_configuration(&user).is_none());
    }

    #[test]
    fn the_pointer_leaving_the_popup_closes_it() {
        let mut popup = popup(&["a"]);
        assert_eq!(popup.on_mouse(&Mouse::Hover(1, 2)), PopupAction::Stay);
        assert_eq!(popup.on_mouse(&Mouse::Hover(-1, 0)), PopupAction::Close);
        assert_eq!(popup.on_mouse(&Mouse::ScrollUp(1)), PopupAction::Stay);
    }

    #[test]
    fn clicking_the_popup_closes_it() {
        let mut popup = popup(&["a"]);
        assert_eq!(popup.on_mouse(&Mouse::LeftClick(1, 2)), PopupAction::Close);
        assert_eq!(popup.on_mouse(&Mouse::RightClick(1, 2)), PopupAction::Close);
    }

    #[test]
    fn a_popup_nobody_hovered_closes_when_its_time_is_up() {
        let mut popup = popup(&["a"]);
        assert_eq!(popup.on_timer(), PopupAction::Close);
        popup.on_mouse(&Mouse::Hover(0, 0));
        assert_eq!(popup.on_timer(), PopupAction::Stay);
    }

    #[test]
    fn only_the_bar_that_opened_the_popup_can_close_it() {
        let popup = popup(&["a"]);
        assert!(popup.is_from_owner(&PipeSource::Plugin(7)));
        for source in [
            PipeSource::Plugin(8),
            PipeSource::Cli("pipe".to_string()),
            PipeSource::Keybind,
        ] {
            assert!(!popup.is_from_owner(&source), "{source:?}");
        }
        // A tag that names no bar is closed by no message.
        let stray =
            PreviewPopup::from_configuration(&configuration("stray", &entries(&["a"]), false))
                .expect("a popup");
        assert!(!stray.is_from_owner(&PipeSource::Plugin(7)));
    }

    #[test]
    fn a_popup_is_sized_to_its_entries_and_frame() {
        let labels = entries(&["h←  move focus left", "j↓  down"]);
        let widest = "h←  move focus left".chars().count();
        assert_eq!(size(&labels, false), (widest + 4, 2 + 2));
        assert_eq!(size(&[], false), (4, 2));
        // A wide character takes two columns.
        assert_eq!(size(&entries(&["漢字"]), false), (4 + 4, 1 + 2));
    }

    #[test]
    fn a_popup_draws_its_entries_in_a_frame_of_the_size_asked_for() {
        let popup = popup(&["h  left", "l  right"]);
        let (width, height) = size(&popup.entries, false);
        assert_eq!(
            popup.lines(height, width),
            vec![
                "┌──────────┐",
                "│ h  left  │",
                "│ l  right │",
                "└──────────┘",
            ]
        );
    }

    #[test]
    fn a_popup_too_short_for_its_entries_ends_in_an_ellipsis() {
        let popup = popup(&["one", "two", "three"]);
        assert_eq!(
            popup.lines(4, 9),
            vec!["┌───────┐", "│ one   │", "│ …     │", "└───────┘"]
        );
        // With room for all of them there is no ellipsis.
        assert_eq!(popup.lines(5, 9).len(), 5);
    }

    #[test]
    fn a_popup_too_narrow_for_an_entry_cuts_it_short() {
        let popup = popup(&["a long entry"]);
        let lines = popup.lines(3, 9);
        assert_eq!(lines[0], "┌───────┐");
        // Plain text, so the cut adds no reset that would restyle the frame.
        assert_eq!(lines[1], "│ a lo… │");
        assert_eq!(lines[2], "└───────┘");
    }

    #[test]
    fn a_popup_cuts_wide_characters_without_breaking_its_frame() {
        let popup = popup(&["漢字漢字"]);
        for cols in 7..=11 {
            for line in popup.lines(3, cols) {
                assert_eq!(
                    crate::calculate_visible_length(&line, false),
                    cols,
                    "{line:?} in {cols} columns"
                );
            }
        }
        assert_eq!(popup.lines(3, 10)[1], "│ 漢字…  │");
    }

    #[test]
    fn a_popup_shows_control_characters_instead_of_sending_them() {
        let labels = entries(&["\u{1b}]52;c;aGk=\u{7}", "a\u{1b}[2Jb"]);
        let popup = PreviewPopup::from_configuration(&configuration("7-2", &labels, false))
            .expect("a popup");
        let (width, height) = size(&labels, false);
        let lines = popup.lines(height, width);
        assert_eq!(lines[1], "│ \\u{1b}]52;c;aGk=\\u{7} │");
        assert_eq!(lines[2], "│ a\\u{1b}[2Jb           │");
        assert!(lines.iter().all(|line| !line.contains('\u{1b}')));
        // Cut short, too.
        assert!(popup
            .lines(height, 8)
            .iter()
            .all(|line| !line.contains('\u{1b}')));
    }

    #[test]
    fn a_popup_with_no_room_for_a_frame_draws_nothing() {
        let popup = popup(&["a"]);
        assert!(popup.lines(2, 20).is_empty());
        assert!(popup.lines(5, 4).is_empty());
        assert!(popup.lines(0, 0).is_empty());
    }
}
