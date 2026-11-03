//! Fitting a line of items into the width a bar has, by dropping whole items.
//!
//! A bar renders each item (a hint, a tab, a task) on its own, then hands the
//! pieces to `drop_to_fit`, which gives up whole items until the line fits.
//! What it gave up is one contiguous run, so a single drop indicator drawn at
//! the gap can stand for all of it.
//!
//! Which items go first follows the user's pinned order: each item is in the
//! leading group (pinned ahead of the unpinned ones), the middle group (not
//! pinned) or the trailing group (pinned after them). Unpinned items go first,
//! and `Precedence` says which pinned group is held onto longest.
//!
//! `window` is a separate, simpler policy for bars of ordered items such as
//! tabs. Nothing is pinned by group there; one item (the active tab) must stay
//! on screen, and the rest are shown as far outward from it as they fit.

/// Where an item sits relative to the unpinned ones in a pinned order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Pinned ahead of the unpinned items.
    Leading,
    /// Not pinned.
    Middle,
    /// Pinned after the unpinned items.
    Trailing,
}

/// Which pinned group has precedence, meaning it is kept longest, when items
/// must be given up. Named for zjstatus's `format_precedence`, and read the
/// same way: the groups in the order they are held onto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Precedence {
    /// `"tl"`: the trailing group outlives the leading one.
    #[default]
    Trailing,
    /// `"lt"`: the leading group outlives the trailing one.
    Leading,
}

impl Precedence {
    /// The precedence an option value names. Anything but `lt` is the
    /// default, `tl`.
    pub fn from_config(value: &str) -> Self {
        match value.to_lowercase().trim() {
            "lt" => Self::Leading,
            _ => Self::Trailing,
        }
    }
}

/// One rendered item `drop_to_fit` can keep or give up.
pub trait Fittable {
    /// Columns the item takes on screen, not counting the spacer between it
    /// and its neighbors.
    fn width(&self) -> usize;
    /// The item's place in the pinned order.
    fn group(&self) -> Group;
}

/// Drop items until the line fits in `limit` columns, and report where the
/// gap opened.
///
/// Items go in a single contiguous run so one indicator can stand for all of
/// them. Unpinned items are given up first, from the right. When those run
/// out the run keeps growing outward from that gap: first the leading group
/// from its inner edge, then the trailing group from its inner edge, or the
/// other way around when `precedence` keeps the leading group. Dropping from
/// the far ends instead would open a second gap and need a second indicator.
///
/// `pieces` must be in display order with the groups in order: every leading
/// item, then every middle one, then every trailing one. Sorting by the pinned
/// order gives that.
///
/// `spacer_width` is the gap drawn between neighbors, and `indicator_width`
/// the indicator drawn where items were dropped, which takes a spacer of its
/// own once it is on the line.
///
/// Returns the index in `pieces` the indicator belongs at, or `None` if
/// nothing was dropped. A single item wider than the whole limit survives; the
/// caller cuts it instead, with `crate::ansi::cut_line`.
pub fn drop_to_fit<T: Fittable>(
    pieces: &mut Vec<T>,
    spacer_width: usize,
    limit: usize,
    indicator_width: usize,
    precedence: Precedence,
) -> Option<usize> {
    let width = |pieces: &Vec<T>, extra: usize| {
        let items: usize = pieces.iter().map(Fittable::width).sum();
        let count = pieces.len() + usize::from(extra > 0);
        items + extra + spacer_width * count.saturating_sub(1)
    };

    let mut gap = None;
    while pieces.len() > 1 {
        // Once an item has been dropped the indicator is part of the line, so
        // it has to be paid for out of the same budget.
        let extra = gap.map_or(0, |_| indicator_width);
        if width(pieces, extra) <= limit {
            break;
        }
        let Some(index) = next_to_drop(pieces, gap, precedence) else {
            break;
        };
        pieces.remove(index);
        gap = Some(index);
    }
    gap
}

