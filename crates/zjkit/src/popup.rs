//! The popup of a hover preview: the plugin running as a list of the entries
//! an item's menu would offer, opened by the bar below the item and drawn
//! like that menu. Clicking an entry has the bar choose it, as it would in
//! the menu. The popup tells the bar when it opens and closes, and closes
//! itself when the pointer leaves it, a click lands on it, or it is never
//! hovered.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use crate::mouse::HOVER_PICKED_ENTRY_ARG;

/// How long a popup waits to be hovered before it closes itself, which is
/// what keeps one from outliving a pointer that left the bar elsewhere.
const HOVER_IDLE_SECONDS: f64 = 3.0;

/// The messages a bar and its popups exchange, and the configuration keys a
/// popup is opened with, all named after the plugin so that two plugins'
/// bars and popups never answer each other. Build it with `popup_names!`.
#[derive(Debug)]
pub struct Names {
    /// Sent by a popup to the bar that opened it, with the popup's tag, once
    /// it can take messages, so the bar learns its plugin id.
    pub opened: &'static str,
    /// Sent by a popup to its bar, with its tag, as it closes.
    pub closed: &'static str,
    /// Sent by the bar to one of its popups to close it.
    pub close: &'static str,
    /// Sent by a popup to its bar, with its tag, when one of its entries is
    /// clicked. `HOVER_PICKED_ENTRY_ARG` holds the entry's index.
    pub picked: &'static str,
    /// What marks an instance as a popup rather than the bar, and what it
    /// shows. Named apart from the options, so a user option that reads like
    /// a switch for the feature cannot turn a bar into a popup.
    pub preview_key: &'static str,
    pub tag_key: &'static str,
    pub entries_key: &'static str,
}

impl Names {
    /// The configuration keys only a popup is opened with, which a settings
    /// screen refuses to add.
    pub const fn internal_options(&self) -> [&'static str; 3] {
        [self.preview_key, self.tag_key, self.entries_key]
    }
}

/// The `Names` of the plugin called `$plugin`: messages `<plugin>::hover_*`
/// and configuration keys `<plugin>_internal_preview*`.
#[macro_export]
macro_rules! popup_names {
    ($plugin:literal) => {
        $crate::popup::Names {
            opened: concat!($plugin, "::hover_opened"),
            closed: concat!($plugin, "::hover_closed"),
            close: concat!($plugin, "::hover_close"),
            picked: concat!($plugin, "::hover_picked"),
            preview_key: concat!($plugin, "_internal_preview"),
            tag_key: concat!($plugin, "_internal_preview_tag"),
            entries_key: concat!($plugin, "_internal_preview_entries"),
        }
    };
}

