//! Running a plugin as a shared instance: one instance serves every pane that
//! loads it, and Zellij calls it once per pane, each pane a slot.
//!
//! `Shared<P>` keeps a whole plugin state, a `P`, per slot, so a plugin
//! written for one pane runs unchanged in each. Each plugin registers it with
//! `register_shared_plugin!(zjkit::shared::Shared<State>)`.
//!
//! - `slot_added` builds the slot's `P` and loads it with the slot's
//!   configuration, then hands it what the instance already knows and the new
//!   slot would otherwise never see: Zellij sends a `ModeUpdate`, `TabUpdate`
//!   or `PaneUpdate` to an instance only when it changes.
//! - An event or message goes to the slot it is for: the slot Zellij names,
//!   the slot that set the timer, placed the frame item, showed the prompt or
//!   ran the command, and otherwise every slot.
//! - Zellij sends a `ModeUpdate` and a `TabUpdate` per client. Each is kept
//!   per client, and a slot that last saw another client's is given this
//!   client's before it draws for it or takes its mouse or keys.
//! - The calls a `P` makes about its own pane go through the `Host` as
//!   before. While a slot's `P` runs, the slot is the acting slot, and
//!   `host::Zellij` hands those calls here instead of making them, so they
//!   can be made for the acting slot (see `Effect`). Each is made at once,
//!   in the order the `P` makes it, as it would be without `Shared`. Zellij
//!   takes some of them only while it draws the slot or while the event it
//!   sends is about it; those wait in the slot's queue until it draws next.
//! - Zellij reads both a drawing and every call into it from the plugin's
//!   standard output. A call made after the `P` has printed what it draws
//!   would take that text for part of the call, so nothing runs after the
//!   `P` draws, and what it printed is flushed before `render` returns.
//! - Zellij keeps each slot's frame items on a pane apart and shows them
//!   side by side, and sends a click on one to the instance with no slot.
//!   So the slot whose items are on a pane is kept, and a click on that
//!   pane goes to it; `frame_owner` tells a slot which one that is, so the
//!   slots can agree to leave a pane to one of them.
//! - Permissions are the instance's, so a slot asks Zellij only for what no
//!   slot has asked for yet, and every slot waiting on the answer gets it.
//!   Zellij shows the prompt in the pane of the slot it draws, and loses a
//!   prompt for a pane it does not have yet, so a request made away from a
//!   render, as a slot loads, goes out in the next render of a slot waiting
//!   on it: the slot that asked may never be drawn while another slot that
//!   asked for the same is. The slot showing the prompt keeps its row while
//!   the prompt is up, as a collapsed pane would hide it, and its bar says
//!   again whether to keep it once answered. A slot reads its
//!   own pane once granted, so a slot told of a grant as it loads is told
//!   when it first draws or Zellij first lists its pane. One request is out
//!   at a time, and a denial takes away every slot's grant, as Zellij's
//!   answer to a prompt replaces what the instance had (see `Permissions`).
//! - A slot started from the same URL as another one takes its visibility, so
//!   a bar in a new tab is hidden while the others are. A bar hidden or
//!   shown gives up or takes back its row at once, not as it draws: Zellij
//!   draws neither a collapsed slot nor one on a tab nobody looks at.
//!
//! The routing is all in plain methods that return the calls to make, as
//! `Outcome`s, so tests drive it without Zellij. Only the `ZellijSharedPlugin`
//! implementation makes the calls.

use crate::bar::BarCore;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, SystemTime};
use zellij_tile::prelude::*;

/// The key added to the context of a command or action a slot runs, so the
/// result goes back to that slot. Removed again before the slot sees it.
const SLOT_CONTEXT_KEY: &str = "zjkit_slot";

/// What `Shared` needs of a plugin beyond `ZellijPlugin`: its `BarCore` when
/// the slot runs the bar, and what it does when it runs in the background.
pub trait SlotPlugin: ZellijPlugin + Default {
    /// The slot's `BarCore`, when the slot runs the bar rather than the
    /// settings screen or a popup. `Shared` reads the bar's URL and
    /// visibility through it.
    fn bar(&self) -> Option<&BarCore> {
        None
    }

    /// The same `BarCore`, to change.
    fn bar_mut(&mut self) -> Option<&mut BarCore> {
        None
    }

    /// Called after every event and message for a slot loaded in the
    /// background, with `load_plugins`, which Zellij never draws.
    fn background(&mut self) {}

    /// Called on each slot left once slot `id` has closed, after Zellij may
    /// already have sent the `PaneUpdate` for its pane closing. Returns
    /// whether the slot should draw again.
    fn slot_removed(&mut self, _id: SlotId) -> bool {
        false
    }
}

/// The slot whose `P` is running, and the client it runs for when Zellij
/// named one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Acting {
    pub slot: SlotId,
    pub client: Option<ClientId>,
}

/// The acting slot, with what its calls are routed by: its kind, where
/// Zellij is calling from, and the `Router`, lent by `Shared` while the
/// slot's `P` runs.
struct Active {
    acting: Acting,
    kind: SlotKind,
    callback: Callback,
    router: Router,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

/// The acting slot, while a slot's `P` runs.
pub(crate) fn acting() -> Option<Acting> {
    ACTIVE.with(|active| active.borrow().as_ref().map(|active| active.acting))
}

/// Hand `command` over for the acting slot, which routes it at once.
/// Returns false when no slot is acting, and the caller makes the call
/// itself.
pub(crate) fn submit(command: Command) -> bool {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let Some(active) = active.as_mut() else {
            return false;
        };
        let slot = active.acting.slot;
        active
            .router
            .route(slot, active.kind, active.callback, command);
        true
    })
}

/// `context` marked with the acting slot, if any, so the result of the
/// command or action it goes with comes back to that slot.
pub(crate) fn tag_context(mut context: BTreeMap<String, String>) -> BTreeMap<String, String> {
    if let Some(acting) = acting() {
        context.insert(SLOT_CONTEXT_KEY.to_string(), acting.slot.to_string());
    }
    context
}

/// The slot that may place frame items on `pane`: the one whose items are
/// on it, or wait in its queue for its next render to go there. `None` when
/// no slot's are, and while no slot is acting, as outside `Shared`.
///
/// A slot's items stay on a pane until the slot takes them off, which takes
/// effect only in its own render, so a pane is the slot's until then. A
/// slot that takes off items it never placed gives the pane up at once.
pub fn frame_owner(pane: PaneId) -> Option<SlotId> {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .and_then(|active| active.router.frame_owner(pane))
    })
}

/// Note that the acting slot has a timer coming that it did not set through
/// the `Host`, as a `zellij_tile` button sets one when it is pressed, so the
/// timer is still counted as the slot's.
pub(crate) fn expect_timer(seconds: f64) {
    submit(Command::ExpectTimer(seconds));
}

fn begin(active: Active) {
    ACTIVE.with(|slot| *slot.borrow_mut() = Some(active));
}

/// End the acting slot's run, handing back the `Router`.
fn end() -> Router {
    ACTIVE
        .with(|active| active.borrow_mut().take())
        .map(|active| active.router)
        .unwrap_or_default()
}

/// A call the acting slot hands over, which `Router::route` makes for it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    SetSelectable(bool),
    SetCollapsed(bool),
    ShowSelf,
    CloseSelf,
    SetTimeout(f64),
    /// A timer set by other means; see `expect_timer`.
    ExpectTimer(f64),
    RequestPermission(Vec<PermissionType>),
    SetPaneFrameItems(PaneId, Vec<FrameItem>),
    OpenContextMenu {
        target: ContextMenuTarget,
        line: usize,
        column: usize,
    },
    OpenPopup(PopupRequest),
    /// The slot showed a prompt, which Zellij gave this id.
    Prompted(u64),
}

/// A popup to open, with what `open_plugin_popup_with_options` takes.
#[derive(Clone, Debug, PartialEq)]
pub struct PopupRequest {
    pub url: String,
    pub configuration: BTreeMap<String, String>,
    pub line: usize,
    pub column: usize,
    pub width: usize,
    pub height: usize,
    pub options: PopupOptions,
}

/// A call `Shared` makes into Zellij for a slot.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    SetSelectable(SlotId, bool),
    SetCollapsed(SlotId, bool),
    /// Show the slot's pane, floating it if it was hidden.
    Show(SlotId),
    Close(SlotId),
    SetTimeout(f64),
    RequestPermission(Vec<PermissionType>),
    SetPaneFrameItems(PaneId, Vec<FrameItem>),
    OpenContextMenu {
        target: ContextMenuTarget,
        line: usize,
        column: usize,
    },
    OpenPopup(PopupRequest),
}

impl Effect {
    /// Make the call, while Zellij draws or adds slot `drawing`, if any.
    ///
    /// A call about the drawing slot's pane is made in the form Zellij takes
    /// only then, which it answers with nothing. Another slot's is made in
    /// the slot form, which Zellij answers with an error for a slot it no
    /// longer has; the error goes to the log.
    pub fn perform(self, drawing: Option<SlotId>) {
        let report = |result: Result<(), String>| {
            if let Err(error) = result {
                eprintln!("zjkit: {error}");
            }
        };
        let own = |slot: SlotId| drawing == Some(slot);
        match self {
            Self::SetSelectable(slot, selectable) if own(slot) => set_selectable(selectable),
            Self::SetSelectable(slot, selectable) => {
                report(set_selectable_slot(slot, selectable));
            }
            Self::SetCollapsed(slot, collapsed) if own(slot) => set_self_collapsed(collapsed),
            Self::SetCollapsed(slot, collapsed) => report(set_collapsed_slot(slot, collapsed)),
            Self::Show(slot) if own(slot) => show_self(true),
            Self::Show(slot) => report(show_slot(slot, true)),
            Self::Close(slot) if own(slot) => close_self(),
            Self::Close(slot) => report(close_slot(slot)),
            Self::SetTimeout(seconds) => set_timeout(seconds),
            Self::RequestPermission(permissions) => request_permission(&permissions),
            Self::SetPaneFrameItems(pane_id, items) => set_pane_frame_items(pane_id, items),
            Self::OpenContextMenu {
                target,
                line,
                column,
            } => open_context_menu(target, line, column),
            Self::OpenPopup(popup) => open_plugin_popup_with_options(
                popup.url,
                popup.configuration,
                popup.line,
                popup.column,
                popup.width,
                popup.height,
                popup.options,
            ),
        }
    }

    /// Whether this, waiting in a slot's queue, makes `earlier` pointless:
    /// new frame items for the same pane.
    fn replaces(&self, earlier: &Effect) -> bool {
        match (self, earlier) {
            (Self::SetPaneFrameItems(pane, _), Self::SetPaneFrameItems(earlier, _)) => {
                pane == earlier
            }
            _ => false,
        }
    }
}

/// What one call into `Shared` leaves to do: the calls to make into Zellij,
/// in order, unless `Shared` made them as they came (see `Shared::live`),
/// and which slots to draw again.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub effects: Vec<Effect>,
    pub render: RenderResponse,
}

/// The timers the slots set, to tell which slot a `Timer` event is for.
///
/// Zellij sends a timer back to the instance with no slot, so each one set
/// is kept with the time it is due. Timers fire in the order they are due,
/// so a `Timer` event is for the earliest one still waiting, and of two due
/// at the same moment, the one set first.
#[derive(Debug, Default)]
struct Timers {
    /// Each timer waiting: when it is due, the order it was set in, and the
    /// slot that set it.
    pending: Vec<(Duration, u64, SlotId)>,
    sequence: u64,
}

impl Timers {
    fn set(&mut self, now: Duration, seconds: f64, slot: SlotId) {
        let delay = Duration::try_from_secs_f64(seconds).unwrap_or_default();
        self.sequence += 1;
        self.pending.push((now + delay, self.sequence, slot));
    }

    /// The slot the timer that just fired belongs to, which is then no
    /// longer waiting. `None` when no timer is.
    fn fired(&mut self) -> Option<SlotId> {
        let earliest = self
            .pending
            .iter()
            .enumerate()
            .min_by_key(|(_, (due, sequence, _))| (*due, *sequence))
            .map(|(index, _)| index)?;
        Some(self.pending.remove(earliest).2)
    }
}

