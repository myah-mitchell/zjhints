//! Configurations no layout should hold, for tests that load a plugin with
//! them and check that it never panics.
//!
//! Zellij lets anyone who can launch a plugin add a pane to a running
//! shared instance with whatever options they like, and a panic in any of
//! the instance's slots takes down every bar it runs. So a plugin has to
//! load, take its first events and draw with any options at all. `Junk`
//! makes such options: the plugin's own keys and its internal ones with
//! values that are empty, huge, negative, non-ASCII, very long or full of
//! escapes, and keys that are none of those.
//!
//! It is a fixed pseudo-random sequence from a seed, so a failing map is
//! the same on every run and can be printed and replayed.

use crate::bar::BarSpec;
use crate::host::{ALIAS_KEY, INSTANCE_KEY, SETTINGS_KEY};
use crate::shared::{Shared, SlotPlugin};
use std::collections::{BTreeMap, HashMap};
use std::panic::{self, AssertUnwindSafe};
use zellij_tile::prelude::*;

/// Pieces values are made of, each a likely edge for some parser.
const PIECES: &[&str] = &[
    "",
    " ",
    "true",
    "false",
    "TRUE",
    "yes",
    "0",
    "1",
    "-1",
    "01",
    "+7",
    " 7 ",
    "1.5",
    "-0.0",
    "1e309",
    "NaN",
    "inf",
    "-inf",
    "255",
    "256",
    "65535",
    "65536",
    "4294967295",
    "4294967296",
    "9223372036854775807",
    "-9223372036854775808",
    "18446744073709551615",
    "18446744073709551616",
    "340282366920938463463374607431768211456",
    "é",
    "日本語",
    "ｗｉｄｅ",
    "🦀",
    "👩\u{200d}👩\u{200d}👧",
    "e\u{301}",
    "\u{202e}",
    "\u{fe0f}",
    "\u{ad}",
    "\0",
    "\t",
    "\n",
    "\r\n",
    "\x1b",
    "\x1b[31m",
    "\x1b]8;;x\x1b\\",
    "\x7f",
    "\\",
    "\\n",
    "\\\\",
    "\"",
    "'",
    "{",
    "}",
    "{}",
    "{mode}",
    "{{",
    "%",
    "%s",
    "#",
    "#fff",
    "#ffffff",
    "#gggggg",
    "#12345",
    "rgb(1,2,3)",
    "red",
    "bold",
    ",",
    ";",
    "=",
    ":",
    "|",
    "<",
    ">",
    "<n>",
    "/",
    "..",
    "zellij:",
    "file:",
    "/dev/null",
];

/// The `<...>` placeholders of a family's keys are filled with these.
const KEY_FILLS: &[&str] = &[
    "0",
    "1",
    "2",
    "07",
    "-1",
    "99999999999999999999",
    "x",
    "é",
    "",
];

/// Keys no plugin reads, or reads only by accident.
const ODD_KEYS: &[&str] = &[
    "",
    " ",
    "_",
    "é",
    "🦀",
    "\0",
    "\n",
    "key with spaces",
    "UPPER",
    "-",
    "launch_",
    "color_",
    "label_",
];

/// A fixed pseudo-random source of configurations. See the module's docs.
pub struct Junk {
    state: u64,
}

impl Junk {
    pub fn new(seed: u64) -> Self {
        Junk { state: seed }
    }

