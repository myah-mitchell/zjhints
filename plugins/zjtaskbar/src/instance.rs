//! What the bars of one instance share.
//!
//! Zellij runs one instance of the plugin for every pane that loads it from
//! the same place with the same `instance` option, and `zjkit::shared`
//! keeps a `State` of its own for each of those panes, its slot. Most of
//! what a bar knows is its own: its options, what it drew, the client it
//! draws for. What follows is the instance's instead, and every slot sees
//! the same:
//!
//! - The panes minimized from any bar, each with where it stood, so every
//!   bar lists them and any of them can bring one back. Each bar still lists
//!   only the tabs its own `pane_scope` takes in.
//! - The slots running the bar, so a bar can tell where the others stand:
//!   it never lists one of them, and it leaves the panes of a tab that has a
//!   bar to that bar. See `frame`.
//! - The bars whose frame close button asks before closing. A pane can keep
//!   one bar's buttons after it moves to another bar's tab, and its close
//!   button then asks if any bar would. See `frame`.
//!
//! Which bar's buttons are on a pane's frame is the instance's too, but
//! zjkit keeps that, as it sends the clicks on them by it. See
//! `zjkit::shared::frame_owner`.
//!
//! An instance is a WebAssembly module of its own, run on one thread, and
//! every slot's `State` lives in it, so they reach what they share through a
//! thread-local: a `State` made by `Default`, as `Shared` makes each one,
//! joins the instance's. A `State` that stands alone, such as one drawn for
//! the docs images or one a test drives by itself, takes `Instance::alone`
//! instead, so nothing it does reaches another.

use crate::entries::Minimized;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeSet;
use std::rc::Rc;

/// What the slots share. See the module's docs.
#[derive(Debug, Default)]
pub(crate) struct Common {
    /// The panes minimized from any bar, with where each stood.
    pub minimized: Minimized,
    /// The plugin id of each slot running the bar, which is its pane's id.
    pub bars: BTreeSet<u32>,
    /// The plugin ids of the bars whose frame close button asks first.
    pub confirm_frame_close: BTreeSet<u32>,
}

thread_local! {
    /// The instance's `Common`, which every `Instance::default` shares.
    static COMMON: RefCell<Rc<RefCell<Common>>> = RefCell::default();
}

/// A slot's way to what the slots share.
///
/// Every borrow is let go of before the bar calls into Zellij, so no call
/// finds it borrowed: nothing reaches another slot's `State` until this
/// one's call is over.
#[derive(Clone, Debug)]
pub(crate) struct Instance(Rc<RefCell<Common>>);

impl Default for Instance {
    /// The instance's, shared with every other slot.
    fn default() -> Self {
        COMMON.with(|common| Instance(Rc::clone(&common.borrow())))
    }
}

impl Instance {
    /// One shared with no other `State`.
    pub(crate) fn alone() -> Self {
        Instance(Rc::default())
    }

    pub(crate) fn get(&self) -> Ref<'_, Common> {
        self.0.borrow()
    }

    pub(crate) fn get_mut(&self) -> RefMut<'_, Common> {
        self.0.borrow_mut()
    }
}

/// Start the instance's `Common` again, empty, for the next `State`s made by
/// `Default`, as a test of several slots does first. Those made before keep
/// what they had.
#[cfg(test)]
pub(crate) fn reset() {
    COMMON.with(|common| *common.borrow_mut() = Rc::default());
}