/// The instance's permissions: what Zellij granted, what it was last asked
/// for, and the slots waiting on that answer.
///
/// A grant replaces what the instance had, so a request always carries
/// everything granted or asked for so far as well as what the slot needs.
/// A denial leaves the instance with nothing.
///
/// One request is out at a time. Zellij shows each prompt in a pane of its
/// own and grants exactly what the answered prompt asked for, so with two
/// out, the answer to the smaller one could come last and take away what
/// the larger one gave. What a slot asks for while a prompt is up waits in
/// `next` and goes out once that prompt is answered.
#[derive(Debug, Default)]
struct Permissions {
    granted: Vec<PermissionType>,
    pending: Option<Vec<PermissionType>>,
    waiting: BTreeSet<SlotId>,
    /// Whether `pending` is still to be asked for, in the next render of a
    /// slot in `waiting`, or of any pane slot when none of them has a pane.
    unsent: bool,
    /// The slot Zellij shows the prompt for `pending` in, once asked.
    prompter: Option<SlotId>,
    /// What slots asked for while the prompt for `pending` was up, to ask
    /// for once it is answered.
    next: Vec<PermissionType>,
    /// The slots waiting on `next`.
    next_waiting: BTreeSet<SlotId>,
}

/// What a slot asking for permissions leads to.
#[derive(Debug, PartialEq)]
enum Ask {
    /// Granted already: the slot is told so without asking Zellij.
    Granted,
    /// Asked for already, or to ask for once the prompt up is answered: the
    /// slot waits for that answer.
    Wait,
    /// Ask Zellij for this whole set.
    Request(Vec<PermissionType>),
}

fn covers(set: &[PermissionType], wanted: &[PermissionType]) -> bool {
    wanted.iter().all(|permission| set.contains(permission))
}

/// `set` with each of `more` it lacks added.
fn joined(mut set: Vec<PermissionType>, more: &[PermissionType]) -> Vec<PermissionType> {
    for permission in more {
        if !set.contains(permission) {
            set.push(*permission);
        }
    }
    set
}

impl Permissions {
    fn ask(&mut self, slot: SlotId, wanted: &[PermissionType]) -> Ask {
        if covers(&self.granted, wanted) {
            return Ask::Granted;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| covers(pending, wanted))
        {
            self.waiting.insert(slot);
            return Ask::Wait;
        }
        if self.pending.is_some() && !self.unsent {
            // A slot is told only the answer to what it asked for last, as
            // a bar takes a grant for everything it asked for.
            self.waiting.remove(&slot);
            self.next = joined(std::mem::take(&mut self.next), wanted);
            self.next_waiting.insert(slot);
            return Ask::Wait;
        }
        self.waiting.insert(slot);
        let asked = self.pending.clone().unwrap_or_else(|| self.granted.clone());
        let all = joined(asked, wanted);
        self.pending = Some(all.clone());
        Ask::Request(all)
    }

    /// Whether Zellij shows, or may yet show, a prompt in slot `id`'s pane.
    fn prompting(&self, id: SlotId) -> bool {
        self.prompter == Some(id) && self.pending.is_some() && !self.unsent
    }

    /// Forget slot `id`, which is gone. A prompt in its pane went with it,
    /// and Zellij never answers it, so it is asked for again for the slots
    /// still waiting. A request no slot waits on any more is dropped.
    fn forget(&mut self, id: SlotId) {
        self.waiting.remove(&id);
        self.next_waiting.remove(&id);
        if self.prompter == Some(id) {
            self.prompter = None;
            self.unsent = self.pending.is_some();
        }
        if self.unsent && self.waiting.is_empty() {
            self.unsent = false;
            self.pending = None;
            self.ask_next();
        }
    }

    /// Take Zellij's answer. Returns the slots that were waiting on it.
    fn answer(&mut self, status: &PermissionStatus) -> BTreeSet<SlotId> {
        self.unsent = false;
        self.prompter = None;
        let asked = self.pending.take();
        self.granted = match status {
            PermissionStatus::Granted => asked.unwrap_or_else(|| self.granted.clone()),
            PermissionStatus::Denied => Vec::new(),
        };
        let mut told = std::mem::take(&mut self.waiting);
        if covers(&self.granted, &self.next) {
            told.append(&mut self.next_waiting);
            self.next.clear();
        }
        self.ask_next();
        told
    }

    /// Make what slots asked for while a prompt was up the next request,
    /// still to be asked for, once nothing is pending.
    fn ask_next(&mut self) {
        if self.pending.is_some() || self.next_waiting.is_empty() {
            return;
        }
        let next = std::mem::take(&mut self.next);
        self.pending = Some(joined(self.granted.clone(), &next));
        self.waiting = std::mem::take(&mut self.next_waiting);
        self.unsent = true;
    }
}

/// The events Zellij sends per client, which are kept per client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Seeded {
    Keybinds,
    Mode,
    Tabs,
}

impl Seeded {
    /// In the order a slot is given them: the keybindings before the mode
    /// they go with.
    const ALL: [Seeded; 3] = [Seeded::Keybinds, Seeded::Mode, Seeded::Tabs];

    fn of(event: &Event) -> Option<Self> {
        match event {
            Event::InitialKeybinds(_) => Some(Self::Keybinds),
            Event::ModeUpdate(_) => Some(Self::Mode),
            Event::TabUpdate(_) => Some(Self::Tabs),
            _ => None,
        }
    }
}

/// What Zellij was last told to do with the slot, and where it is calling
/// from.
#[derive(Clone, Copy, Debug)]
struct Callback {
    /// The slot Zellij counts as the current one: the slot it draws or adds,
    /// or the one the event is about.
    slot: Option<SlotId>,
    /// Whether Zellij takes calls about the current slot's own pane, which it
    /// does only while it draws or adds the slot.
    self_commands: bool,
    /// Whether the slot is being added, when Zellij has not put its pane in
    /// place yet.
    adding: bool,
}

impl Callback {
    const NONE: Self = Self {
        slot: None,
        self_commands: false,
        adding: false,
    };

    fn drawing(slot: SlotId) -> Self {
        Self {
            slot: Some(slot),
            self_commands: true,
            adding: false,
        }
    }

    fn adding(slot: SlotId) -> Self {
        Self {
            adding: true,
            ..Self::drawing(slot)
        }
    }

    /// The slot Zellij draws or adds, if it is doing either.
    fn drawn(self) -> Option<SlotId> {
        self.slot.filter(|_| self.self_commands)
    }
}

/// One slot's plugin state and what `Shared` keeps about it.
struct SlotState<P> {
    plugin: P,
    kind: SlotKind,
    /// The sequence number of each per-client event the slot last got.
    seen: BTreeMap<Seeded, u64>,
    /// Whether the slot is still to take its visibility from another slot.
    adopting: bool,
    /// Whether the slot's bar was hidden when `Shared` last looked.
    hidden: bool,
}

/// A plugin `P` run as a shared instance, a `P` per slot. See the module
/// documentation.
pub struct Shared<P> {
    slots: BTreeMap<SlotId, SlotState<P>>,
    /// The last per-client events, by client, each with its sequence number.
    clients: BTreeMap<ClientId, BTreeMap<Seeded, (u64, Event)>>,
    /// The client whose per-client event came last, whose events a new slot
    /// is given.
    latest_client: Option<ClientId>,
    sequence: u64,
    /// The last `PaneUpdate`, which a new slot is given.
    panes: Option<PaneManifest>,
    /// The last `InitialKeybinds` sent with no client, which a new slot is
    /// given when no client's are known.
    keybinds: Option<Event>,
    router: Router,
}

/// What routing a slot's calls keeps and changes, which `Shared` lends to the
/// acting slot while its `P` runs, so each call is routed as it is made.
struct Router {
    timers: Timers,
    permissions: Permissions,
    /// The slot that placed the frame items on each pane.
    frame_owners: BTreeMap<PaneId, SlotId>,
    /// The slot that showed each prompt, by request id, until it is
    /// answered, also once the slot has closed.
    prompts: BTreeMap<u64, SlotId>,
    /// The calls waiting for each slot's next render.
    deferred: BTreeMap<SlotId, Vec<Effect>>,
    /// The time now, which timers are counted from.
    clock: fn() -> Duration,
    /// What makes each call as it comes, with the slot Zellij draws or adds,
    /// if any. Without one, the calls are kept in `effects`.
    performer: Option<fn(Effect, Option<SlotId>)>,
    /// The calls kept for the `Outcome` of the call into `Shared` under way.
    effects: Vec<Effect>,
    redraw: BTreeSet<SlotId>,
    /// Events for a slot that wait until the slot that caused them is done.
    queued: VecDeque<(SlotId, Event)>,
    /// Events for a slot still being added, which wait until Zellij has its
    /// pane: its first render, or the first `PaneUpdate` that lists it.
    held: BTreeMap<SlotId, Vec<Event>>,
    /// The slots whose panes Zellij was last told to collapse.
    collapsed: BTreeSet<SlotId>,
    /// The bars whose row is not what they last told Zellij, as a prompt
    /// was up in their pane: a collapse held back, or a pane expanded to
    /// show the prompt. Each says again once the prompt is answered.
    retell: BTreeSet<SlotId>,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            timers: Timers::default(),
            permissions: Permissions::default(),
            frame_owners: BTreeMap::new(),
            prompts: BTreeMap::new(),
            deferred: BTreeMap::new(),
            clock: system_clock,
            performer: None,
            effects: Vec::new(),
            redraw: BTreeSet::new(),
            queued: VecDeque::new(),
            held: BTreeMap::new(),
            collapsed: BTreeSet::new(),
            retell: BTreeSet::new(),
        }
    }
}

fn system_clock() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
}

impl<P> Default for Shared<P> {
    fn default() -> Self {
        Self {
            slots: BTreeMap::new(),
            clients: BTreeMap::new(),
            latest_client: None,
            sequence: 0,
            panes: None,
            keybinds: None,
            router: Router::default(),
        }
    }
}

impl<P: SlotPlugin> Shared<P> {
    /// The plugin state of slot `id`.
    pub fn plugin(&self, id: SlotId) -> Option<&P> {
        self.slots.get(&id).map(|state| &state.plugin)
    }

    /// The same plugin state, to change.
    pub fn plugin_mut(&mut self, id: SlotId) -> Option<&mut P> {
        self.slots.get_mut(&id).map(|state| &mut state.plugin)
    }

    /// Every slot's id, in order.
    pub fn slot_ids(&self) -> Vec<SlotId> {
        self.slots.keys().copied().collect()
    }

    /// Start slot `slot`: build its `P`, load it with the slot's
    /// configuration, and hand it what the instance already knows. A slot
    /// added after a grant that covers what it asks for is told it is
    /// granted, without asking Zellij again.
    pub fn add_slot(&mut self, slot: Slot) -> Outcome {
        self.deliver_queued(Callback::NONE);
        let id = slot.id;
        let callback = Callback::adding(id);
        self.slots.insert(
            id,
            SlotState {
                plugin: P::default(),
                kind: slot.kind,
                seen: BTreeMap::new(),
                adopting: true,
                hidden: false,
            },
        );
        let configuration = slot.configuration;
        self.deliver(id, None, callback, |plugin| {
            plugin.load(configuration);
            false
        });
        match self.latest_client {
            Some(client) => self.sync(id, client, callback),
            None => {
                if let Some(keybinds) = self.keybinds.clone() {
                    self.update_slot(id, None, callback, keybinds);
                }
            }
        }
        if let Some(panes) = self.panes.clone() {
            self.update_slot(id, None, callback, Event::PaneUpdate(panes));
        }
        self.adopt_visibility();
        // Hidden as it starts, the bar gives its row up as it first draws.
        if let Some(state) = self.slots.get_mut(&id) {
            state.hidden = state
                .plugin
                .bar()
                .is_some_and(|bar| bar.visibility.is_hidden());
        }
        self.finish(callback)
    }

    /// End slot `id`: its `P` gets `BeforeClose`, as a plugin of its own
    /// would as its pane closes, and is dropped.
    pub fn remove_slot(&mut self, id: SlotId) -> Outcome {
        self.update_slot(id, None, Callback::NONE, Event::BeforeClose);
        self.slots.remove(&id);
        let router = &mut self.router;
        router.frame_owners.retain(|_, owner| *owner != id);
        // The prompts it showed are kept, so an answer to one still goes to
        // it, and so to no slot, rather than to every slot.
        router.permissions.forget(id);
        if router.permissions.unsent {
            let waiting = router.permissions.waiting.clone();
            router.redraw.extend(waiting);
        }
        router.deferred.remove(&id);
        router.held.remove(&id);
        router.collapsed.remove(&id);
        router.retell.remove(&id);
        router.queued.retain(|(slot, _)| *slot != id);
        for other in self.slot_ids() {
            self.deliver(other, None, Callback::NONE, |plugin| {
                plugin.slot_removed(id)
            });
        }
        self.finish(Callback::NONE)
    }