/// The item to give up next, as an index into `pieces`.
///
/// Always the one adjacent to the gap, so what is dropped stays a single run.
/// Unpinned items go first; between the two pinned groups, `precedence` says
/// which is held onto longer.
fn next_to_drop<T: Fittable>(
    pieces: &[T],
    gap: Option<usize>,
    precedence: Precedence,
) -> Option<usize> {
    // Only ever take from the inner edge of a group, the side facing the gap.
    let inner_left = |group: Group| {
        pieces
            .iter()
            .rposition(|piece| piece.group() == group)
            .filter(|index| gap.is_none_or(|gap| *index < gap))
    };
    let inner_right = |group: Group| {
        pieces
            .iter()
            .position(|piece| piece.group() == group)
            .filter(|index| gap.is_none_or(|gap| *index >= gap))
    };

    let leading = || inner_left(Group::Leading);
    let trailing = || inner_right(Group::Trailing);

    inner_left(Group::Middle)
        .or_else(|| inner_right(Group::Middle))
        .or_else(|| match precedence {
            // Keep the trailing group: spend the leading one first.
            Precedence::Trailing => leading().or_else(trailing),
            Precedence::Leading => trailing().or_else(leading),
        })
}

/// The slice of an ordered bar of items that is drawn, and how much was left
/// out on each side. Returned by `window`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    /// Indices of the items to draw, always contiguous. Empty only when there
    /// were no items.
    pub range: std::ops::Range<usize>,
    /// How many items before `range` are hidden. A marker (such as `← +3`)
    /// belongs ahead of the items when this is not zero.
    pub hidden_before: usize,
    /// How many items after `range` are hidden. A marker (such as `+3 →`)
    /// belongs behind the items when this is not zero.
    pub hidden_after: usize,
}

