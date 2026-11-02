use super::SCHEMA;
use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zjkit::settings::editor::*;
use zjkit::settings::schema::Kind;
use zjkit::settings::view::*;

fn alias(configuration: &[(&str, &str)]) -> Vec<PluginAliasEntry> {
    vec![PluginAliasEntry {
        name: "zjhints".to_string(),
        plugin: PluginEntry {
            location: "file:~/zjhints.wasm".to_string(),
            cwd: None,
            configuration: configuration
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        },
    }]
}

fn editor(configuration: &[(&str, &str)]) -> Editor {
    Editor::new(
        &SCHEMA,
        "file:~/zjhints.wasm",
        alias(configuration),
        alias(configuration),
        None,
    )
    .unwrap()
}

/// `row` as drawn on its own, laying the theme for it alone.
fn row_view(editor: &Editor, row: &Row, view: FormatView) -> RowView {
    Rows::new(editor, view, None).view(row)
}

fn option_row(name: &str) -> Row {
    Row::Option(SCHEMA.find(name).unwrap())
}

fn key_row(key: &str) -> Row {
    Row::Key(key.to_string())
}

/// The text of the field of `row`, formats shown raw.
fn raw(editor: &Editor, row: &Row) -> String {
    match row_view(editor, row, FormatView::Raw).control {
        Control::Text { text, .. } => text,
        other => panic!("expected a text field, got {other:?}"),
    }
}

fn text(text: &str, placeholder: bool) -> Control {
    Control::Text {
        text: text.to_string(),
        placeholder,
    }
}

#[test]
fn each_kind_of_option_gets_its_own_widget() {
    let editor = editor(&[("show_mode", "true"), ("direction_keys", "arrows")]);
    let view = |name| row_view(&editor, &option_row(name), FormatView::Raw);
    assert_eq!(view("show_mode").control, Control::Toggle(true));
    assert_eq!(view("auto_width").control, Control::Toggle(true));
    assert_eq!(
        view("direction_keys").control,
        Control::Dropdown(
            vec![
                "both".to_string(),
                "arrows".to_string(),
                "letters".to_string()
            ],
            1
        )
    );
    assert_eq!(view("max_length").control, text("0", true));
    assert_eq!(view("max_length").label, "max_length");
}

#[test]
fn markers_say_whether_a_value_is_unsaved_default_or_set() {
    let mut editor = editor(&[("max_length", "40")]);
    let marker =
        |editor: &Editor, name| row_view(editor, &option_row(name), FormatView::Raw).marker;
    assert_eq!(marker(&editor, "max_length"), Marker::None);
    assert_eq!(marker(&editor, "auto_width"), Marker::Default);
    editor.submit(
        &Field::Value {
            key: "max_length".to_string(),
        },
        "60",
    );
    assert_eq!(marker(&editor, "max_length"), Marker::Unsaved);
    // Unsetting a saved value is unsaved too.
    editor.submit(
        &Field::Value {
            key: "max_length".to_string(),
        },
        "",
    );
    assert_eq!(marker(&editor, "max_length"), Marker::Unsaved);
    assert_eq!(Marker::Unsaved.text(), "● unsaved");
    assert_eq!(Marker::Default.text(), "○ default");
}

#[test]
fn an_unset_field_shows_its_default_as_a_placeholder() {
    let editor = editor(&[("max_length", "40")]);
    assert_eq!(
        row_view(&editor, &option_row("overflow_str"), FormatView::Raw).control,
        text("\"...\"", true)
    );
    assert_eq!(
        row_view(&editor, &option_row("max_length"), FormatView::Raw).control,
        text("40", false)
    );
}

#[test]
fn text_is_always_quoted_and_nothing_else_is() {
    let editor = editor(&[
        ("pipe_name", "bar"),
        ("label_quit", "bye"),
        ("dim_strength", "0.25"),
        ("key_format", "{key}"),
    ]);
    assert_eq!(raw(&editor, &option_row("pipe_name")), "\"bar\"");
    assert_eq!(raw(&editor, &key_row("label_quit")), "\"bye\"");
    assert_eq!(raw(&editor, &option_row("key_format")), "\"{key}\"");
    assert_eq!(raw(&editor, &option_row("dim_strength")), "0.25");
    assert_eq!(show_value("", Kind::Integer), "\"\"");
    assert_eq!(show_value(" 1", Kind::Integer), "\" 1\"");
}