    /// Hand `event` to the slots it is for. See the module documentation.
    pub fn take_event(&mut self, event: Event, context: EventContext) -> Outcome {
        self.deliver_queued(Callback::NONE);
        let callback = Callback {
            slot: context.slot_id,
            self_commands: false,
            adding: false,
        };
        let client = context.client_id;
        match event {
            Event::ModeUpdate(_) | Event::TabUpdate(_) | Event::InitialKeybinds(_) => {
                self.take_seeded(event, client, callback);
            }
            Event::PaneUpdate(ref manifest) => {
                self.panes = Some(manifest.clone());
                self.update_all(client, callback, &event);
                self.adopt_visibility();
                self.release_held(manifest);
            }
            Event::Timer(_) => {
                if let Some(id) = self.router.timers.fired() {
                    self.update_slot(id, client, callback, event);
                }
            }
            Event::FrameItemClicked(ref click) => {
                if let Some(&id) = self.router.frame_owners.get(&click.pane_id) {
                    self.update_from_client(id, client, callback, event);
                }
            }
            Event::PromptResult(request_id, _) => match self.router.prompts.remove(&request_id) {
                Some(id) => self.update_slot(id, client, callback, event),
                None => self.update_all(client, callback, &event),
            },
            Event::RunCommandResult(exit_code, stdout, stderr, context) => {
                let (slot, context) = untag_context(context);
                let event = Event::RunCommandResult(exit_code, stdout, stderr, context);
                self.update_slot_or_all(slot, client, callback, event);
            }
            Event::ActionComplete(action, pane_id, context) => {
                let (slot, context) = untag_context(context);
                let event = Event::ActionComplete(action, pane_id, context);
                self.update_slot_or_all(slot, client, callback, event);
            }
            Event::PermissionRequestResult(ref status) => {
                let had = !self.router.permissions.granted.is_empty();
                let mut told = self.router.permissions.answer(status);
                self.reopen_rows(&told);
                // A denial takes away what the instance was granted before,
                // so every slot told it was granted is told it is no more,
                // but for those waiting on the next request.
                if *status == PermissionStatus::Denied && had {
                    let next = &self.router.permissions.waiting;
                    let others: Vec<SlotId> = self
                        .slot_ids()
                        .into_iter()
                        .filter(|id| !next.contains(id))
                        .collect();
                    told.extend(others);
                }
                for id in told {
                    self.update_slot(id, client, callback, event.clone());
                }
                // What slots asked for while the prompt was up goes out next.
                if self.router.permissions.unsent {
                    let next = self.router.permissions.waiting.clone();
                    self.router.redraw.extend(next);
                }
            }
            event => match context.slot_id {
                Some(id) => self.update_from_client(id, client, callback, event),
                None => self.update_all(client, callback, &event),
            },
        }
        self.run_background(callback);
        self.finish(callback)
    }

    /// Hand `message` to the slot it is addressed to, or to every slot when
    /// it is addressed to the instance or to no one.
    ///
    /// A request from Zellij's settings screen goes to one bar alone, so only
    /// one settings screen opens: the bar on the screen of the client that
    /// made it, else the first bar. The bar is told that client as its own,
    /// so `host::settings_requested_by` passes.
    pub fn take_message(&mut self, message: PipeMessage, context: EventContext) -> Outcome {
        self.deliver_queued(Callback::NONE);
        let callback = Callback {
            slot: context.slot_id,
            self_commands: false,
            adding: false,
        };
        let client = context.client_id;
        if context.slot_id.is_none() && message.name == OPEN_PLUGIN_SETTINGS_MESSAGE {
            let requester = message
                .args
                .get(OPEN_PLUGIN_SETTINGS_CLIENT_ID_ARG)
                .and_then(|id| id.parse::<ClientId>().ok());
            if let Some(id) = self.settings_slot(requester) {
                // The bar answers only a request from the client it draws
                // for, which a shared bar has none of. It draws for the
                // requester while it answers this one alone: the argument
                // is whatever the sender wrote.
                let drew_for = self
                    .plugin_mut(id)
                    .and_then(SlotPlugin::bar_mut)
                    .map(|bar| std::mem::replace(&mut bar.client_id, requester));
                self.deliver(id, requester.or(client), callback, |plugin| {
                    plugin.pipe(message)
                });
                if let Some(bar) = self.plugin_mut(id).and_then(SlotPlugin::bar_mut) {
                    bar.client_id = drew_for.flatten();
                }
            }
        } else if let Some(id) = context.slot_id {
            self.deliver(id, client, callback, |plugin| plugin.pipe(message));
        } else {
            for id in self.slot_ids() {
                let message = message.clone();
                self.deliver(id, client, callback, |plugin| plugin.pipe(message));
            }
        }
        self.run_background(callback);
        self.finish(callback)
    }

    /// Draw slot `id` for `client`: give it that client's events if it last
    /// saw another's, and the events its calls led to, make the calls
    /// waiting in its queue, then render it.
    ///
    /// Nothing runs after the slot's `P` has drawn, as a call then would
    /// follow the drawing out. Events its drawing leads to wait for the next
    /// call into `Shared`, and come before what that call brings.
    ///
    /// Zellij takes no answer from a render, so the slots to draw again,
    /// other than this one, are kept for the next call that has one.
    pub fn render_slot(
        &mut self,
        rows: usize,
        cols: usize,
        id: SlotId,
        client: ClientId,
    ) -> Outcome {
        let callback = Callback::drawing(id);
        self.sync(id, client, callback);
        for event in self.router.held.remove(&id).unwrap_or_default() {
            self.router.queued.push_back((id, event));
        }
        self.deliver_queued(callback);
        self.follow_hidden(callback);
        if self.asks_in_render(id) {
            self.router.ask_zellij(id, Some(id), Some(id));
        }
        let deferred = self.router.deferred.remove(&id).unwrap_or_default();
        for effect in deferred {
            self.router.perform(id, effect, Some(id));
        }
        self.deliver(id, Some(client), callback, |plugin| {
            plugin.render(rows, cols);
            false
        });
        self.router.redraw.remove(&id);
        Outcome {
            effects: std::mem::take(&mut self.router.effects),
            render: RenderResponse::Nothing,
        }
    }

    /// Keep the slots `render` asks to draw again, except `drawn`, for the
    /// next call whose answer Zellij takes: it takes none from adding or
    /// removing a slot.
    fn keep_redraw(&mut self, render: RenderResponse, drawn: Option<SlotId>) {
        if let RenderResponse::Slots(ids) = render {
            let others = ids.into_iter().filter(|id| Some(*id) != drawn);
            self.router.redraw.extend(others);
        }
    }

    /// Whether the permissions still to be asked for go out as slot `id`
    /// draws: it waits on them, or no slot that does is ever drawn, as a
    /// background slot is not.
    fn asks_in_render(&self, id: SlotId) -> bool {
        let permissions = &self.router.permissions;
        let in_pane = |slot: &SlotId| {
            self.slots
                .get(slot)
                .is_some_and(|state| state.kind == SlotKind::Pane)
        };
        permissions.unsent
            && (permissions.waiting.contains(&id) || !permissions.waiting.iter().any(in_pane))
    }

    /// Make every call as it comes, through Zellij, rather than keep it for
    /// the `Outcome`. The `ZellijSharedPlugin` implementation does this.
    pub fn live(&mut self) {
        self.router.performer = Some(Effect::perform);
    }

    /// Run `call` on slot `id`'s `P` as the acting slot, then route the calls
    /// it handed over. Returns what `call` returned, which is whether to draw
    /// the slot again.
    fn deliver(
        &mut self,
        id: SlotId,
        client: Option<ClientId>,
        callback: Callback,
        call: impl FnOnce(&mut P) -> bool,
    ) -> bool {
        let Some(state) = self.slots.get_mut(&id) else {
            return false;
        };
        begin(Active {
            acting: Acting { slot: id, client },
            kind: state.kind,
            callback,
            router: std::mem::take(&mut self.router),
        });
        let redraw = call(&mut state.plugin);
        self.router = end();
        if redraw {
            self.router.redraw.insert(id);
        }
        redraw
    }

    fn update_slot(
        &mut self,
        id: SlotId,
        client: Option<ClientId>,
        callback: Callback,
        event: Event,
    ) {
        self.deliver(id, client, callback, |plugin| plugin.update(event));
    }

    fn update_all(&mut self, client: Option<ClientId>, callback: Callback, event: &Event) {
        for id in self.slot_ids() {
            self.update_slot(id, client, callback, event.clone());
        }
    }

    fn update_slot_or_all(
        &mut self,
        slot: Option<SlotId>,
        client: Option<ClientId>,
        callback: Callback,
        event: Event,
    ) {
        // A result for a slot that has closed is for no other.
        match slot {
            Some(id) => self.update_slot(id, client, callback, event),
            None => self.update_all(client, callback, &event),
        }
    }

    /// Hand slot `id` an event from `client`, such as a click, after giving
    /// it that client's mode and tabs if it last saw another's.
    fn update_from_client(
        &mut self,
        id: SlotId,
        client: Option<ClientId>,
        callback: Callback,
        event: Event,
    ) {
        if let Some(client) = client {
            self.sync(id, client, callback);
        }
        self.update_slot(id, client, callback, event);
    }

    /// Keep a per-client event and hand it to every slot.
    fn take_seeded(&mut self, event: Event, client: Option<ClientId>, callback: Callback) {
        let Some(kind) = Seeded::of(&event) else {
            return;
        };
        let Some(client) = client else {
            if kind == Seeded::Keybinds {
                self.keybinds = Some(event.clone());
            }
            self.update_all(None, callback, &event);
            return;
        };
        self.sequence += 1;
        let sequence = self.sequence;
        self.clients
            .entry(client)
            .or_default()
            .insert(kind, (sequence, event.clone()));
        self.latest_client = Some(client);
        for id in self.slot_ids() {
            if let Some(state) = self.slots.get_mut(&id) {
                state.seen.insert(kind, sequence);
            }
            self.update_slot(id, Some(client), callback, event.clone());
        }
    }

    /// Give slot `id` each of `client`'s per-client events it has not seen.
    fn sync(&mut self, id: SlotId, client: ClientId, callback: Callback) {
        let Some(seeds) = self.clients.get(&client) else {
            return;
        };
        let Some(state) = self.slots.get(&id) else {
            return;
        };
        let missing: Vec<(Seeded, u64, Event)> = Seeded::ALL
            .iter()
            .filter_map(|kind| {
                let (sequence, event) = seeds.get(kind)?;
                (state.seen.get(kind) != Some(sequence)).then(|| (*kind, *sequence, event.clone()))
            })
            .collect();
        for (kind, sequence, event) in missing {
            if let Some(state) = self.slots.get_mut(&id) {
                state.seen.insert(kind, sequence);
            }
            self.update_slot(id, Some(client), callback, event);
        }
    }

    /// Give each bar still to take its visibility the visibility of another
    /// bar started from the same URL, once its URL is known. A bar with no
    /// such sibling keeps its own.
    fn adopt_visibility(&mut self) {
        let adopting: Vec<SlotId> = self
            .slots
            .iter()
            .filter(|(_, state)| state.adopting)
            .map(|(id, _)| *id)
            .collect();
        for id in adopting {
            let Some(state) = self.slots.get(&id) else {
                continue;
            };
            let url = match state.plugin.bar() {
                Some(bar) => match &bar.own_url {
                    Some(url) => Some(url.clone()),
                    None => continue,
                },
                None => None,
            };
            let hidden = url.and_then(|url| {
                self.slots
                    .iter()
                    .filter(|(other, state)| **other != id && !state.adopting)
                    .find_map(|(_, state)| {
                        let bar = state.plugin.bar()?;
                        (bar.own_url.as_deref() == Some(url.as_str()))
                            .then(|| bar.visibility.is_hidden())
                    })
            });
            let Some(state) = self.slots.get_mut(&id) else {
                continue;
            };
            state.adopting = false;
            let changed = hidden.is_some_and(|hidden| {
                state
                    .plugin
                    .bar_mut()
                    .is_some_and(|bar| bar.visibility.adopt(hidden))
            });
            if changed {
                self.router.redraw.insert(id);
            }
        }
    }

    /// Once a prompt is answered, have each bar whose row the prompt kept
    /// from what it last told Zellij say again in its next render, and
    /// expand the pane of each bar in `waiting` that collapsed, so it can
    /// too. A bar with no permissions may draw nothing and collapse, and
    /// Zellij does not draw a collapsed slot, so it would never expand
    /// once it had something to draw.
    fn reopen_rows(&mut self, waiting: &BTreeSet<SlotId>) {
        let mut retell = std::mem::take(&mut self.router.retell);
        for &id in waiting {
            if self.router.collapsed.contains(&id) {
                self.router.collapse(id, false, None);
                retell.insert(id);
            }
        }
        for id in retell {
            if let Some(bar) = self.plugin_mut(id).and_then(SlotPlugin::bar_mut) {
                bar.visibility.row_kept();
            }
            self.router.redraw.insert(id);
        }
    }