/// The configuration a popup named `tag` is opened with.
pub fn configuration(names: &Names, tag: &str, entries: &[String]) -> BTreeMap<String, String> {
    BTreeMap::from([
        (names.preview_key.to_string(), "true".to_string()),
        (names.tag_key.to_string(), tag.to_string()),
        (names.entries_key.to_string(), encode_entries(entries)),
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

/// The list widget the menu a click opens is drawn with, holding `entries`.
/// Entries are plain text, and whoever opened the popup chose them, so a
/// control character in one is shown, not obeyed.
fn menu_list(entries: &[String]) -> MenuList {
    MenuList::new(
        entries
            .iter()
            .map(|entry| MenuItem::new(crate::ansi::show_controls(entry)))
            .collect(),
    )
    .focused()
}

/// The columns and rows a popup of `entries` needs: the list as wide as the
/// menu a click opens makes it, inside a frame.
pub fn size(entries: &[String]) -> (usize, usize) {
    let list = menu_list(entries);
    (
        list.natural_width() + LIST_PADDING + FRAME_COLUMNS,
        list.height_for(MENU_ROWS) + FRAME_ROWS,
    )
}

/// The frame of a popup of `rows` by `cols`, rounded like the click menu's,
/// or nothing when there is no room for one.
pub fn frame_lines(rows: usize, cols: usize) -> Vec<String> {
    if rows <= FRAME_ROWS || cols <= FRAME_COLUMNS {
        return vec![];
    }
    let border = "─".repeat(cols - 2);
    let side = format!("│{}│", " ".repeat(cols - 2));
    let mut lines = vec![format!("╭{border}╮")];
    lines.extend(std::iter::repeat_n(side, rows - FRAME_ROWS));
    lines.push(format!("╰{border}╯"));
    lines
}

/// Where the list sits inside the frame, with a column of padding each side.
const LIST_COLUMN: usize = 2;
const LIST_ROW: usize = 1;
/// A border and a column of padding on each side, a border above and below.
const FRAME_COLUMNS: usize = 4;
const FRAME_ROWS: usize = 2;
/// What the click menu adds to its list's own width.
const LIST_PADDING: usize = 2;
/// The rows of entries shown at most, the rest a scroll away, as in the
/// click menu.
const MENU_ROWS: usize = 12;

#[derive(Debug, PartialEq)]
enum PopupAction {
    Stay,
    Redraw,
    Close,
    /// Have the bar choose this entry, then close.
    Pick(usize),
}

#[derive(Debug)]
pub struct PreviewPopup {
    names: &'static Names,
    tag: String,
    /// The plugin id of the bar that opened it.
    owner: Option<u32>,
    list: MenuList,
    /// Whether the pointer has been over the popup.
    hovered: bool,
}

impl PreviewPopup {
    /// The popup `configuration` asks for, if it is a popup's.
    pub fn from_configuration(
        names: &'static Names,
        configuration: &BTreeMap<String, String>,
    ) -> Option<Self> {
        if configuration.get(names.preview_key).map(String::as_str) != Some("true") {
            return None;
        }
        let tag = configuration
            .get(names.tag_key)
            .cloned()
            .unwrap_or_default();
        let entries = configuration
            .get(names.entries_key)
            .map(|encoded| decode_entries(encoded))
            .unwrap_or_default();
        Some(Self {
            names,
            owner: crate::mouse::parse_preview_tag(&tag).map(|(owner, _)| owner),
            tag,
            list: menu_list(&entries),
            hovered: false,
        })
    }

    /// Subscribes to what the popup needs and asks for `permissions`, which
    /// must be the bar's whole set (see below).
    pub fn load(&self, permissions: &[PermissionType]) {
        subscribe(&[
            EventType::Mouse,
            EventType::Timer,
            EventType::PermissionRequestResult,
            EventType::BeforeClose,
        ]);
        // Messaging the bar needs a permission. The popup shares the bar's
        // location, and so its cached grants, which a request replaces: the
        // bar's whole set is granted from the cache, without a prompt.
        request_permission(permissions);
        set_timeout(HOVER_IDLE_SECONDS);
    }

    pub fn update(&mut self, event: Event) -> bool {
        let action = match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.tell_owner(self.names.opened, BTreeMap::new());
                PopupAction::Stay
            }
            // However the popup closes, so the bar can show it again.
            Event::BeforeClose => {
                self.tell_owner(self.names.closed, BTreeMap::new());
                PopupAction::Stay
            }
            Event::Mouse(mouse) => self.on_mouse(mouse),
            Event::Timer(_) => self.on_timer(),
            _ => PopupAction::Stay,
        };
        match action {
            PopupAction::Stay => {}
            PopupAction::Redraw => return true,
            PopupAction::Close => close_self(),
            PopupAction::Pick(index) => {
                let args =
                    BTreeMap::from([(HOVER_PICKED_ENTRY_ARG.to_string(), index.to_string())]);
                self.tell_owner(self.names.picked, args);
                close_self();
            }
        }
        false
    }

    pub fn pipe(&self, message: &PipeMessage) {
        if message.name == self.names.close && self.is_from_owner(&message.source) {
            close_self();
        }
    }

    pub fn render(&mut self, rows: usize, cols: usize) {
        for (row, line) in frame_lines(rows, cols).into_iter().enumerate() {
            print_text_with_coordinates(Text::from(line), 0, row, Some(cols), None);
        }
        print!("{}", self.lay_out(rows, cols));
    }

    /// The list drawn inside the frame of a popup of `rows` by `cols`, which
    /// also tells it where it is for the clicks that follow.
    fn lay_out(&mut self, rows: usize, cols: usize) -> String {
        if rows <= FRAME_ROWS || cols <= FRAME_COLUMNS {
            return String::new();
        }
        self.list.serialize(
            LIST_COLUMN,
            LIST_ROW,
            cols - FRAME_COLUMNS,
            rows - FRAME_ROWS,
        )
    }

    fn tell_owner(&self, name: &str, args: BTreeMap<String, String>) {
        if let Some(owner) = self.owner {
            pipe_message_to_plugin(
                MessageToPlugin::new(name)
                    .with_destination_plugin_id(owner)
                    .with_payload(self.tag.clone())
                    .with_args(args),
            );
        }
    }

    /// The pointer leaving the popup closes it. A left-click on an entry
    /// picks it, one on a scroll arrow scrolls, and any other click closes
    /// the popup. The list highlights the entry under the pointer and
    /// scrolls with the wheel, as the click menu does.
    fn on_mouse(&mut self, mouse: Mouse) -> PopupAction {
        match mouse {
            Mouse::Hover(line, _) if line < 0 => PopupAction::Close,
            Mouse::LeftClick(..) => match self.list.handle_mouse(mouse) {
                UiResponse::Submitted(UiValue::Choice { index, .. }) => PopupAction::Pick(index),
                UiResponse::NotHandled => PopupAction::Close,
                _ => PopupAction::Redraw,
            },
            Mouse::RightClick(..) => PopupAction::Close,
            _ => {
                if matches!(mouse, Mouse::Hover(..)) {
                    self.hovered = true;
                }
                if self.list.handle_mouse(mouse).is_handled() {
                    PopupAction::Redraw
                } else {
                    PopupAction::Stay
                }
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    static NAMES: Names = crate::popup_names!("zjtest");

    fn entries(labels: &[&str]) -> Vec<String> {
        labels.iter().map(|label| label.to_string()).collect()
    }

    fn popup(labels: &[&str]) -> PreviewPopup {
        PreviewPopup::from_configuration(&NAMES, &configuration(&NAMES, "7-2", &entries(labels)))
            .expect("a popup")
    }

    /// `popup`, laid out at the size its entries ask for.
    fn drawn(labels: &[&str]) -> PreviewPopup {
        let mut popup = popup(labels);
        let (cols, rows) = size(&entries(labels));
        popup.lay_out(rows, cols);
        popup
    }

    fn labels(popup: &PreviewPopup) -> Vec<&str> {
        popup.list.items().iter().map(MenuItem::label).collect()
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
        let popup =
            PreviewPopup::from_configuration(&NAMES, &configuration(&NAMES, "7-2", &original))
                .expect("a popup");
        // The newline arrives, and the popup shows it escaped.
        let mut shown = original.clone();
        shown[3] = "two\\nlines".to_string();
        assert_eq!(labels(&popup), shown);
        assert_eq!(popup.tag, "7-2");
        assert_eq!(popup.owner, Some(7));
    }

    #[test]
    fn names_are_the_plugins_own() {
        assert_eq!(NAMES.opened, "zjtest::hover_opened");
        assert_eq!(NAMES.closed, "zjtest::hover_closed");
        assert_eq!(NAMES.close, "zjtest::hover_close");
        assert_eq!(NAMES.picked, "zjtest::hover_picked");
        assert_eq!(
            NAMES.internal_options(),
            [
                "zjtest_internal_preview",
                "zjtest_internal_preview_tag",
                "zjtest_internal_preview_entries",
            ]
        );
        // Another plugin's popup configuration makes no popup here.
        static OTHER: Names = crate::popup_names!("zjother");
        let theirs = configuration(&OTHER, "7-2", &entries(&["a"]));
        assert!(PreviewPopup::from_configuration(&NAMES, &theirs).is_none());
    }

    #[test]
    fn only_a_popup_configuration_makes_a_popup() {
        assert!(PreviewPopup::from_configuration(&NAMES, &BTreeMap::new()).is_none());
        let bar = BTreeMap::from([("mouse".to_string(), "true".to_string())]);
        assert!(PreviewPopup::from_configuration(&NAMES, &bar).is_none());
        let off = BTreeMap::from([(NAMES.preview_key.to_string(), "false".to_string())]);
        assert!(PreviewPopup::from_configuration(&NAMES, &off).is_none());
        // An option a user might set to turn the feature on leaves a bar.
        let user = BTreeMap::from([("hover_preview".to_string(), "true".to_string())]);
        assert!(PreviewPopup::from_configuration(&NAMES, &user).is_none());
    }

    #[test]
    fn the_pointer_leaving_the_popup_closes_it() {
        let mut popup = drawn(&["a"]);
        assert_eq!(popup.on_mouse(Mouse::Hover(1, 3)), PopupAction::Redraw);
        assert_eq!(popup.on_mouse(Mouse::Hover(-1, 0)), PopupAction::Close);
    }

    #[test]
    fn clicking_an_entry_picks_it() {
        let mut popup = drawn(&["a", "b"]);
        assert_eq!(popup.on_mouse(Mouse::LeftClick(1, 3)), PopupAction::Pick(0));
        assert_eq!(popup.on_mouse(Mouse::LeftClick(2, 3)), PopupAction::Pick(1));
    }

    #[test]
    fn any_other_click_closes_the_popup() {
        let mut framed = drawn(&["a", "b"]);
        // The frame above and below the entries, and beside them.
        assert_eq!(framed.on_mouse(Mouse::LeftClick(0, 3)), PopupAction::Close);
        assert_eq!(framed.on_mouse(Mouse::LeftClick(3, 3)), PopupAction::Close);
        assert_eq!(framed.on_mouse(Mouse::LeftClick(1, 0)), PopupAction::Close);
        assert_eq!(framed.on_mouse(Mouse::RightClick(1, 3)), PopupAction::Close);
        // Not drawn yet, so nothing is on any line.
        let mut fresh = popup(&["a"]);
        assert_eq!(fresh.on_mouse(Mouse::LeftClick(1, 3)), PopupAction::Close);
    }

    #[test]
    fn the_entry_under_the_pointer_is_highlighted() {
        let mut popup = drawn(&["a", "b"]);
        assert_eq!(popup.on_mouse(Mouse::Hover(2, 3)), PopupAction::Redraw);
        assert_eq!(popup.list.highlighted_index(), Some(1));
        // The frame is no entry, and leaves the highlight where it was.
        assert_eq!(popup.on_mouse(Mouse::Hover(0, 3)), PopupAction::Redraw);
        assert_eq!(popup.list.highlighted_index(), Some(1));
    }

    #[test]
    fn a_long_list_scrolls_to_entries_that_did_not_fit() {
        let many: Vec<String> = (0..20).map(|index| format!("entry {index}")).collect();
        let labels: Vec<&str> = many.iter().map(String::as_str).collect();
        let (_, rows) = size(&many);
        assert_eq!(rows, MENU_ROWS + FRAME_ROWS);
        let mut popup = drawn(&labels);
        // The arrow under the last entry shown scrolls a page down.
        let arrow = (rows - FRAME_ROWS) as isize;
        assert_eq!(
            popup.on_mouse(Mouse::LeftClick(arrow, 3)),
            PopupAction::Redraw
        );
        assert!(popup.list.scroll_offset() > 0);
        let first = popup.list.scroll_offset();
        assert_eq!(
            popup.on_mouse(Mouse::LeftClick(2, 3)),
            PopupAction::Pick(first)
        );
    }

    #[test]
    fn a_popup_nobody_hovered_closes_when_its_time_is_up() {
        let mut popup = popup(&["a"]);
        assert_eq!(popup.on_timer(), PopupAction::Close);
        popup.on_mouse(Mouse::Hover(0, 0));
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
        let stray = PreviewPopup::from_configuration(
            &NAMES,
            &configuration(&NAMES, "stray", &entries(&["a"])),
        )
        .expect("a popup");
        assert!(!stray.is_from_owner(&PipeSource::Plugin(7)));
    }

    #[test]
    fn a_popup_is_as_wide_as_the_click_menu_makes_its_list() {
        let labels = entries(&["h←  move focus left", "j↓  down"]);
        let widest = "h←  move focus left".chars().count();
        // A space, the mark column and a space before a label and one
        // after, the menu's padding, then the frame.
        assert_eq!(size(&labels), (widest + 5 + 2 + 4, 2 + 2));
        // A wide character takes two columns.
        assert_eq!(size(&entries(&["漢字"])), (4 + 5 + 2 + 4, 1 + 2));
    }

    #[test]
    fn a_popup_frame_is_rounded() {
        assert_eq!(
            frame_lines(4, 6),
            vec!["╭────╮", "│    │", "│    │", "╰────╯"]
        );
    }

    #[test]
    fn a_popup_with_no_room_for_a_frame_draws_nothing() {
        assert!(frame_lines(2, 20).is_empty());
        assert!(frame_lines(5, 4).is_empty());
        assert!(frame_lines(0, 0).is_empty());
        assert!(popup(&["a"]).lay_out(2, 20).is_empty());
    }

    #[test]
    fn a_popup_shows_control_characters_instead_of_sending_them() {
        let popup = popup(&["\u{1b}]52;c;aGk=\u{7}", "a\u{1b}[2Jb"]);
        assert_eq!(
            labels(&popup),
            vec!["\\u{1b}]52;c;aGk=\\u{7}", "a\\u{1b}[2Jb"]
        );
    }
}