#[test]
fn control_characters_in_keys_and_values_show_as_escapes() {
    let osc = "\u{1b}]52;c;aGk=\u{7}";
    let editor = editor(&[
        ("label_quit", osc),
        ("key_format", osc),
        ("max_length", "\u{1b}[2J"),
        ("x\u{1b}[1A", "1"),
    ]);
    let shown = "\"\\u{1b}]52;c;aGk=\\u{7}\"";
    for view in [FormatView::Raw, FormatView::Rendered] {
        let row = |row: &Row| row_view(&editor, row, view);
        assert_eq!(row(&key_row("label_quit")).control, text(shown, false));
        let format = row(&option_row("key_format"));
        assert_eq!(format.control, text(shown, false));
        assert_eq!(format.styled, None);
        assert_eq!(
            row(&option_row("max_length")).control,
            text("\\u{1b}[2J", false)
        );
        assert_eq!(row(&key_row("x\u{1b}[1A")).label, "x\\u{1b}[1A");
    }
}

#[test]
fn add_rows_are_buttons_naming_the_families_they_add() {
    let editor = editor(&[]);
    let button = |prefix| row_view(&editor, &Row::Add(prefix), FormatView::Raw).control;
    assert_eq!(
        button("color_"),
        Control::Button("+ add color_<name>".to_string())
    );
    assert_eq!(
        button("label_"),
        Control::Button("+ add label_<id> or label_<mode>_<id>".to_string())
    );
    assert_eq!(button(""), Control::Button("+ add a key".to_string()));
    assert_eq!(
        row_view(&editor, &Row::Heading("Colors"), FormatView::Raw).control,
        Control::Heading("Colors".to_string())
    );
}

#[test]
fn the_alias_row_is_a_dropdown_of_the_aliases() {
    let editor = editor(&[]);
    assert_eq!(
        row_view(&editor, &Row::Alias, FormatView::Raw).control,
        Control::Dropdown(vec!["zjhints".to_string()], 0)
    );
}

#[test]
fn formats_are_rendered_unless_raw_is_asked_for() {
    let editor = editor(&[
        ("key_format", "#[fg=#ff0000,bold] {key} "),
        ("desc_format", "[{desc}]"),
        ("mode_format", "<{mode}>"),
        ("mode_format_enter_search", "{mode}"),
        ("hint_spacer", "#[fg=red]|"),
        ("label_quit", "{key}"),
    ]);
    let rendered = |row: &Row| row_view(&editor, row, FormatView::Rendered);

    let view = rendered(&option_row("key_format"));
    assert_eq!(view.control, text(" Ctrl p ", false));
    let ansi = view.styled.unwrap();
    assert!(ansi.contains("38;2;255;0;0"));
    assert!(ansi.contains(" Ctrl p "));

    let shown = |row: &Row| match rendered(row).control {
        Control::Text { text, .. } => text,
        other => panic!("{other:?}"),
    };
    assert_eq!(shown(&option_row("desc_format")), "[Pane]");
    assert_eq!(shown(&option_row("mode_format")), "<NORMAL>");
    assert_eq!(shown(&key_row("mode_format_enter_search")), "ENTERSEARCH");
    assert_eq!(shown(&option_row("hint_spacer")), "|");
    // Labels are text, not format strings, whatever they hold.
    assert_eq!(shown(&key_row("label_quit")), "\"{key}\"");

    let view = row_view(&editor, &option_row("key_format"), FormatView::Raw);
    assert_eq!(view.control, text("\"#[fg=#ff0000,bold] {key} \"", false));
    assert_eq!(view.styled, None);
}

#[test]
fn a_rendered_key_uses_the_modifier_aliases_being_edited() {
    let editor = editor(&[("mod_alias_ctrl", "^"), ("key_format", "{key}")]);
    let view = row_view(&editor, &option_row("key_format"), FormatView::Rendered);
    assert_eq!(view.control, text("^p", false));
}

#[test]
fn a_format_that_draws_nothing_visible_shows_raw() {
    let editor = editor(&[("hint_spacer", "  "), ("desc_format", "#[fg=red]")]);
    let view = |name| row_view(&editor, &option_row(name), FormatView::Rendered);
    assert_eq!(view("hint_spacer").control, text("\"  \"", false));
    assert_eq!(view("desc_format").control, text("\"#[fg=red]\"", false));
    assert_eq!(view("desc_format").styled, None);
}

#[test]
fn a_rendered_value_is_cut_by_visible_characters() {
    let ansi = "\u{1b}[31mabcdefghij\u{1b}[0m";
    let cut = take_visible(ansi, 5);
    assert!(cut.contains("abcde") && !cut.contains('f'));
    assert!(cut.ends_with("\u{1b}[0m"));
}