    /// The next number of the sequence (SplitMix64).
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `n`, which is more than 0.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[self.below(from.len())]
    }

    /// A value: one to three pieces, now and then repeated to a length
    /// past any width a bar is drawn at, and in places cut short of the
    /// end so a multi-byte character or an escape is left whole but alone.
    pub fn value(&mut self) -> String {
        let mut value = String::new();
        for _ in 0..=self.below(3) {
            value.push_str(self.pick(PIECES));
        }
        match self.below(8) {
            0 => value = value.repeat(1 + self.below(4000)),
            1 => {
                let piece = self.pick(&["é", "日", "🦀", "e\u{301}", "\x1b[1m", "a"]);
                value.push_str(&piece.repeat(self.below(300)));
            }
            _ => {}
        }
        value
    }

    /// A configuration with some of `known`, a plugin's own keys, and now
    /// and then an odd key or a known one changed a little, each with a
    /// value from `value`.
    pub fn configuration(&mut self, known: &[String]) -> BTreeMap<String, String> {
        let mut configuration = BTreeMap::new();
        for _ in 0..self.below(known.len() + 1) {
            let key = match self.below(10) {
                0 => self.pick(ODD_KEYS).to_string(),
                1 => {
                    let key = &known[self.below(known.len())];
                    format!("{key}{}", self.pick(&["_", "x", "é", " ", "0"]))
                }
                _ => known[self.below(known.len())].clone(),
            };
            let value = self.value();
            configuration.insert(key, value);
        }
        configuration
    }

    /// The keys a bar of `spec` reads, with each family's placeholders
    /// filled in a few ways, the settings screen's and popups' own keys,
    /// and the keys zjkit reads for every plugin.
    pub fn known_keys(&mut self, spec: &BarSpec) -> Vec<String> {
        let schema = spec.schema;
        let mut keys: Vec<String> = schema
            .options
            .iter()
            .map(|option| option.name)
            .chain(schema.screen_options.iter().copied())
            .chain(schema.internal_options.iter().copied())
            .chain(spec.popup.iter().flat_map(|names| names.internal_options()))
            .chain([SETTINGS_KEY, ALIAS_KEY, INSTANCE_KEY])
            .map(str::to_string)
            .collect();
        for family in schema.families {
            for _ in 0..4 {
                keys.push(self.fill(family.pattern));
            }
        }
        keys
    }

    /// `pattern` with each `<...>` placeholder replaced.
    fn fill(&mut self, pattern: &str) -> String {
        let mut key = String::new();
        let mut rest = pattern;
        while let Some(start) = rest.find('<') {
            key.push_str(&rest[..start]);
            match rest[start..].find('>') {
                Some(end) => {
                    key.push_str(self.pick(KEY_FILLS));
                    rest = &rest[start + end + 1..];
                }
                None => {
                    rest = &rest[start..];
                    break;
                }
            }
        }
        key.push_str(rest);
        key
    }
}

/// Run `count` instances of `P` as Zellij would, each with slots loaded
/// with configurations from `Junk::new(seed)`: two bars' panes and a
/// background slot, which take a client's first events and a grant, are
/// drawn at a few sizes, clicked and closed. `fresh` starts whatever the
/// plugin keeps for an instance again before each.
///
/// Every other round, the first configuration is a popup's or a settings
/// screen's, so their own keys get junk too. Those two call into Zellij for
/// what it answers as soon as they take an event, which a test cannot
/// answer, so they are only loaded and drawn, each in an instance of its
/// own.
///
/// Panics with the configurations that made `P` panic.
pub fn shake<P: SlotPlugin>(spec: &BarSpec, seed: u64, count: usize, mut fresh: impl FnMut()) {
    let mut junk = Junk::new(seed);
    for round in 0..count {
        let known = junk.known_keys(spec);
        let mut configurations: Vec<_> = (0..3).map(|_| junk.configuration(&known)).collect();
        let flag = match (round % 4, spec.popup) {
            (1, Some(names)) => Some(names.preview_key),
            (3, _) => Some(SETTINGS_KEY),
            _ => None,
        };
        if let Some(flag) = flag {
            configurations[0].insert(flag.to_string(), "true".to_string());
        }
        fresh();
        let run = panic::catch_unwind(AssertUnwindSafe(|| {
            let (others, bars): (Vec<_>, Vec<_>) = configurations
                .iter()
                .cloned()
                .partition(|configuration| is_not_bar(spec, configuration));
            for configuration in others {
                draw_only::<P>(configuration);
            }
            if !bars.is_empty() {
                run::<P>(&bars);
            }
        }));
        if run.is_err() {
            panic!("seed {seed}, round {round}: {configurations:#?}");
        }
    }
}