    /// The bar to open the settings screen from, for a request `client`
    /// made: the first bar on the tab the client is looking at, else the
    /// first bar.
    fn settings_slot(&self, client: Option<ClientId>) -> Option<SlotId> {
        let bars: Vec<SlotId> = self
            .slots
            .iter()
            .filter(|(_, state)| state.kind == SlotKind::Pane && state.plugin.bar().is_some())
            .map(|(id, _)| *id)
            .collect();
        let shown = client
            .and_then(|client| self.active_tab(client))
            .and_then(|position| self.panes.as_ref()?.panes.get(&position))
            .and_then(|panes| {
                bars.iter().copied().find(|id| {
                    panes
                        .iter()
                        .any(|pane| pane.is_plugin && pane.id == *id && !pane.is_suppressed)
                })
            });
        shown.or_else(|| bars.first().copied())
    }

    /// The position of the tab `client` is looking at, from its last
    /// `TabUpdate`.
    fn active_tab(&self, client: ClientId) -> Option<usize> {
        match &self.clients.get(&client)?.get(&Seeded::Tabs)?.1 {
            Event::TabUpdate(tabs) => tabs.iter().find(|tab| tab.active).map(|tab| tab.position),
            _ => None,
        }
    }

    /// Hand the slots whose panes `manifest` lists the events they held
    /// while Zellij had no pane for them.
    fn release_held(&mut self, manifest: &PaneManifest) {
        let listed: Vec<SlotId> = self
            .router
            .held
            .keys()
            .copied()
            .filter(|id| {
                manifest
                    .panes
                    .values()
                    .flatten()
                    .any(|pane| pane.is_plugin && pane.id == *id)
            })
            .collect();
        for id in listed {
            for event in self.router.held.remove(&id).unwrap_or_default() {
                self.router.queued.push_back((id, event));
            }
        }
    }

    fn run_background(&mut self, callback: Callback) {
        let background: Vec<SlotId> = self
            .slots
            .iter()
            .filter(|(_, state)| state.kind == SlotKind::Background)
            .map(|(id, _)| *id)
            .collect();
        for id in background {
            self.deliver(id, None, callback, |plugin| {
                plugin.background();
                false
            });
        }
    }

    /// Deliver the events a slot's calls led to, collapse or expand the
    /// bars hidden or shown, then hand back what is left to do.
    fn finish(&mut self, callback: Callback) -> Outcome {
        self.deliver_queued(callback);
        self.follow_hidden(callback);
        self.outcome()
    }

    /// Collapse the pane of each bar hidden since `Shared` last looked, and
    /// expand the pane of each bar shown, at once rather than as it draws:
    /// Zellij draws neither a collapsed slot nor one on a tab nobody looks
    /// at. A bar shown says in its next render whether it keeps the row.
    fn follow_hidden(&mut self, callback: Callback) {
        for (&id, state) in self.slots.iter_mut() {
            if state.kind != SlotKind::Pane {
                continue;
            }
            let Some(bar) = state.plugin.bar_mut() else {
                continue;
            };
            let hidden = bar.visibility.is_hidden();
            if hidden == state.hidden {
                continue;
            }
            state.hidden = hidden;
            if let Some(collapsed) = bar.visibility.row_after_hiding() {
                self.router.collapse(id, collapsed, callback.drawn());
            }
        }
    }

    fn deliver_queued(&mut self, callback: Callback) {
        while let Some((id, event)) = self.router.queued.pop_front() {
            self.update_slot(id, None, callback, event);
        }
    }

    /// What is left to do. Only pane slots are drawn.
    fn outcome(&mut self) -> Outcome {
        let redraw: Vec<SlotId> = std::mem::take(&mut self.router.redraw)
            .into_iter()
            .filter(|id| {
                self.slots
                    .get(id)
                    .is_some_and(|state| state.kind == SlotKind::Pane)
            })
            .collect();
        Outcome {
            effects: std::mem::take(&mut self.router.effects),
            render: if redraw.is_empty() {
                RenderResponse::Nothing
            } else {
                RenderResponse::Slots(redraw)
            },
        }
    }
}

impl Router {
    /// Make a call slot `id` handed over, now or in the slot's next render.
    fn route(&mut self, id: SlotId, kind: SlotKind, callback: Callback, command: Command) {
        let current = callback.slot == Some(id);
        let drawing = callback.drawn();
        match command {
            Command::SetSelectable(selectable) => {
                self.perform(id, Effect::SetSelectable(id, selectable), drawing);
            }
            Command::SetCollapsed(collapsed) => self.collapse(id, collapsed, drawing),
            Command::ShowSelf => self.perform(id, Effect::Show(id), drawing),
            Command::CloseSelf => self.perform(id, Effect::Close(id), drawing),
            Command::SetTimeout(seconds) => {
                self.timers.set((self.clock)(), seconds, id);
                self.perform(id, Effect::SetTimeout(seconds), drawing);
            }
            Command::ExpectTimer(seconds) => self.timers.set((self.clock)(), seconds, id),
            Command::Prompted(request_id) => {
                self.prompts.insert(request_id, id);
            }
            // Zellij shows the prompt in the current slot's pane, or the
            // first pane's when there is none. A slot being added has no
            // pane yet, and a prompt for a pane Zellij cannot find is lost.
            // So a request made away from the slot's own callback, or as it
            // is added, waits for the render of a slot waiting on it: the
            // slot that asked may never be drawn, and every slot asking for
            // no more after it waits on the same answer. A slot that needs
            // nothing more would take the prompt from the one that asked,
            // such as a settings screen.
            //
            // A background slot is never drawn, so its request is made at
            // once. Zellij keeps a hidden floating pane for it, and shows
            // that to prompt in.
            Command::RequestPermission(wanted) => match self.permissions.ask(id, &wanted) {
                Ask::Granted => {
                    let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
                    // The slot reads its own pane once granted, which Zellij
                    // has not created while it adds the slot.
                    if callback.adding && kind == SlotKind::Pane {
                        self.held.entry(id).or_default().push(granted);
                        self.redraw.insert(id);
                    } else {
                        self.queued.push_back((id, granted));
                    }
                }
                Ask::Wait => {}
                Ask::Request(_) => {
                    if (current && !callback.adding) || kind == SlotKind::Background {
                        self.ask_zellij(id, callback.slot, drawing);
                    } else {
                        self.permissions.unsent = true;
                        self.redraw.insert(id);
                    }
                }
            },
            Command::SetPaneFrameItems(pane_id, items) => {
                let now = current && callback.self_commands;
                self.anchored(
                    id,
                    kind,
                    now.then_some(drawing),
                    Effect::SetPaneFrameItems(pane_id, items),
                );
            }
            Command::OpenContextMenu {
                target,
                line,
                column,
            } => {
                let effect = Effect::OpenContextMenu {
                    target,
                    line,
                    column,
                };
                self.anchored(id, kind, current.then_some(drawing), effect);
            }
            Command::OpenPopup(popup) => self.anchored(
                id,
                kind,
                current.then_some(drawing),
                Effect::OpenPopup(popup),
            ),
        }
    }

    /// Make a call Zellij takes for the current slot alone: now, when `now`
    /// holds the slot Zellij draws or adds, if any, else in slot `id`'s next
    /// render. A background slot is never drawn, so its call is dropped.
    fn anchored(
        &mut self,
        id: SlotId,
        kind: SlotKind,
        now: Option<Option<SlotId>>,
        effect: Effect,
    ) {
        if let Some(drawing) = now {
            self.perform(id, effect, drawing);
            return;
        }
        if kind == SlotKind::Background {
            return;
        }
        let deferred = self.deferred.entry(id).or_default();
        deferred.retain(|earlier| !effect.replaces(earlier));
        deferred.push(effect);
        self.redraw.insert(id);
    }

    /// Collapse or expand slot `id`'s pane, while Zellij draws or adds slot
    /// `drawing`, if any.
    ///
    /// Collapsed, the pane would hide a prompt Zellij shows in it, and the
    /// slot would wait on an answer nobody can give, so a collapse waits
    /// for the answer. It is not made then, as Zellij does not draw a
    /// collapsed slot, and one that waited on a grant before it had
    /// anything to draw could never expand again. The bar says again
    /// instead, in the render the answer leads to (see `retell`).
    fn collapse(&mut self, id: SlotId, collapsed: bool, drawing: Option<SlotId>) {
        if collapsed && self.permissions.prompting(id) {
            self.retell.insert(id);
            return;
        }
        self.retell.remove(&id);
        if collapsed {
            self.collapsed.insert(id);
        } else {
            self.collapsed.remove(&id);
        }
        self.perform(id, Effect::SetCollapsed(id, collapsed), drawing);
    }

    /// Ask Zellij for the pending permissions on slot `id`'s behalf, while
    /// Zellij counts `shown_in` as the current slot, in whose pane it shows
    /// the prompt. That pane is expanded first if collapsed, and stays so
    /// while the prompt is up.
    fn ask_zellij(&mut self, id: SlotId, shown_in: Option<SlotId>, drawing: Option<SlotId>) {
        let Some(all) = self.permissions.pending.clone() else {
            return;
        };
        self.permissions.unsent = false;
        self.permissions.prompter = shown_in;
        if let Some(slot) = shown_in.filter(|slot| self.collapsed.remove(slot)) {
            self.retell.insert(slot);
            self.perform(slot, Effect::SetCollapsed(slot, false), drawing);
        }
        self.perform(id, Effect::RequestPermission(all), drawing);
    }

    /// See `frame_owner`.
    fn frame_owner(&self, pane: PaneId) -> Option<SlotId> {
        self.frame_owners.get(&pane).copied().or_else(|| {
            self.deferred.iter().find_map(|(id, effects)| {
                effects
                    .iter()
                    .any(|effect| {
                        matches!(effect, Effect::SetPaneFrameItems(placed, items)
                            if *placed == pane && !items.is_empty())
                    })
                    .then_some(*id)
            })
        })
    }

    /// Make a call for slot `id`, noting which slot placed frame items.
    /// `drawing` is the slot Zellij draws or adds, if any.
    fn perform(&mut self, id: SlotId, effect: Effect, drawing: Option<SlotId>) {
        if let Effect::SetPaneFrameItems(pane_id, items) = &effect {
            if !items.is_empty() {
                self.frame_owners.insert(*pane_id, id);
            } else if self.frame_owners.get(pane_id) == Some(&id) {
                self.frame_owners.remove(pane_id);
            }
        }
        match self.performer {
            Some(perform) => perform(effect, drawing),
            None => self.effects.push(effect),
        }
    }
}

/// The slot `context` was marked with by `tag_context`, and `context` without
/// the mark.
fn untag_context(
    mut context: BTreeMap<String, String>,
) -> (Option<SlotId>, BTreeMap<String, String>) {
    let slot = context
        .remove(SLOT_CONTEXT_KEY)
        .and_then(|slot| slot.parse().ok());
    (slot, context)
}