#[test]
fn wide_characters_are_cut_by_their_columns() {
    assert_eq!(truncate("漢字漢字", 8), "漢字漢字");
    assert_eq!(truncate("漢字漢字", 7), "漢字漢…");
    assert_eq!(truncate("漢字漢字", 6), "漢字…");
    assert_eq!(truncate("abc", 0), "");
    assert_eq!(columns("漢字"), 4);
}

#[test]
fn hex_colors_and_aliases_for_them_get_a_swatch() {
    let config: BTreeMap<String, String> = [
        ("color_blue", "#0000ff"),
        ("color_red", "red"),
        ("color_short", "#0f0"),
    ]
    .iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect();
    assert_eq!(swatch_color("#ff0000", &config), Some((255, 0, 0)));
    assert_eq!(swatch_color("#F00", &config), Some((255, 0, 0)));
    assert_eq!(swatch_color(" #00ff00 ", &config), Some((0, 255, 0)));
    assert_eq!(swatch_color("$blue", &config), Some((0, 0, 255)));
    assert_eq!(swatch_color("$short", &config), Some((0, 255, 0)));
    assert_eq!(swatch_color("$red", &config), None);
    assert_eq!(swatch_color("$missing", &config), None);
    assert_eq!(swatch_color("red", &config), None);
    assert_eq!(swatch_color("#12", &config), None);
    assert_eq!(swatch_color("#1234", &config), None);
    assert_eq!(swatch_color("#gggggg", &config), None);
    assert_eq!(swatch_color("#[fg=#ff0000]", &config), None);
    assert_eq!(
        swatch_ansi((1, 2, 3)),
        "\u{1b}[38;2;1;2;3m██\u{1b}[0m".to_string()
    );

    let editor = editor(&[("color_blue", "#0000ff"), ("color_accent", "$blue")]);
    let swatch = |key| row_view(&editor, &key_row(key), FormatView::Raw).swatch;
    assert_eq!(swatch("color_blue"), Some((0, 0, 255)));
    assert_eq!(swatch("color_accent"), Some((0, 0, 255)));
}

#[test]
fn values_are_drawn_against_the_selected_theme() {
    let editor = editor(&[
        ("color_theme", "tokyo-night"),
        ("key_format_quit", "#[fg=$bg,bg=$red,bold] {key} "),
        ("color_accent", "$red"),
    ]);
    let themed = crate::theme::layer(editor.working(), None);
    let red = swatch_color(&themed["color_red"], &themed).expect("a hex red");

    // A `$red` value the theme defines gets its swatch.
    let view = row_view(&editor, &key_row("color_accent"), FormatView::Raw);
    assert_eq!(view.swatch, Some(red));

    // A format using the theme's aliases renders in its colors.
    let view = row_view(&editor, &key_row("key_format_quit"), FormatView::Rendered);
    let (red_r, red_g, red_b) = red;
    let ansi = view.styled.expect("a rendered format");
    assert!(ansi.contains(&format!("48;2;{red_r};{red_g};{red_b}")));

    // What is saved stays the user's own.
    assert!(!editor.working().contains_key("color_red"));
}

#[test]
fn values_follow_the_zellij_theme_when_no_color_theme_is_chosen() {
    let editor = editor(&[("key_format", "#[bg=$accent] {key} ")]);
    let mut styling = Styling::default();
    styling.ribbon_selected.background = PaletteColor::Rgb((1, 2, 3));
    let row = option_row("key_format");
    let view = Rows::new(&editor, FormatView::Rendered, Some(&styling)).view(&row);
    let ansi = view.styled.expect("a rendered format");
    assert!(ansi.contains("48;2;1;2;3"), "{ansi:?}");
    // Without the palette there is no `$accent` to draw.
    let view = row_view(&editor, &row, FormatView::Rendered);
    let ansi = view.styled.expect("a rendered format");
    assert!(!ansi.contains("48;2;1;2;3"));
}

#[test]
fn an_unset_option_a_theme_sets_shows_the_themes_value() {
    let editor = editor(&[("color_theme", "nord"), ("format_theme", "flat")]);
    let themed = crate::theme::layer(editor.working(), None);
    let view = row_view(&editor, &option_row("key_format"), FormatView::Raw);
    let shown = show_value(&themed["key_format"], Kind::Format);
    assert_eq!(view.control, text(&shown, true));
    assert_eq!(view.marker, Marker::Theme);

    let view = row_view(&editor, &option_row("key_format"), FormatView::Rendered);
    assert_eq!(view.control, text(" Ctrl p ", true));
    assert!(view.styled.is_some());

    // Without a theme the built-in default shows, as before.
    let view = row_view(&editor, &option_row("max_length"), FormatView::Raw);
    assert_eq!(view.marker, Marker::Default);
}