/// Whether `configuration` loads a popup or a settings screen.
fn is_not_bar(spec: &BarSpec, configuration: &BTreeMap<String, String>) -> bool {
    let popup = spec.popup.is_some_and(|names| {
        configuration.get(names.preview_key).map(String::as_str) == Some("true")
    });
    popup || configuration.get(SETTINGS_KEY).map(String::as_str) == Some("true")
}

/// A popup or a settings screen, loaded with `configuration` and drawn.
fn draw_only<P: SlotPlugin>(configuration: BTreeMap<String, String>) {
    let mut shared = Shared::<P>::default();
    shared.add_slot(Slot {
        id: 10,
        kind: SlotKind::Pane,
        configuration,
    });
    for (rows, cols) in [(1, 80), (0, 0), (1, 1), (3, 7), (50, 150)] {
        shared.render_slot(rows, cols, 10, 1);
    }
}

/// One instance of `shake`, with a slot for each of `configurations`, the
/// last in the background.
fn run<P: SlotPlugin>(configurations: &[BTreeMap<String, String>]) {
    let mut shared = Shared::<P>::default();
    let ids: Vec<u32> = (0..configurations.len() as u32).map(|n| 10 + n).collect();
    let panes: Vec<PaneInfo> = ids
        .iter()
        .map(|id| PaneInfo {
            id: *id,
            is_plugin: true,
            is_selectable: true,
            pane_content_columns: 80,
            pane_content_rows: 1,
            plugin_url: Some("file:/junk.wasm".to_string()),
            ..Default::default()
        })
        .chain([PaneInfo {
            id: 1,
            title: "vim".to_string(),
            is_focused: true,
            is_selectable: true,
            is_floating: true,
            ..Default::default()
        }])
        .collect();
    let context = |slot_id| EventContext {
        slot_id,
        client_id: Some(1),
    };
    let last = ids.len() - 1;
    for (n, (id, configuration)) in ids.iter().zip(configurations).enumerate() {
        let kind = if n == last {
            SlotKind::Background
        } else {
            SlotKind::Pane
        };
        shared.add_slot(Slot {
            id: *id,
            kind,
            configuration: configuration.clone(),
        });
    }
    let tab = TabInfo {
        active: true,
        name: "Tab #1".to_string(),
        ..Default::default()
    };
    shared.take_event(Event::ModeUpdate(ModeInfo::default()), context(None));
    shared.take_event(Event::TabUpdate(vec![tab]), context(None));
    let manifest = PaneManifest {
        panes: HashMap::from([(0, panes)]),
    };
    shared.take_event(Event::PaneUpdate(manifest), context(None));
    let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
    shared.take_event(granted, context(Some(ids[0])));
    for id in &ids[..last] {
        shared.take_event(Event::Visible(true), context(Some(*id)));
        for (rows, cols) in [(1, 80), (1, 0), (1, 1), (3, 7), (50, 150)] {
            shared.render_slot(rows, cols, *id, 1);
        }
        let click = Event::Mouse(Mouse::LeftClick(0, 3));
        shared.take_event(click, context(Some(*id)));
        let hover = Event::Mouse(Mouse::Hover(0, 5));
        shared.take_event(hover, context(Some(*id)));
        shared.render_slot(1, 80, *id, 1);
    }
    for id in ids {
        shared.remove_slot(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_configurations() {
        let known = vec!["max_length".to_string(), "mouse".to_string()];
        let mut one = Junk::new(7);
        let mut two = Junk::new(7);
        for _ in 0..50 {
            assert_eq!(one.configuration(&known), two.configuration(&known));
        }
    }

    #[test]
    fn a_family_s_placeholders_are_filled() {
        let mut junk = Junk::new(1);
        for _ in 0..50 {
            let key = junk.fill("launch_<n>_name");
            assert!(
                key.starts_with("launch_") && key.ends_with("_name"),
                "{key}"
            );
            assert!(!key.contains('<'), "{key}");
        }
        assert_eq!(junk.fill("open<"), "open<");
    }
}