/// The calls into Zellij, around the plain methods above, which make each
/// call into Zellij as it comes (see `Shared::live`).
impl<P: SlotPlugin> ZellijSharedPlugin for Shared<P> {
    /// Each slot loads with its own configuration in `slot_added`.
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        self.live();
    }

    /// Zellij draws the new slot whatever it asks; the other slots to draw
    /// again wait for the next answer Zellij takes.
    fn slot_added(&mut self, slot: Slot) {
        self.live();
        let id = slot.id;
        let render = self.add_slot(slot).render;
        self.keep_redraw(render, Some(id));
    }

    fn slot_removed(&mut self, slot_id: SlotId) {
        self.live();
        let render = self.remove_slot(slot_id).render;
        self.keep_redraw(render, None);
    }

    fn update(&mut self, event: Event, context: EventContext) -> RenderResponse {
        self.live();
        self.take_event(event, context).render
    }

    fn pipe(&mut self, pipe_message: PipeMessage, context: EventContext) -> RenderResponse {
        self.live();
        self.take_message(pipe_message, context).render
    }

    /// Draw the slot, then flush what it printed: text left in the buffer of
    /// standard output would otherwise come out ahead of the next call into
    /// Zellij, which would then take it for part of the call.
    fn render(&mut self, rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {
        self.live();
        self.render_slot(rows, cols, slot_id, client_id);
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Call, Host, Recorder};
    use crate::panes::own_pane_url;
    use crate::visibility::Request;
    use std::cell::Cell;
    use std::collections::HashMap;

    /// A call a `Fake` makes through its host the next time it runs.
    #[derive(Clone, Debug)]
    enum Act {
        Selectable(bool),
        Show,
        Close,
        Collapse(bool),
        Timeout(f64),
        Ask(Vec<PermissionType>),
        FrameItems(PaneId, Vec<FrameItem>),
        Menu,
        Popup,
        Prompt,
        RunCommand,
        /// Note `frame_owner` of the pane in `owners`.
        Owner(PaneId),
        /// Say what the bar's row should be, as a bar with
        /// `collapse_when_empty` on does as it draws, with nothing to draw
        /// when true.
        Row(bool),
        /// Hide or show the bar, as a message asks.
        Visibility(Request),
    }

    /// A plugin that keeps what it is given and makes the calls it is told
    /// to. Loaded with `bar` in its configuration, it runs as a bar, which
    /// learns its URL from `PaneUpdate`. With `ask`, it asks for
    /// `permissions(ask)` as it loads.
    #[derive(Default)]
    struct Fake {
        host: Recorder,
        bar: Option<BarCore>,
        events: Vec<Event>,
        messages: Vec<String>,
        renders: usize,
        backgrounds: usize,
        /// What `frame_owner` said, for each `Act::Owner`.
        owners: Vec<Option<SlotId>>,
        next: Vec<Act>,
    }

    impl Fake {
        fn act(&mut self) {
            for act in std::mem::take(&mut self.next) {
                let host = &self.host;
                match act {
                    Act::Selectable(selectable) => host.set_selectable(selectable),
                    Act::Show => host.show_self(),
                    Act::Close => host.close_self(),
                    Act::Collapse(collapsed) => host.set_collapsed(collapsed),
                    Act::Timeout(seconds) => host.set_timeout(seconds),
                    Act::Ask(permissions) => host.request_permission(&permissions),
                    Act::FrameItems(pane_id, items) => host.set_pane_frame_items(pane_id, items),
                    Act::Menu => host.open_context_menu(ContextMenuTarget::Bar, 0, 1),
                    Act::Popup => host.open_preview_popup(
                        &crate::popup_names!("zjtest"),
                        "zjtest",
                        "tag",
                        &["entry".to_string()],
                        0,
                        1,
                    ),
                    Act::Prompt => {
                        host.prompt(PromptRequest::menu([ChoiceItem::labeled("0", "Sure")]));
                    }
                    Act::RunCommand => host.run_command(&["true"], BTreeMap::new()),
                    Act::Owner(pane) => self.owners.push(frame_owner(pane)),
                    Act::Row(empty) => {
                        if let Some(bar) = self.bar.as_mut() {
                            bar.sync_collapsed(host, true, empty);
                        }
                    }
                    Act::Visibility(request) => {
                        if let Some(bar) = self.bar.as_mut() {
                            bar.change_visibility(request, None);
                        }
                    }
                }
            }
        }

        /// The kinds of the events it was given, in order.
        fn kinds(&self) -> Vec<EventType> {
            self.events.iter().map(EventType::from).collect()
        }
    }

    impl ZellijPlugin for Fake {
        fn load(&mut self, configuration: BTreeMap<String, String>) {
            if configuration.contains_key("bar") {
                self.bar = Some(BarCore {
                    plugin_id: Some(self.host.plugin_ids().plugin_id),
                    ..Default::default()
                });
            }
            if let Some(ask) = configuration.get("ask") {
                self.host.request_permission(&permissions(ask));
            }
            self.act();
        }

        fn update(&mut self, event: Event) -> bool {
            if let (Event::PaneUpdate(manifest), Some(bar)) = (&event, &mut self.bar) {
                if let Some(url) = own_pane_url(manifest, bar.plugin_id) {
                    bar.own_url = Some(url);
                }
            }
            self.events.push(event);
            self.act();
            log("updated".to_string());
            true
        }

        fn pipe(&mut self, message: PipeMessage) -> bool {
            self.messages.push(message.name);
            self.act();
            true
        }

        fn render(&mut self, _rows: usize, _cols: usize) {
            self.renders += 1;
            self.act();
            // Where a plugin prints what it draws.
            log("drawn".to_string());
        }
    }

    impl SlotPlugin for Fake {
        fn bar(&self) -> Option<&BarCore> {
            self.bar.as_ref()
        }

        fn bar_mut(&mut self) -> Option<&mut BarCore> {
            self.bar.as_mut()
        }

        fn background(&mut self) {
            self.backgrounds += 1;
        }
    }

    /// `r` for `ReadApplicationState`, `o` for `OpenTerminalsOrPlugins`,
    /// `c` for `ChangeApplicationState`.
    fn permissions(letters: &str) -> Vec<PermissionType> {
        letters
            .chars()
            .map(|letter| match letter {
                'r' => PermissionType::ReadApplicationState,
                'o' => PermissionType::OpenTerminalsOrPlugins,
                _ => PermissionType::ChangeApplicationState,
            })
            .collect()
    }

    thread_local! {
        /// What `fake_clock` reports, in milliseconds.
        static NOW: Cell<u64> = const { Cell::new(0) };
    }

    fn fake_clock() -> Duration {
        Duration::from_millis(NOW.with(Cell::get))
    }

    thread_local! {
        /// The calls `logged` made and where each `Fake` drew or took an
        /// event, in order.
        static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    fn log(entry: String) {
        LOG.with(|log| log.borrow_mut().push(entry));
    }

    /// A performer that writes each call to `LOG` as it is made, with the
    /// slot Zellij draws.
    fn logged(effect: Effect, drawing: Option<SlotId>) {
        log(format!("{effect:?} drawing {drawing:?}"));
    }

    /// `shared` with each call made as it comes, through `logged`.
    fn logging(ids: &[SlotId]) -> Shared<Fake> {
        let mut shared = with_panes(ids);
        shared.router.performer = Some(logged);
        LOG.with(|log| log.borrow_mut().clear());
        shared
    }

    fn logged_so_far() -> Vec<String> {
        LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
    }

    fn shared() -> Shared<Fake> {
        NOW.with(|now| now.set(0));
        Shared {
            router: Router {
                clock: fake_clock,
                ..Router::default()
            },
            ..Shared::default()
        }
    }

    fn slot(id: SlotId, kind: SlotKind, pairs: &[(&str, &str)]) -> Slot {
        Slot {
            id,
            kind,
            configuration: pairs
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    /// `shared` with a pane slot added for each of `ids`, which made no
    /// calls.
    fn with_panes(ids: &[SlotId]) -> Shared<Fake> {
        let mut shared = shared();
        for id in ids {
            shared.add_slot(slot(*id, SlotKind::Pane, &[]));
        }
        shared
    }

    fn context(slot_id: Option<SlotId>, client_id: Option<ClientId>) -> EventContext {
        EventContext { slot_id, client_id }
    }

    fn mode(session: &str) -> Event {
        Event::ModeUpdate(ModeInfo {
            session_name: Some(session.to_string()),
            ..Default::default()
        })
    }

    /// The session names of the mode updates `id` was given, in order.
    fn modes(shared: &Shared<Fake>, id: SlotId) -> Vec<String> {
        shared
            .plugin(id)
            .unwrap()
            .events
            .iter()
            .filter_map(|event| match event {
                Event::ModeUpdate(mode_info) => mode_info.session_name.clone(),
                _ => None,
            })
            .collect()
    }

    fn tabs(active: usize) -> Event {
        Event::TabUpdate(
            (0..2)
                .map(|position| TabInfo {
                    position,
                    active: position == active,
                    ..Default::default()
                })
                .collect(),
        )
    }

    fn plugin_pane(id: u32, url: &str) -> PaneInfo {
        PaneInfo {
            id,
            is_plugin: true,
            plugin_url: Some(url.to_string()),
            ..Default::default()
        }
    }

    /// A `PaneUpdate` with the panes of each tab, by position.
    fn panes(tabs: Vec<Vec<PaneInfo>>) -> Event {
        Event::PaneUpdate(PaneManifest {
            panes: tabs.into_iter().enumerate().collect::<HashMap<_, _>>(),
        })
    }

    fn act(shared: &mut Shared<Fake>, id: SlotId, acts: &[Act]) {
        shared.plugin_mut(id).unwrap().next = acts.to_vec();
    }

    fn broadcast(name: &str) -> PipeMessage {
        PipeMessage::new(PipeSource::Plugin(7), name, &None, &None, false)
    }

    fn frame_items() -> Vec<FrameItem> {
        vec![FrameItem::new("item", "x").clickable()]
    }

    fn click(pane_id: PaneId) -> Event {
        Event::FrameItemClicked(FrameItemClick {
            pane_id,
            item_id: "item".to_string(),
            button: MouseButton::Left,
            modifiers: BTreeSet::new(),
        })
    }

    #[test]
    fn a_new_slot_loads_its_own_configuration_and_is_given_what_the_instance_knows() {
        let mut shared = with_panes(&[10]);
        shared.take_event(mode("one"), context(None, Some(1)));
        shared.take_event(tabs(0), context(None, Some(1)));
        shared.take_event(
            panes(vec![vec![plugin_pane(11, "zjtest")]]),
            context(None, None),
        );

        shared.add_slot(slot(11, SlotKind::Pane, &[("bar", "")]));
        let added = shared.plugin(11).unwrap();
        assert_eq!(
            added.kinds(),
            vec![
                EventType::ModeUpdate,
                EventType::TabUpdate,
                EventType::PaneUpdate
            ]
        );
        assert_eq!(added.bar.as_ref().unwrap().plugin_id, Some(11));
        assert_eq!(
            added.bar.as_ref().unwrap().own_url.as_deref(),
            Some("zjtest")
        );
        // The slot already there is left alone.
        assert_eq!(shared.plugin(10).unwrap().events.len(), 3);
    }

    #[test]
    fn a_slot_is_given_the_mode_of_the_client_it_draws_for_before_drawing() {
        let mut shared = with_panes(&[10, 11]);
        shared.take_event(mode("one"), context(None, Some(1)));
        shared.take_event(mode("two"), context(None, Some(2)));
        assert_eq!(modes(&shared, 10), ["one", "two"]);

        shared.render_slot(1, 80, 10, 1);
        assert_eq!(modes(&shared, 10), ["one", "two", "one"]);
        assert_eq!(shared.plugin(10).unwrap().renders, 1);
        // Seen already, so not given again.
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(modes(&shared, 10), ["one", "two", "one"]);
        // Drawing for the client it last saw changes nothing.
        shared.render_slot(1, 80, 11, 2);
        assert_eq!(modes(&shared, 11), ["one", "two"]);
    }

    #[test]
    fn a_click_is_taken_with_the_mode_and_tabs_of_the_client_that_clicked() {
        let mut shared = with_panes(&[10]);
        shared.take_event(tabs(0), context(None, Some(1)));
        shared.take_event(tabs(1), context(None, Some(2)));
        let mouse = Event::Mouse(Mouse::LeftClick(0, 1));
        shared.take_event(mouse.clone(), context(Some(10), Some(1)));
        let events = &shared.plugin(10).unwrap().events;
        assert_eq!(&events[2..], [tabs(0), mouse]);
    }

    #[test]
    fn an_event_about_a_slot_goes_to_that_slot_alone() {
        let mut shared = with_panes(&[10, 11]);
        let outcome = shared.take_event(Event::Visible(true), context(Some(11), Some(1)));
        assert!(shared.plugin(10).unwrap().events.is_empty());
        assert_eq!(shared.plugin(11).unwrap().events, [Event::Visible(true)]);
        assert_eq!(outcome.render, RenderResponse::Slots(vec![11]));
        // One about no slot goes to every slot.
        let outcome = shared.take_event(
            Event::CwdChanged(PaneId::Terminal(1), "/".into(), vec![]),
            context(None, None),
        );
        assert_eq!(outcome.render, RenderResponse::Slots(vec![10, 11]));
    }

    #[test]
    fn each_timer_goes_to_the_slot_whose_timer_is_due_first() {
        let mut shared = with_panes(&[10, 11, 12]);
        act(&mut shared, 10, &[Act::Timeout(3.0)]);
        let outcome = shared.take_event(Event::Visible(true), context(Some(10), None));
        assert_eq!(outcome.effects, [Effect::SetTimeout(3.0)]);
        NOW.with(|now| now.set(1000));
        act(&mut shared, 11, &[Act::Timeout(1.0)]);
        act(&mut shared, 12, &[Act::Timeout(1.0)]);
        shared.take_event(Event::Visible(true), context(Some(11), None));
        shared.take_event(Event::Visible(true), context(Some(12), None));

        let timers_of = |shared: &Shared<Fake>| -> Vec<usize> {
            [10, 11, 12]
                .iter()
                .map(|id| {
                    let plugin = shared.plugin(*id).unwrap();
                    plugin
                        .kinds()
                        .iter()
                        .filter(|kind| **kind == EventType::Timer)
                        .count()
                })
                .collect()
        };
        let timer = || Event::Timer(1.0);
        // Due at 2s, both: the one set first fires first.
        shared.take_event(timer(), context(None, None));
        assert_eq!(timers_of(&shared), [0, 1, 0]);
        shared.take_event(timer(), context(None, None));
        assert_eq!(timers_of(&shared), [0, 1, 1]);
        shared.take_event(timer(), context(None, None));
        assert_eq!(timers_of(&shared), [1, 1, 1]);
        // A timer nobody is waiting for goes to nobody.
        shared.take_event(timer(), context(None, None));
        assert_eq!(timers_of(&shared), [1, 1, 1]);
    }

    #[test]
    fn a_timer_a_button_set_by_itself_still_counts_as_the_slot_s() {
        let mut shared = with_panes(&[10, 11]);
        shared.deliver(10, None, Callback::NONE, |_| {
            expect_timer(0.4);
            false
        });
        shared.take_event(Event::Timer(0.4), context(None, None));
        assert_eq!(shared.plugin(10).unwrap().kinds(), [EventType::Timer]);
        assert!(shared.plugin(11).unwrap().events.is_empty());
    }

    #[test]
    fn calls_about_the_plugin_s_own_pane_are_made_for_the_acting_slot() {
        let mut shared = with_panes(&[10, 11]);
        act(
            &mut shared,
            11,
            &[
                Act::Selectable(false),
                Act::Collapse(true),
                Act::Show,
                Act::Close,
            ],
        );
        let outcome = shared.take_message(broadcast("anything"), context(None, None));
        assert_eq!(
            outcome.effects,
            [
                Effect::SetSelectable(11, false),
                Effect::SetCollapsed(11, true),
                Effect::Show(11),
                Effect::Close(11),
            ]
        );
        // The recorder saw them made by slot 11.
        let calls = shared.plugin(11).unwrap().host.slot_calls();
        assert_eq!(calls[0], (Some(11), Call::SetSelectable(false)));
    }

    #[test]
    fn frame_items_placed_outside_the_slot_s_render_wait_for_it() {
        let mut shared = with_panes(&[10]);
        let pane = PaneId::Terminal(3);
        act(
            &mut shared,
            10,
            &[
                Act::FrameItems(pane, vec![FrameItem::new("old", "o")]),
                Act::FrameItems(pane, frame_items()),
            ],
        );
        let outcome = shared.take_event(Event::Visible(true), context(Some(10), None));
        assert_eq!(outcome.effects, []);
        assert_eq!(outcome.render, RenderResponse::Slots(vec![10]));

        // Only the newer items for the pane are placed.
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            outcome.effects,
            [Effect::SetPaneFrameItems(pane, frame_items())]
        );
        // In its render, they are placed at once.
        act(&mut shared, 10, &[Act::FrameItems(pane, vec![])]);
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(outcome.effects, [Effect::SetPaneFrameItems(pane, vec![])]);
    }

    #[test]
    fn a_frame_item_click_goes_to_the_slot_that_placed_the_items() {
        let mut shared = with_panes(&[10, 11]);
        let pane = PaneId::Terminal(3);
        act(&mut shared, 11, &[Act::FrameItems(pane, frame_items())]);
        shared.render_slot(1, 80, 11, 1);

        shared.take_event(click(pane), context(None, Some(1)));
        assert!(shared.plugin(10).unwrap().events.is_empty());
        assert_eq!(shared.plugin(11).unwrap().events, [click(pane)]);
        // A click on a pane nobody placed items on goes to nobody.
        shared.take_event(click(PaneId::Terminal(4)), context(None, Some(1)));
        assert_eq!(shared.plugin(11).unwrap().events.len(), 1);

        // Taking its items off gives the pane up.
        act(&mut shared, 11, &[Act::FrameItems(pane, vec![])]);
        shared.render_slot(1, 80, 11, 1);
        shared.take_event(click(pane), context(None, Some(1)));
        assert_eq!(shared.plugin(11).unwrap().events.len(), 1);
    }

    #[test]
    fn a_pane_s_frame_owner_is_the_slot_whose_items_are_on_it_or_wait_to_go_there() {
        let mut shared = with_panes(&[10, 11]);
        let pane = PaneId::Terminal(3);
        let owners = |shared: &mut Shared<Fake>, id: SlotId| {
            act(shared, id, &[Act::Owner(pane)]);
            shared.take_event(Event::Visible(true), context(Some(id), None));
            shared.plugin_mut(id).unwrap().owners.pop().unwrap()
        };
        assert_eq!(owners(&mut shared, 10), None);
        // Waiting for the slot's render, the items already make it the
        // owner, so no other slot places its own beside them.
        act(&mut shared, 11, &[Act::FrameItems(pane, frame_items())]);
        shared.take_event(Event::Visible(true), context(Some(11), None));
        assert_eq!(owners(&mut shared, 10), Some(11));
        // Taken off before they were placed, the pane is given up at once.
        act(&mut shared, 11, &[Act::FrameItems(pane, vec![])]);
        shared.take_event(Event::Visible(true), context(Some(11), None));
        assert_eq!(owners(&mut shared, 10), None);

        act(&mut shared, 11, &[Act::FrameItems(pane, frame_items())]);
        shared.render_slot(1, 80, 11, 1);
        assert_eq!(owners(&mut shared, 10), Some(11));
        // Once placed, the items stay until the slot's render takes them
        // off.
        act(&mut shared, 11, &[Act::FrameItems(pane, vec![])]);
        shared.take_event(Event::Visible(true), context(Some(11), None));
        assert_eq!(owners(&mut shared, 10), Some(11));
        shared.render_slot(1, 80, 11, 1);
        assert_eq!(owners(&mut shared, 10), None);

        // Zellij drops a removed slot's items, so its panes are free.
        act(&mut shared, 11, &[Act::FrameItems(pane, frame_items())]);
        shared.render_slot(1, 80, 11, 1);
        shared.remove_slot(11);
        assert_eq!(owners(&mut shared, 10), None);
        // Outside `Shared`, no slot owns anything.
        assert_eq!(frame_owner(pane), None);
    }

    #[test]
    fn a_menu_or_popup_opens_at_once_from_an_event_about_the_slot_and_waits_otherwise() {
        let mut shared = with_panes(&[10]);
        act(&mut shared, 10, &[Act::Menu]);
        let mouse = Event::Mouse(Mouse::LeftClick(0, 1));
        let outcome = shared.take_event(mouse, context(Some(10), Some(1)));
        assert!(matches!(
            outcome.effects[..],
            [Effect::OpenContextMenu { .. }]
        ));

        act(&mut shared, 10, &[Act::Popup]);
        let outcome = shared.take_message(broadcast("anything"), context(None, None));
        assert_eq!(outcome.effects, []);
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert!(matches!(outcome.effects[..], [Effect::OpenPopup(_)]));
    }

    #[test]
    fn a_background_slot_is_never_drawn_and_runs_its_background_work_after_each_event() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Background, &[]));
        shared.add_slot(slot(11, SlotKind::Pane, &[]));
        act(
            &mut shared,
            10,
            &[
                Act::FrameItems(PaneId::Terminal(3), frame_items()),
                Act::Menu,
            ],
        );
        let outcome = shared.take_event(Event::Visible(true), context(None, None));
        // Calls that wait for a render are dropped, and only the pane is
        // drawn again.
        assert_eq!(outcome.effects, []);
        assert_eq!(outcome.render, RenderResponse::Slots(vec![11]));
        assert_eq!(shared.plugin(10).unwrap().backgrounds, 1);
        assert_eq!(shared.plugin(11).unwrap().backgrounds, 0);
        shared.take_message(broadcast("anything"), context(None, None));
        assert_eq!(shared.plugin(10).unwrap().backgrounds, 2);
    }

    #[test]
    fn permissions_are_asked_for_once_and_every_waiting_slot_gets_the_answer() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            outcome.effects,
            [Effect::RequestPermission(permissions("r"))]
        );
        // Asked for already: the slot waits for the same answer.
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "r")]));
        let outcome = shared.render_slot(1, 80, 11, 1);
        assert_eq!(outcome.effects, []);

        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        let outcome = shared.take_event(granted.clone(), context(Some(10), Some(1)));
        assert_eq!(outcome.render, RenderResponse::Slots(vec![10, 11]));
        for id in [10, 11] {
            assert_eq!(shared.plugin(id).unwrap().events, vec![granted.clone()]);
        }

        // A slot added later that needs no more is told it is granted when
        // it first draws.
        let outcome = shared.add_slot(slot(12, SlotKind::Pane, &[("ask", "r")]));
        assert_eq!(outcome.effects, []);
        shared.render_slot(1, 80, 12, 1);
        assert_eq!(shared.plugin(12).unwrap().events, vec![granted.clone()]);

        // One that needs more asks for everything.
        shared.add_slot(slot(13, SlotKind::Pane, &[("ask", "c")]));
        let outcome = shared.render_slot(1, 80, 13, 1);
        assert_eq!(
            outcome.effects,
            [Effect::RequestPermission(permissions("rc"))]
        );
    }

    #[test]
    fn a_permission_request_made_as_a_slot_loads_waits_for_its_pane() {
        let mut shared = shared();
        // Zellij has no pane for the slot while it is added, and loses a
        // prompt for a pane it cannot find.
        let outcome = shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        assert_eq!(outcome.effects, []);
        assert_eq!(outcome.render, RenderResponse::Slots(vec![10]));
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            outcome.effects,
            [Effect::RequestPermission(permissions("r"))]
        );
        // Asked once: a later render does not ask again.
        assert_eq!(shared.render_slot(1, 80, 10, 1).effects, []);
    }

    #[test]
    fn a_grant_for_a_slot_being_added_waits_for_its_pane() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.take_event(granted.clone(), context(Some(10), Some(1)));

        // Granted already, but the slot reads its own pane when told so, and
        // Zellij has none for it until it first draws the slot.
        let outcome = shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "r")]));
        assert_eq!(outcome.render, RenderResponse::Slots(vec![11]));
        assert_eq!(shared.plugin(11).unwrap().events, []);
        shared.render_slot(1, 80, 11, 1);
        assert_eq!(shared.plugin(11).unwrap().events, [granted.clone()]);

        // A background slot has no pane to wait for.
        shared.add_slot(slot(12, SlotKind::Background, &[("ask", "r")]));
        assert_eq!(shared.plugin(12).unwrap().events, [granted]);
    }

    #[test]
    fn a_bar_that_collapses_as_it_asks_keeps_its_row_while_the_prompt_is_up() {
        let mut shared = logging(&[]);
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        // With nothing to show yet, the bar collapses in the render that
        // asks. Collapsed, its pane would hide the prompt, which nobody could
        // answer, and every event Zellij sends would be denied for good.
        act(&mut shared, 10, &[Act::Collapse(true)]);
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            logged_so_far(),
            [
                format!(
                    "{:?} drawing Some(10)",
                    Effect::RequestPermission(permissions("r"))
                ),
                "drawn".to_string(),
            ]
        );
        // Answered, the pane stays: collapsed, Zellij would not draw the bar
        // again once the events it was denied start to come.
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.take_event(granted.clone(), context(None, Some(1)));
        assert_eq!(logged_so_far(), ["updated"]);
        assert_eq!(shared.plugin(10).unwrap().events, [granted]);
        // With no prompt up, it collapses at once.
        act(
            &mut shared,
            10,
            &[Act::Collapse(false), Act::Collapse(true)],
        );
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            logged_so_far(),
            [
                "SetCollapsed(10, false) drawing Some(10)",
                "SetCollapsed(10, true) drawing Some(10)",
                "drawn",
            ]
        );
    }

    #[test]
    fn a_bar_kept_from_its_row_by_a_prompt_says_again_once_answered() {
        let mut shared = logging(&[]);
        let granted = || Event::PermissionRequestResult(PermissionStatus::Granted);
        // Empty as it asks, the bar collapses in the render that shows the
        // prompt, which waits for the answer.
        shared.add_slot(slot(10, SlotKind::Pane, &[("bar", ""), ("ask", "r")]));
        act(&mut shared, 10, &[Act::Row(true)]);
        shared.render_slot(1, 80, 10, 1);
        logged_so_far();
        let outcome = shared.take_event(granted(), context(None, Some(1)));
        assert_eq!(outcome.render, RenderResponse::Slots(vec![10]));
        assert_eq!(logged_so_far(), ["updated"]);
        // Still empty, it collapses in the render the answer leads to.
        act(&mut shared, 10, &[Act::Row(true)]);
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            logged_so_far(),
            ["SetCollapsed(10, true) drawing Some(10)", "drawn"]
        );

        // A collapsed bar expanded to show a prompt collapses again too.
        act(
            &mut shared,
            10,
            &[Act::Ask(permissions("rc")), Act::Row(true)],
        );
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            logged_so_far(),
            [
                "SetCollapsed(10, false) drawing Some(10)".to_string(),
                format!(
                    "{:?} drawing Some(10)",
                    Effect::RequestPermission(permissions("rc"))
                ),
                "drawn".to_string(),
            ]
        );
        shared.take_event(granted(), context(None, Some(1)));
        act(&mut shared, 10, &[Act::Row(true)]);
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            logged_so_far(),
            [
                "updated",
                "SetCollapsed(10, true) drawing Some(10)",
                "drawn"
            ]
        );

        // A bar waiting on another's prompt, empty for want of permissions,
        // collapses, and is expanded once answered, as Zellij would not draw
        // it again to let it expand by itself.
        shared.add_slot(slot(11, SlotKind::Pane, &[("bar", ""), ("ask", "rco")]));
        shared.add_slot(slot(12, SlotKind::Pane, &[("bar", ""), ("ask", "rco")]));
        shared.render_slot(1, 80, 12, 1);
        act(&mut shared, 11, &[Act::Row(true)]);
        shared.render_slot(1, 80, 11, 1);
        logged_so_far();
        shared.take_event(granted(), context(None, Some(1)));
        assert_eq!(
            logged_so_far(),
            ["SetCollapsed(11, false) drawing None", "updated", "updated"]
        );
        act(&mut shared, 11, &[Act::Row(false)]);
        shared.render_slot(1, 80, 11, 1);
        assert_eq!(logged_so_far(), ["drawn"]);
    }

    #[test]
    fn a_collapsed_slot_expands_to_show_its_prompt() {
        let mut shared = with_panes(&[10]);
        act(&mut shared, 10, &[Act::Collapse(true)]);
        shared.render_slot(1, 80, 10, 1);
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            outcome.effects,
            [
                Effect::SetCollapsed(10, false),
                Effect::RequestPermission(permissions("r")),
            ]
        );
        let denied = Event::PermissionRequestResult(PermissionStatus::Denied);
        let outcome = shared.take_event(denied, context(Some(10), Some(1)));
        assert_eq!(outcome.effects, []);
        // A slot that does not show the prompt collapses as before.
        act(&mut shared, 10, &[Act::Collapse(true)]);
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(outcome.effects, [Effect::SetCollapsed(10, true)]);
    }

    #[test]
    fn a_request_from_a_slot_that_is_never_drawn_goes_out_in_another_s_render() {
        let mut shared = shared();
        // Slot 10 is on a tab nobody looks at, so Zellij never draws it, and
        // slot 11 waits on the answer to the request slot 10 made.
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "r")]));
        let outcome = shared.render_slot(1, 80, 11, 1);
        assert_eq!(
            outcome.effects,
            [Effect::RequestPermission(permissions("r"))]
        );
        // Asked once only.
        assert_eq!(shared.render_slot(1, 80, 10, 1).effects, []);
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.take_event(granted.clone(), context(None, Some(1)));
        for id in [10, 11] {
            assert_eq!(
                shared.plugin(id).unwrap().events,
                std::slice::from_ref(&granted)
            );
        }
    }

    #[test]
    fn a_request_goes_out_only_in_the_render_of_a_slot_waiting_on_it() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.take_event(granted, context(None, Some(1)));
        // A settings screen asks for more as it loads. The bar needs nothing
        // more, so its prompt is not shown in the bar's pane.
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "rc")]));
        assert_eq!(shared.render_slot(1, 80, 10, 1).effects, []);
        assert_eq!(
            shared.render_slot(1, 80, 11, 1).effects,
            [Effect::RequestPermission(permissions("rc"))]
        );
    }

    #[test]
    fn a_grant_for_a_slot_being_added_is_told_once_zellij_lists_its_pane() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.take_event(granted.clone(), context(None, Some(1)));

        // Slot 11 is never drawn, but Zellij has its pane once it lists it.
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "r")]));
        let others = panes(vec![vec![plugin_pane(10, "file:/bar.wasm")]]);
        shared.take_event(others.clone(), context(None, None));
        assert!(!shared.plugin(11).unwrap().events.contains(&granted));
        let listed = panes(vec![vec![
            plugin_pane(10, "file:/bar.wasm"),
            plugin_pane(11, "file:/bar.wasm"),
        ]]);
        shared.take_event(listed.clone(), context(None, None));
        assert_eq!(
            shared.plugin(11).unwrap().events,
            [others, listed, granted.clone()]
        );
        // Told once: its first render does not tell it again.
        shared.render_slot(1, 80, 11, 1);
        let told = shared.plugin(11).unwrap().events.iter();
        assert_eq!(told.filter(|event| **event == granted).count(), 1);
    }

    #[test]
    fn a_request_nobody_waits_on_any_more_is_not_made() {
        let mut shared = with_panes(&[10]);
        // A settings screen that asks for more as it loads, closed before it
        // ever draws.
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "c")]));
        shared.remove_slot(11);
        assert_eq!(shared.render_slot(1, 80, 10, 1).effects, []);
        // Asked for later, it is asked for again.
        act(&mut shared, 10, &[Act::Ask(permissions("c"))]);
        assert_eq!(
            shared.render_slot(1, 80, 10, 1).effects,
            [Effect::RequestPermission(permissions("c"))]
        );
    }

    #[test]
    fn a_permission_request_made_away_from_the_slot_waits_for_its_render() {
        let mut shared = with_panes(&[10]);
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        let outcome = shared.take_message(broadcast("anything"), context(None, None));
        assert_eq!(outcome.effects, []);
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            outcome.effects,
            [Effect::RequestPermission(permissions("r"))]
        );
    }

    #[test]
    fn a_slot_may_ask_again_after_a_denial() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        let denied = Event::PermissionRequestResult(PermissionStatus::Denied);
        shared.take_event(denied.clone(), context(Some(10), Some(1)));
        assert_eq!(shared.plugin(10).unwrap().events, [denied]);
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            outcome.effects,
            [Effect::RequestPermission(permissions("r"))]
        );
    }

    #[test]
    fn a_request_for_more_waits_until_the_prompt_up_is_answered() {
        let mut shared = shared();
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        // A settings screen asks for more while the bar's prompt is up. A
        // second prompt out at once could be answered first and leave the
        // instance with less than either was told it has.
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "rc")]));
        assert_eq!(shared.render_slot(1, 80, 11, 1).effects, []);
        let outcome = shared.take_event(granted.clone(), context(Some(10), Some(1)));
        // The answer was to the bar's prompt alone.
        assert_eq!(
            shared.plugin(10).unwrap().events,
            std::slice::from_ref(&granted)
        );
        assert_eq!(shared.plugin(11).unwrap().events, []);
        assert_eq!(outcome.render, RenderResponse::Slots(vec![10, 11]));
        // The screen's request goes out next, in its own render.
        assert_eq!(shared.render_slot(1, 80, 10, 1).effects, []);
        assert_eq!(
            shared.render_slot(1, 80, 11, 1).effects,
            [Effect::RequestPermission(permissions("rc"))]
        );
        shared.take_event(granted.clone(), context(Some(11), Some(1)));
        assert_eq!(
            shared.plugin(11).unwrap().events,
            std::slice::from_ref(&granted)
        );
        assert_eq!(shared.plugin(10).unwrap().events, [granted]);
    }

    #[test]
    fn a_slot_that_asks_again_for_more_is_told_only_the_answer_to_that() {
        let mut shared = shared();
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        // Its options changed while the prompt was up. A bar takes a grant
        // for everything it asked for last.
        act(&mut shared, 10, &[Act::Ask(permissions("rc"))]);
        shared.take_event(Event::Visible(true), context(Some(10), None));
        shared.take_event(granted.clone(), context(Some(10), Some(1)));
        assert!(!shared.plugin(10).unwrap().events.contains(&granted));
        assert_eq!(
            shared.render_slot(1, 80, 10, 1).effects,
            [Effect::RequestPermission(permissions("rc"))]
        );
    }

    #[test]
    fn a_background_slot_s_prompt_is_the_one_a_bar_asking_the_same_waits_on() {
        let mut shared = shared();
        // Zellij prompts in the background slot's hidden pane.
        let outcome = shared.add_slot(slot(5, SlotKind::Background, &[("ask", "r")]));
        assert_eq!(
            outcome.effects,
            [Effect::RequestPermission(permissions("r"))]
        );
        // A bar asking for the same shows no second prompt.
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        assert_eq!(shared.render_slot(1, 80, 10, 1).effects, []);
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.take_event(granted.clone(), context(Some(5), Some(1)));
        for id in [5, 10] {
            assert!(shared.plugin(id).unwrap().events.contains(&granted));
        }
    }

    #[test]
    fn a_background_slot_s_request_left_by_a_closed_prompt_goes_out_in_a_bar_s_render() {
        let mut shared = with_panes(&[10]);
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 11, 1);
        shared.add_slot(slot(5, SlotKind::Background, &[("ask", "r")]));
        // The prompt goes with slot 11's pane, leaving the background slot
        // waiting, which is never drawn, so any bar's render asks again.
        shared.remove_slot(11);
        assert_eq!(
            shared.render_slot(1, 80, 10, 1).effects,
            [Effect::RequestPermission(permissions("r"))]
        );
        assert_eq!(shared.render_slot(1, 80, 10, 1).effects, []);
    }

    #[test]
    fn a_prompt_whose_pane_closes_is_asked_for_again() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        // The prompt was in slot 10's pane, which Zellij never answers once
        // closed.
        let outcome = shared.remove_slot(10);
        assert_eq!(outcome.render, RenderResponse::Slots(vec![11]));
        assert_eq!(
            shared.render_slot(1, 80, 11, 1).effects,
            [Effect::RequestPermission(permissions("r"))]
        );

        // A settings screen closed with its prompt up, and opened again.
        let mut shared = with_panes(&[10]);
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "rc")]));
        shared.render_slot(1, 80, 11, 1);
        shared.remove_slot(11);
        shared.add_slot(slot(12, SlotKind::Pane, &[("ask", "rc")]));
        assert_eq!(
            shared.render_slot(1, 80, 12, 1).effects,
            [Effect::RequestPermission(permissions("rc"))]
        );
    }

    #[test]
    fn a_denial_takes_away_every_slot_s_grant() {
        let mut shared = shared();
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        let denied = Event::PermissionRequestResult(PermissionStatus::Denied);
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        shared.render_slot(1, 80, 10, 1);
        shared.take_event(granted.clone(), context(Some(10), Some(1)));
        // Zellij leaves the instance with nothing once a prompt for more is
        // denied.
        shared.add_slot(slot(11, SlotKind::Pane, &[("ask", "rc")]));
        shared.render_slot(1, 80, 11, 1);
        shared.take_event(denied.clone(), context(Some(11), Some(1)));
        assert_eq!(shared.plugin(10).unwrap().events, [granted, denied.clone()]);
        assert_eq!(shared.plugin(11).unwrap().events, [denied]);
        // So a slot asking for what was granted before asks Zellij again.
        shared.add_slot(slot(12, SlotKind::Pane, &[("ask", "r")]));
        assert_eq!(
            shared.render_slot(1, 80, 12, 1).effects,
            [Effect::RequestPermission(permissions("r"))]
        );
    }

    #[test]
    fn a_prompt_s_answer_goes_to_the_slot_that_showed_it() {
        let mut shared = with_panes(&[10, 11]);
        act(&mut shared, 11, &[Act::Prompt]);
        shared.take_event(Event::Visible(true), context(Some(11), None));
        let answer = Event::PromptResult(1, PromptResult::Cancelled);
        shared.take_event(answer.clone(), context(None, Some(1)));
        assert!(shared.plugin(10).unwrap().events.is_empty());
        assert_eq!(shared.plugin(11).unwrap().events[1], answer);
        // An answer to a prompt nobody is known to have shown goes to every
        // slot, which each tell their own prompts by id.
        let stray = Event::PromptResult(9, PromptResult::Cancelled);
        shared.take_event(stray.clone(), context(None, Some(1)));
        assert_eq!(shared.plugin(10).unwrap().events, [stray]);
    }

    #[test]
    fn a_command_s_result_goes_to_the_slot_that_ran_it_without_the_mark() {
        let mut shared = with_panes(&[10, 11]);
        act(&mut shared, 11, &[Act::RunCommand]);
        shared.take_event(Event::Visible(true), context(Some(11), None));
        let Some(Call::RunCommand {
            context: tagged, ..
        }) = shared.plugin(11).unwrap().host.calls().pop()
        else {
            panic!("no command was run");
        };
        let result = |context| Event::RunCommandResult(Some(0), vec![], vec![], context);
        shared.take_event(result(tagged), context(None, Some(1)));
        assert!(shared.plugin(10).unwrap().events.is_empty());
        assert_eq!(
            shared.plugin(11).unwrap().events[1],
            result(BTreeMap::new())
        );
        // An unmarked result goes to every slot.
        shared.take_event(result(BTreeMap::new()), context(None, Some(1)));
        assert_eq!(shared.plugin(10).unwrap().events.len(), 1);
    }

    #[test]
    fn a_result_for_a_slot_that_has_closed_goes_to_no_other() {
        let mut shared = with_panes(&[10, 11]);
        act(&mut shared, 11, &[Act::RunCommand, Act::Prompt]);
        shared.take_event(Event::Visible(true), context(Some(11), None));
        let Some(Call::RunCommand {
            context: tagged, ..
        }) = shared
            .plugin(11)
            .unwrap()
            .host
            .calls()
            .into_iter()
            .find(|call| matches!(call, Call::RunCommand { .. }))
        else {
            panic!("no command was run");
        };
        shared.remove_slot(11);
        let result = Event::RunCommandResult(Some(0), vec![], vec![], tagged.clone());
        shared.take_event(result, context(None, Some(1)));
        let action = Event::ActionComplete(actions::Action::NoOp, None, tagged);
        shared.take_event(action, context(None, Some(1)));
        let answer = Event::PromptResult(1, PromptResult::Cancelled);
        shared.take_event(answer, context(None, Some(1)));
        assert_eq!(shared.plugin(10).unwrap().events, []);
    }

    #[test]
    fn a_message_goes_to_the_slot_it_is_addressed_to_or_to_every_slot() {
        let mut shared = with_panes(&[10, 11]);
        let outcome = shared.take_message(broadcast("one"), context(Some(11), None));
        assert!(shared.plugin(10).unwrap().messages.is_empty());
        assert_eq!(shared.plugin(11).unwrap().messages, ["one"]);
        assert_eq!(outcome.render, RenderResponse::Slots(vec![11]));
        shared.take_message(broadcast("two"), context(None, None));
        assert_eq!(shared.plugin(10).unwrap().messages, ["two"]);
        assert_eq!(shared.plugin(11).unwrap().messages, ["one", "two"]);
    }

    #[test]
    fn a_settings_request_goes_to_one_bar_on_the_requesting_client_s_tab() {
        let mut shared = shared();
        shared.add_slot(slot(9, SlotKind::Pane, &[]));
        for id in [10, 11, 12] {
            shared.add_slot(slot(id, SlotKind::Pane, &[("bar", "")]));
        }
        shared.take_event(
            panes(vec![
                vec![plugin_pane(10, "zjtest")],
                vec![plugin_pane(9, "zjtest"), plugin_pane(11, "zjtest")],
            ]),
            context(None, None),
        );
        shared.take_event(tabs(1), context(None, Some(2)));
        let request = |client: &str| {
            let mut message = broadcast(OPEN_PLUGIN_SETTINGS_MESSAGE);
            message.args = BTreeMap::from([(
                OPEN_PLUGIN_SETTINGS_CLIENT_ID_ARG.to_string(),
                client.to_string(),
            )]);
            message
        };
        let received = |shared: &Shared<Fake>| -> Vec<SlotId> {
            shared
                .slot_ids()
                .into_iter()
                .filter(|id| !shared.plugin(*id).unwrap().messages.is_empty())
                .collect()
        };

        let drew_for = shared.plugin(11).unwrap().bar.as_ref().unwrap().client_id;
        shared.take_message(request("2"), context(None, None));
        assert_eq!(received(&shared), [11]);
        // It drew for client 2 while it answered, and no longer.
        assert_eq!(
            shared.plugin(11).unwrap().bar.as_ref().unwrap().client_id,
            drew_for
        );
        // A client with no tabs known yet reaches the first bar.
        shared.take_message(request("3"), context(None, None));
        assert_eq!(received(&shared), [10, 11]);
    }

    #[test]
    fn a_new_bar_takes_the_visibility_of_a_bar_started_from_the_same_url() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("bar", "")]));
        let manifest = || {
            panes(vec![vec![
                plugin_pane(10, "zjtest"),
                plugin_pane(11, "zjtest"),
                plugin_pane(12, "zjtest-alt"),
                plugin_pane(13, "zjtest"),
            ]])
        };
        shared.take_event(manifest(), context(None, None));
        let bar = |shared: &Shared<Fake>, id| {
            shared
                .plugin(id)
                .unwrap()
                .bar
                .as_ref()
                .unwrap()
                .visibility
                .is_hidden()
        };
        shared
            .plugin_mut(10)
            .unwrap()
            .bar
            .as_mut()
            .unwrap()
            .change_visibility(Request::Hide, None);

        let outcome = shared.add_slot(slot(11, SlotKind::Pane, &[("bar", "")]));
        assert!(bar(&shared, 11));
        assert_eq!(outcome.render, RenderResponse::Slots(vec![11]));
        // A bar of another alias keeps its own.
        shared.add_slot(slot(12, SlotKind::Pane, &[("bar", "")]));
        assert!(!bar(&shared, 12));

        // A bar the user showed before its URL was known stays shown.
        shared.take_event(panes(vec![vec![]]), context(None, None));
        shared.add_slot(slot(13, SlotKind::Pane, &[("bar", "")]));
        shared
            .plugin_mut(13)
            .unwrap()
            .bar
            .as_mut()
            .unwrap()
            .change_visibility(Request::Show, None);
        shared.take_event(manifest(), context(None, None));
        assert!(!bar(&shared, 13));
    }

    #[test]
    fn a_bar_hidden_or_shown_gives_up_or_takes_back_its_row_at_once() {
        let mut shared = logging(&[]);
        shared.add_slot(slot(10, SlotKind::Pane, &[("bar", "")]));
        // On a tab nobody looks at, slot 11 is not drawn.
        shared.add_slot(slot(11, SlotKind::Pane, &[("bar", "")]));
        let message = |shared: &mut Shared<Fake>, request| {
            act(shared, 10, &[Act::Visibility(request)]);
            act(shared, 11, &[Act::Visibility(request)]);
            shared.take_message(broadcast("zjtest::toggle"), context(None, None));
        };
        message(&mut shared, Request::Hide);
        assert_eq!(
            logged_so_far(),
            [
                "SetCollapsed(10, true) drawing None",
                "SetCollapsed(11, true) drawing None"
            ]
        );
        // Drawn while hidden, with nothing to draw, it says nothing more.
        act(&mut shared, 10, &[Act::Row(true)]);
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(logged_so_far(), ["drawn"]);
        // Shown, Zellij would never draw it to let it expand by itself.
        message(&mut shared, Request::Show);
        assert_eq!(
            logged_so_far(),
            [
                "SetCollapsed(10, false) drawing None",
                "SetCollapsed(11, false) drawing None"
            ]
        );
        // Still empty, it gives the row up again as it draws.
        act(&mut shared, 10, &[Act::Row(true)]);
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            logged_so_far(),
            ["SetCollapsed(10, true) drawing Some(10)", "drawn"]
        );
        // A bar collapsed for being empty is not expanded by other messages.
        shared.take_message(broadcast("anything"), context(None, None));
        assert_eq!(logged_so_far(), Vec::<String>::new());
    }

    #[test]
    fn calls_made_while_a_slot_draws_are_made_before_it_prints_and_in_its_order() {
        let mut shared = logging(&[10]);
        let pane = PaneId::Terminal(3);
        // Placed while Zellij is not drawing the slot, the items wait.
        act(&mut shared, 10, &[Act::FrameItems(pane, frame_items())]);
        shared.take_message(broadcast("anything"), context(None, None));
        assert_eq!(logged_so_far(), Vec::<String>::new());

        act(
            &mut shared,
            10,
            &[Act::Collapse(true), Act::Selectable(false)],
        );
        let outcome = shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            logged_so_far(),
            [
                format!(
                    "{:?} drawing Some(10)",
                    Effect::SetPaneFrameItems(pane, frame_items())
                ),
                "SetCollapsed(10, true) drawing Some(10)".to_string(),
                "SetSelectable(10, false) drawing Some(10)".to_string(),
                "drawn".to_string(),
            ]
        );
        // Made as they came, they are not handed back as well.
        assert_eq!(outcome.effects, []);
    }

    #[test]
    fn calls_made_while_a_slot_takes_an_event_are_made_as_it_makes_them() {
        let mut shared = logging(&[10]);
        act(&mut shared, 10, &[Act::Timeout(1.0), Act::Collapse(true)]);
        shared.take_event(Event::Visible(true), context(Some(10), Some(1)));
        assert_eq!(
            logged_so_far(),
            [
                "SetTimeout(1.0) drawing None",
                "SetCollapsed(10, true) drawing None",
                "updated",
            ]
        );
    }

    #[test]
    fn events_a_slot_s_drawing_leads_to_wait_until_it_has_drawn_and_after() {
        let mut shared = shared();
        shared.add_slot(slot(10, SlotKind::Pane, &[("ask", "r")]));
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        shared.take_event(granted.clone(), context(Some(10), Some(1)));
        // Granted already, so the slot is told without asking Zellij; but not
        // while it draws, as whatever it does then would follow its drawing.
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(shared.plugin(10).unwrap().events, vec![granted.clone()]);
        shared.take_event(Event::Visible(true), context(Some(10), Some(1)));
        assert_eq!(
            shared.plugin(10).unwrap().events,
            vec![granted.clone(), granted, Event::Visible(true)]
        );
    }

    #[test]
    fn a_slot_to_draw_again_found_while_another_draws_is_drawn_after() {
        let mut shared = with_panes(&[10, 11]);
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        shared.render_slot(1, 80, 10, 1);
        shared.take_event(granted, context(Some(10), Some(1)));
        // Granted already, slot 10 is told so after it has drawn. Zellij
        // draws slot 11 next, takes no answer from that, and the next call
        // is a message for slot 11 alone.
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        shared.render_slot(1, 80, 10, 1);
        assert_eq!(
            shared.render_slot(1, 80, 11, 1).render,
            RenderResponse::Nothing
        );
        let outcome = shared.take_message(broadcast("one"), context(Some(11), None));
        assert_eq!(outcome.render, RenderResponse::Slots(vec![10, 11]));
    }

    #[test]
    fn a_bar_hidden_by_what_it_is_told_as_another_draws_gives_up_its_row_then() {
        let mut shared = logging(&[]);
        for id in [10, 11] {
            shared.add_slot(slot(id, SlotKind::Pane, &[("bar", "")]));
        }
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        shared.render_slot(1, 80, 10, 1);
        shared.take_event(granted, context(Some(10), Some(1)));
        // Told it is granted as slot 11 draws, slot 10 hides.
        act(&mut shared, 10, &[Act::Ask(permissions("r"))]);
        shared.render_slot(1, 80, 10, 1);
        act(&mut shared, 10, &[Act::Visibility(Request::Hide)]);
        logged_so_far();
        shared.render_slot(1, 80, 11, 1);
        assert_eq!(
            logged_so_far(),
            [
                "updated",
                "SetCollapsed(10, true) drawing Some(11)",
                "drawn"
            ]
        );
    }

    #[test]
    fn a_removed_slot_gets_before_close_and_gives_up_its_frame_items() {
        let mut shared = with_panes(&[10]);
        let pane = PaneId::Terminal(3);
        act(&mut shared, 10, &[Act::FrameItems(pane, frame_items())]);
        shared.render_slot(1, 80, 10, 1);
        shared.plugin_mut(10).unwrap().next = vec![Act::Close];
        let outcome = shared.remove_slot(10);
        assert_eq!(outcome.effects, [Effect::Close(10)]);
        assert_eq!(outcome.render, RenderResponse::Nothing);
        assert!(shared.plugin(10).is_none());
        assert!(shared.router.frame_owners.is_empty());
    }

    #[test]
    fn a_recorder_reports_the_acting_slot_and_client_as_its_ids() {
        let host = Recorder::default();
        assert_eq!(host.plugin_ids().plugin_id, Recorder::PLUGIN_ID);
        begin(Active {
            acting: Acting {
                slot: 12,
                client: Some(4),
            },
            kind: SlotKind::Pane,
            callback: Callback::NONE,
            router: Router::default(),
        });
        let ids = host.plugin_ids();
        host.set_selectable(true);
        let router = end();
        assert_eq!((ids.plugin_id, ids.client_id), (12, 4));
        assert_eq!(router.effects, [Effect::SetSelectable(12, true)]);
        assert_eq!(
            host.slot_calls(),
            [
                (None, Call::PluginIds),
                (Some(12), Call::PluginIds),
                (Some(12), Call::SetSelectable(true)),
            ]
        );
        assert_eq!(host.instance_id(), Recorder::INSTANCE_ID);
    }

    #[test]
    fn nothing_is_handed_over_while_no_slot_is_acting() {
        assert!(!submit(Command::ShowSelf));
        assert_eq!(tag_context(BTreeMap::new()), BTreeMap::new());
    }
}