#[test]
fn fields_are_as_wide_as_the_longest_label_and_value_need() {
    let editor = editor(&[]);
    let rows = Rows::new(&editor, FormatView::Raw, None);
    let views: Vec<RowView> = editor
        .rows_of(Section::Group("Fitting"))
        .iter()
        .map(|row| rows.view(row))
        .collect();
    // "hint_precedence" is the longest label in Fitting.
    let (label, value) = field_widths(&views, 80);
    assert_eq!(label, "hint_precedence".len() + 2);
    assert_eq!(value, TEXT_FIELD_WIDTH);

    // A narrow list leaves room for the markers, down to a minimum.
    let (label_narrow, value_narrow) = field_widths(&views, 40);
    assert_eq!(label_narrow, label);
    assert_eq!(value_narrow, 40 - label - "○ default".chars().count() - 2);
    assert_eq!(field_widths(&views, 10).1, MIN_VALUE_WIDTH);

    // A long key is cut at the label limit.
    let long = RowView::plain("x".repeat(60), text("", false));
    assert_eq!(field_widths(&[long], 120).0, MAX_LABEL_WIDTH);
}

#[test]
fn the_swatch_comes_before_the_marker_which_stays_inside_the_list() {
    let mut view = RowView::plain("color_blue", text("\"#0000ff\"", false));
    view.marker = Marker::Unsaved;
    assert_eq!(trailer_columns(&view, 30, 80), (32, 32));
    view.swatch = Some((0, 0, 255));
    assert_eq!(trailer_columns(&view, 30, 80), (32, 36));
    // Too little room pulls the marker back over the gap.
    assert_eq!(trailer_columns(&view, 30, 40), (32, 31));
}

#[test]
fn the_screen_is_centered_with_the_menu_beside_the_rows() {
    let items: Vec<String> = editor(&[])
        .sections()
        .iter()
        .map(|s| s.title().to_string())
        .collect();
    // "Keys and ordering" is the longest item.
    let layout = Layout::new(30, 200, &items, None).unwrap();
    assert_eq!(layout.menu_width, 17 + 4);
    assert_eq!(layout.width, 21 + 2 + 80);
    assert_eq!(layout.x, (200 - 103) / 2);
    assert_eq!(layout.content_x, layout.x + 23);
    assert_eq!(layout.content_width, 80);
    assert_eq!((layout.body_y, layout.body_height), (4, 22));
    assert_eq!(layout.footer_y, 26);

    let layout = Layout::new(30, 60, &items, None).unwrap();
    assert_eq!((layout.x, layout.menu_width), (0, 20));
    assert_eq!(layout.content_width, 38);

    assert_eq!(Layout::new(MIN_ROWS - 1, 80, &items, None), None);
    assert_eq!(Layout::new(30, MIN_COLS - 1, &items, None), None);
}

#[test]
fn a_previewed_list_has_the_preview_right_under_its_rows() {
    let items = vec!["Theme".to_string()];
    let layout = Layout::new(30, 120, &items, Some(3)).unwrap();
    assert_eq!(layout.list_height, 3);
    assert_eq!(layout.preview_y, Some(layout.body_y + 4));

    // A long list leaves room for the preview at the bottom.
    let layout = Layout::new(30, 120, &items, Some(100)).unwrap();
    assert_eq!(
        layout.list_height,
        30 - HEADER_ROWS - FOOTER_ROWS - PREVIEW_ROWS
    );
}

#[test]
fn the_menu_marks_sections_with_unsaved_edits() {
    let mut editor = editor(&[]);
    editor.submit(
        &Field::Value {
            key: "color_blue".to_string(),
        },
        "#0000ff",
    );
    let items = menu_items(&editor);
    assert!(items.contains(&"Colors ●".to_string()));
    assert!(items.contains(&"Fitting".to_string()));
}

#[test]
fn the_header_counts_the_unsaved_changes_and_names_the_buttons() {
    assert_eq!(unsaved_text(0), "0 unsaved changes");
    assert_eq!(unsaved_text(1), "1 unsaved change");
    let buttons = header_buttons(FormatView::Rendered);
    assert_eq!(buttons[0], ("<Ctrl a>", "save", Command::Save));
    assert_eq!(buttons[2].1, "raw formats");
    assert_eq!(header_buttons(FormatView::Raw)[2].1, "rendered formats");
    assert_eq!(overrides_button(true).1, "hide overrides");
}

