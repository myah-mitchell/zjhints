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
/// caller cuts it instead, as `crate::ansi::truncate_ansi_string` does.
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
}