/// Pick the run of items to draw when a bar of ordered items (tabs, say) is
/// too wide, keeping one item, the `pinned` one, on screen.
///
/// If everything fits in `limit` columns, every item is shown and no marker is
/// needed. Otherwise the window starts as just the pinned item and grows one
/// item at a time, alternating between the two sides, so the pinned item stays
/// roughly in the middle. When a side has nothing left, or cannot take its next
/// item, the other side keeps growing until neither can. The window stays
/// contiguous, so one marker per side can stand for everything hidden there.
///
/// Showing the last hidden item on a side removes that side's marker, which
/// can make the line narrower. So when a side's next item does not fit, the
/// whole rest of that side is tried at once, and taken if that fits.
///
/// `item_widths` are the columns each item takes, in display order, not
/// counting the spacer between neighbors. `spacer_width` is that gap. Every
/// marker is drawn with one spacer between it and the items next to it.
///
/// `before_marker_width` and `after_marker_width` give the columns the marker
/// on that side takes for a given hidden count. They are functions rather than
/// numbers because a marker usually prints the count ("+3"), so hiding one more
/// item can make it wider, for example when the count goes from 9 to 10. A
/// marker is charged only on a side that hides something, and always for that
/// side's count as it will be in the result, never for an earlier step.
///
/// The pinned item is always included, even if it alone, with its markers, is
/// wider than `limit`. The caller cuts it, with `crate::ansi::cut_line`, as
/// for `drop_to_fit`.
///
/// An empty `item_widths` gives an empty range and zero hidden counts. A
/// `pinned` index past the end is clamped to the last item.
pub fn window(
    item_widths: &[usize],
    pinned: usize,
    spacer_width: usize,
    limit: usize,
    before_marker_width: impl Fn(usize) -> usize,
    after_marker_width: impl Fn(usize) -> usize,
) -> Window {
    let count = item_widths.len();
    if count == 0 {
        return Window {
            range: 0..0,
            hidden_before: 0,
            hidden_after: 0,
        };
    }
    let pinned = pinned.min(count - 1);

    // Columns drawn for the items start..end plus the marker on each side that
    // hides something, with a spacer between every pair of neighbors.
    let drawn_width = |start: usize, end: usize| {
        let items: usize = item_widths[start..end].iter().sum();
        let mut pieces = end - start;
        let mut width = items;
        if start > 0 {
            width += before_marker_width(start);
            pieces += 1;
        }
        if end < count {
            width += after_marker_width(count - end);
            pieces += 1;
        }
        width + spacer_width * (pieces - 1)
    };

    let mut start = pinned;
    let mut end = pinned + 1;
    let mut grow_before_next = false;

    if drawn_width(0, count) <= limit {
        start = 0;
        end = count;
    } else {
        loop {
            // Where each side's edge would move to: one item further out, or
            // all the way to the end of the bar if only that fits (the marker
            // on that side is then gone).
            let next_start = if start == 0 {
                None
            } else if drawn_width(start - 1, end) <= limit {
                Some(start - 1)
            } else if drawn_width(0, end) <= limit {
                Some(0)
            } else {
                None
            };
            let next_end = if end == count {
                None
            } else if drawn_width(start, end + 1) <= limit {
                Some(end + 1)
            } else if drawn_width(start, count) <= limit {
                Some(count)
            } else {
                None
            };
            let grow_before = match (next_start, next_end) {
                (None, None) => break,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (Some(_), Some(_)) => grow_before_next,
            };
            if let (true, Some(next)) = (grow_before, next_start) {
                start = next;
            } else if let Some(next) = next_end {
                end = next;
            }
            // Give the other side the next turn, even after a side was
            // skipped, so a stuck side does not hog the turns.
            grow_before_next = !grow_before;
        }
    }

    Window {
        range: start..end,
        hidden_before: start,
        hidden_after: count - end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Item {
        name: char,
        width: usize,
        group: Group,
    }

    impl Fittable for Item {
        fn width(&self) -> usize {
            self.width
        }

        fn group(&self) -> Group {
            self.group
        }
    }

    /// One item per character of `groups`, each `width` columns wide and
    /// named `a`, `b`, `c` and so on: `l` leading, `m` middle, `t` trailing.
    fn items(groups: &str, width: usize) -> Vec<Item> {
        groups
            .chars()
            .zip('a'..)
            .map(|(group, name)| Item {
                name,
                width,
                group: match group {
                    'l' => Group::Leading,
                    'm' => Group::Middle,
                    't' => Group::Trailing,
                    other => panic!("unknown group {other}"),
                },
            })
            .collect()
    }

    fn names(pieces: &[Item]) -> String {
        pieces.iter().map(|piece| piece.name).collect()
    }

    #[test]
    fn a_line_that_fits_drops_nothing() {
        let mut pieces = items("lmt", 2);
        // Three items of 2 and two spacers of 1.
        assert_eq!(
            drop_to_fit(&mut pieces, 1, 8, 1, Precedence::Trailing),
            None
        );
        assert_eq!(names(&pieces), "abc");
    }

    #[test]
    fn unpinned_items_go_first_from_the_right() {
        let mut pieces = items("lmmmt", 2);
        let gap = drop_to_fit(&mut pieces, 1, 11, 1, Precedence::Trailing);
        // a, b, the indicator and e: 2 + 2 + 1 + 2 plus three spacers.
        assert_eq!(names(&pieces), "abe");
        assert_eq!(gap, Some(2));
    }

    #[test]
    fn the_indicator_is_paid_for_out_of_the_same_budget() {
        let mut pieces = items("mmm", 2);
        // Dropping one item leaves 2 + 2 + spacer = 5, but the indicator and
        // its spacer need 3 more, which a limit of 7 cannot hold.
        let gap = drop_to_fit(&mut pieces, 1, 7, 2, Precedence::Trailing);
        assert_eq!(names(&pieces), "a");
        assert_eq!(gap, Some(1));
    }

    #[test]
    fn precedence_picks_which_pinned_group_is_spent_first() {
        let mut keep_trailing = items("llmtt", 2);
        drop_to_fit(&mut keep_trailing, 0, 5, 1, Precedence::Trailing);
        assert_eq!(names(&keep_trailing), "de");

        let mut keep_leading = items("llmtt", 2);
        drop_to_fit(&mut keep_leading, 0, 5, 1, Precedence::Leading);
        assert_eq!(names(&keep_leading), "ab");
    }

    #[test]
    fn a_pinned_group_is_spent_from_its_inner_edge() {
        let mut pieces = items("llmmmtt", 1);
        let gap = drop_to_fit(&mut pieces, 0, 4, 1, Precedence::Leading);
        // c, d and e go first; then f, the trailing item next to the gap,
        // rather than g at the far end, so one indicator covers c through f.
        assert_eq!(names(&pieces), "abg");
        assert_eq!(gap, Some(2));
    }

    #[test]
    fn a_single_item_wider_than_the_limit_survives() {
        let mut pieces = items("m", 20);
        assert_eq!(
            drop_to_fit(&mut pieces, 1, 5, 1, Precedence::Trailing),
            None
        );
        assert_eq!(pieces.len(), 1);
    }

    #[test]
    fn precedence_reads_lt_and_defaults_to_tl() {
        assert_eq!(Precedence::from_config(" LT "), Precedence::Leading);
        assert_eq!(Precedence::from_config("tl"), Precedence::Trailing);
        assert_eq!(Precedence::from_config("anything"), Precedence::Trailing);
    }

    /// Columns a marker takes when it prints its count, such as `← +3`:
    /// an arrow, a space, a plus sign and the digits.
    fn marker_width(hidden: usize) -> usize {
        3 + hidden.to_string().len()
    }

    /// Columns drawn for a window, summed independently of `window`: items,
    /// the marker on each side that hides something, and a spacer between
    /// every pair of neighbors.
    fn drawn_width(
        item_widths: &[usize],
        spacer_width: usize,
        result: &Window,
        marker: impl Fn(usize) -> usize,
    ) -> usize {
        let mut widths: Vec<usize> = item_widths[result.range.clone()].to_vec();
        if result.hidden_before > 0 {
            widths.insert(0, marker(result.hidden_before));
        }
        if result.hidden_after > 0 {
            widths.push(marker(result.hidden_after));
        }
        widths.iter().sum::<usize>() + spacer_width * widths.len().saturating_sub(1)
    }

    /// Check the bookkeeping every result must satisfy.
    fn assert_consistent(item_widths: &[usize], result: &Window) {
        assert_eq!(result.hidden_before, result.range.start);
        assert_eq!(result.hidden_after, item_widths.len() - result.range.end);
    }

    #[test]
    fn a_window_that_fits_shows_everything() {
        // Three items of 4 and two spacers of 1.
        let widths = [4, 4, 4];
        let result = window(&widths, 1, 1, 14, marker_width, marker_width);
        assert_eq!(
            result,
            Window {
                range: 0..3,
                hidden_before: 0,
                hidden_after: 0
            }
        );
    }

    #[test]
    fn a_window_pinned_at_the_first_item_hides_only_the_end() {
        let widths = [3; 10];
        let limit = 20;
        let result = window(&widths, 0, 1, limit, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert_eq!(result.range.start, 0);
        assert_eq!(result.hidden_before, 0);
        assert!(result.hidden_after > 0);
        // Four items of 3, three spacers, then a spacer and a 4 column marker
        // make 20. A fifth item would make 24.
        assert_eq!(result.range, 0..4);
        assert!(drawn_width(&widths, 1, &result, marker_width) <= limit);
    }

    #[test]
    fn a_window_pinned_at_the_last_item_hides_only_the_start() {
        let widths = [3; 10];
        let limit = 20;
        let result = window(&widths, 9, 1, limit, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert_eq!(result.range, 6..10);
        assert_eq!(result.hidden_before, 6);
        assert_eq!(result.hidden_after, 0);
        assert!(drawn_width(&widths, 1, &result, marker_width) <= limit);
    }

    #[test]
    fn a_window_pinned_in_the_middle_stays_roughly_centered() {
        let widths = [3; 21];
        let limit = 40;
        let pinned = 10;
        let result = window(&widths, pinned, 1, limit, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert!(result.range.contains(&pinned));
        let before = pinned - result.range.start;
        let after = result.range.end - 1 - pinned;
        assert!(
            before.abs_diff(after) <= 1,
            "{before} before, {after} after"
        );
        assert!(result.hidden_before > 0 && result.hidden_after > 0);
        assert!(drawn_width(&widths, 1, &result, marker_width) <= limit);
    }

    #[test]
    fn a_single_item_is_always_shown() {
        let widths = [5];
        let result = window(&widths, 0, 1, 10, marker_width, marker_width);
        assert_eq!(
            result,
            Window {
                range: 0..1,
                hidden_before: 0,
                hidden_after: 0
            }
        );
        // Wider than the limit, the single item is still kept for the caller to cut.
        let result = window(&widths, 0, 1, 2, marker_width, marker_width);
        assert_eq!(result.range, 0..1);
    }

    #[test]
    fn a_pinned_item_too_wide_alone_is_kept_with_everything_else_hidden() {
        let widths = [3, 3, 30, 3, 3];
        let result = window(&widths, 2, 1, 10, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert_eq!(result.range, 2..3);
        assert_eq!(result.hidden_before, 2);
        assert_eq!(result.hidden_after, 2);
    }

    #[test]
    fn marker_widths_that_grow_with_the_count_are_charged_for_the_final_count() {
        // Fourteen items of 1 with spacers of 1, and a marker that is 4
        // columns for a single digit count and 5 for two digits. Showing 5
        // items hides 9 and takes 9 + 1 + 4 = 14. Showing 4 hides 10, so the
        // marker is a column wider: 7 + 1 + 5 = 13.
        let widths = [1; 14];
        let result = window(&widths, 0, 1, 14, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert_eq!(result.range, 0..5);
        assert_eq!(result.hidden_after, 9);
        assert_eq!(drawn_width(&widths, 1, &result, marker_width), 14);

        let result = window(&widths, 0, 1, 13, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert_eq!(result.range, 0..4);
        assert_eq!(result.hidden_after, 10);
        assert_eq!(drawn_width(&widths, 1, &result, marker_width), 13);

        // Three items hide 11 and take 5 + 1 + 5 = 11, so a limit of 12 still
        // cannot hold four.
        let result = window(&widths, 0, 1, 12, marker_width, marker_width);
        assert_eq!(result.range, 0..3);
        assert_eq!(result.hidden_after, 11);
        assert!(drawn_width(&widths, 1, &result, marker_width) <= 12);
    }

    #[test]
    fn each_side_uses_its_own_marker_width() {
        let widths = [2; 9];
        let before = |hidden: usize| 10 + hidden;
        let after = |hidden: usize| 1 + hidden;
        let limit = 25;
        let result = window(&widths, 4, 1, limit, before, after);
        assert_consistent(&widths, &result);
        let mut total = 0;
        total += result.range.len() * 2 + (result.range.len() - 1);
        if result.hidden_before > 0 {
            total += before(result.hidden_before) + 1;
        }
        if result.hidden_after > 0 {
            total += after(result.hidden_after) + 1;
        }
        assert!(total <= limit, "drawn {total}");
        assert!(result.range.contains(&4));
    }

    #[test]
    fn a_zero_spacer_adds_no_gaps() {
        let widths = [2; 10];
        let limit = 13;
        let result = window(&widths, 5, 0, limit, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert!(result.range.contains(&5));
        assert!(drawn_width(&widths, 0, &result, marker_width) <= limit);
        // Two 4 column markers leave room for two items of 2: 4 + 4 + 4 = 12,
        // and a third item would make 14.
        assert_eq!(result.range.len(), 2);
    }

    #[test]
    fn an_empty_bar_has_an_empty_window() {
        let result = window(&[], 0, 1, 10, marker_width, marker_width);
        assert_eq!(
            result,
            Window {
                range: 0..0,
                hidden_before: 0,
                hidden_after: 0
            }
        );
    }

    #[test]
    fn a_pinned_index_past_the_end_is_clamped_to_the_last_item() {
        let widths = [3; 10];
        let clamped = window(&widths, 99, 1, 20, marker_width, marker_width);
        assert_eq!(
            clamped,
            window(&widths, 9, 1, 20, marker_width, marker_width)
        );
        assert!(clamped.range.contains(&9));
    }

    #[test]
    fn a_side_that_cannot_take_its_next_item_leaves_the_turn_to_the_other() {
        // A wide neighbor before the pinned item never fits, so the narrow
        // items after it keep being added. With the first four, the marker
        // for the last one hidden would push the line to 21, but showing all
        // five drops that marker and fits in 19.
        let widths = [30, 2, 2, 2, 2, 2];
        let limit = 20;
        let result = window(&widths, 1, 1, limit, marker_width, marker_width);
        assert_consistent(&widths, &result);
        assert_eq!(result.hidden_before, 1);
        assert_eq!(result.hidden_after, 0);
        assert_eq!(result.range, 1..6);
        assert!(drawn_width(&widths, 1, &result, marker_width) <= limit);
    }
}