#[test]
fn the_help_line_lists_the_keys_for_what_has_focus() {
    let browsing = |focus, text_row, searching| {
        help_hints(HelpState::Browsing {
            focus,
            text_row,
            searching,
        })
    };
    let list = browsing(Focus::List, false, false);
    assert!(list.contains(&("<Space>", "change")));
    assert!(list.contains(&("<r>", "revert")));
    assert!(list.contains(&("<Esc>", "close")));
    assert!(browsing(Focus::List, true, false).contains(&("<Enter>", "edit")));
    assert!(browsing(Focus::List, false, true).contains(&("<Esc>", "end search")));
    assert!(browsing(Focus::Sidebar, false, false).contains(&("<Tab>", "settings")));
    assert_eq!(
        browsing(Focus::Search, false, true),
        [("<↓>", "results"), ("<Esc>", "end search")]
    );
    assert_eq!(help_hints(HelpState::Editing)[0], ("<Enter>", "apply"));
}

#[test]
fn every_help_button_presses_the_key_it_names() {
    let states = [Focus::Sidebar, Focus::List, Focus::Search]
        .into_iter()
        .flat_map(|focus| {
            [false, true].into_iter().flat_map(move |text_row| {
                [false, true].map(|searching| HelpState::Browsing {
                    focus,
                    text_row,
                    searching,
                })
            })
        })
        .chain([HelpState::Editing, HelpState::Choosing]);
    for state in states {
        for (key, _) in help_hints(state) {
            assert!(hint_press(key).is_some(), "{key} presses nothing");
        }
    }
    assert_eq!(hint_press("<Del>"), Some(BareKey::Delete));
    assert_eq!(hint_press("<↓↑>"), Some(BareKey::Down));
}

#[test]
fn the_description_explains_the_selected_row() {
    let mut editor = editor(&[("color_blue", "#0000ff")]);
    editor.submit(&Field::NewKey, "key_order");
    let text = describe(&editor).unwrap();
    assert!(text.starts_with("How keys within a hint are ordered"));
    assert!(text.ends_with("(default: qwerty)"));
    editor.submit(&Field::NewKey, "color_blue");
    assert!(describe(&editor).unwrap().contains("color_<name>"));
    editor.focus = Focus::Sidebar;
    assert_eq!(describe(&editor), None);
    assert_eq!(match_text(1), "1 match");
    assert_eq!(match_text(0), "0 matches");
}

fn mode_info(mode: InputMode) -> ModeInfo {
    ModeInfo {
        mode,
        base_mode: Some(InputMode::Normal),
        keybinds: vec![(
            InputMode::Normal,
            vec![(KeyWithModifier::new(BareKey::Char('q')), vec![Action::Quit])],
        )],
        ..Default::default()
    }
}

fn config_of(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn overrides_are_the_keys_a_theme_sets_that_the_user_sets_too() {
    let configuration = config_of(&[
        ("color_theme", "nord"),
        ("format_theme", "flat"),
        ("color_accent", "#ebcb8b"),
        ("desc_format", "{desc}"),
        ("color_mine", "#123456"),
        ("max_length", "80"),
    ]);
    assert_eq!(
        theme_overrides(&SCHEMA, &configuration, None),
        vec!["color_accent", "desc_format"]
    );

    // With every theme left at zellij and no palette, nothing is set to
    // override.
    let plain = config_of(&[("key_format", "{key}"), ("color_accent", "#ff0000")]);
    assert!(theme_overrides(&SCHEMA, &plain, None).is_empty());
}

#[test]
fn the_preview_draws_the_bar_with_or_without_the_overrides() {
    let configuration = config_of(&[
        ("format_theme", "plain"),
        ("key_format", "<{key}>"),
        ("hidden", "true"),
    ]);
    let mode_info = mode_info(InputMode::Normal);
    let with = theme_preview(&SCHEMA, &configuration, &mode_info, true, 80);
    let without = theme_preview(&SCHEMA, &configuration, &mode_info, false, 80);
    // Drawn although `hidden` is set.
    assert!(with.contains("<q>"), "{with:?}");
    assert!(!without.contains("<q>"), "{without:?}");
    assert!(without.contains('q'), "{without:?}");
}

#[test]
fn the_caption_says_whether_the_overrides_are_drawn() {
    assert_eq!(preview_caption(0, true), "Preview · no overrides");
    assert_eq!(preview_caption(1, true), "Preview · with your 1 override");
    assert_eq!(
        preview_caption(3, false),
        "Preview · without your 3 overrides"
    );
    assert!(is_previewed(&SCHEMA, Section::Group("Theme")));
    assert!(!is_previewed(&SCHEMA, Section::Group("Colors")));
}
