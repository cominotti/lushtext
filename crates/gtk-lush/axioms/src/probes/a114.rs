// SPDX-License-Identifier: MIT OR Apache-2.0

//! A114: the disclosure routes of `GtkTreeExpander`, and how an application
//! can own them.
//!
//! - **Click.** The `GtkGestureClick` that toggles a row lives on the
//!   expander's `expander` icon child, not on the expander (whose
//!   `observe_controllers()` lists none), and GTK re-creates it with the icon
//!   whenever the row is rebound. It activates `listitem.toggle-expand` on a
//!   release inside the icon. A capture-phase `GtkGestureClick` on the
//!   expander that claims the sequence on a release inside the current icon
//!   denies it, so the click toggles nothing, also after the icon is
//!   re-created; the click leaves the selection on the item it was on and the
//!   focus where it was, as GTK's own toggle does, and a double-press
//!   elsewhere on a row still activates it.
//! - **Keys.** Its class key bindings (`plus`, `minus`, `asterisk`, `slash`,
//!   their keypad forms, `Shift`/`Ctrl+Shift` with `Left`/`Right`, and
//!   `Ctrl+space`) fire only for key events that reach the expander. In a
//!   row built like LushText's (the list item's child an overlay holding the
//!   expander), the focus the list gives a row (a click, `scroll_to` with
//!   `FOCUS`, arrow keys) stays on the row widget, from which none reaches
//!   the expander; Tab moves the focus on into the expander, from which they
//!   fire. The probe records which bindings fired from each; it does not
//!   type `Ctrl+KP_Space`, which has no keycode in the compositor's virtual
//!   keymap (its binding is read from the source). A capture-phase
//!   key controller on an ancestor returning `Stop` prevents them.
//! - **Actions.** Its class actions `listitem.expand`, `listitem.collapse` and
//!   `listitem.toggle-expand` take no parameter (so the generic AT-SPI widget
//!   vtable, `gtkatspiaction.c:1063`, lists them) and resolve before an action
//!   group inserted on the expander under the same prefix; activating
//!   `listitem.collapse` collapses the row at once.
//! - **Accessible state.** GTK sets the expander's accessible `expanded` state
//!   when it binds a row, and on `notify::expanded` updates only the icon's.
//!   GTK records no accessible state under `GTK_A11Y=none`, which the axiom
//!   lane sets, so there the probe skips this half and says so; run the
//!   sample with `GTK_A11Y=test` to observe it.
//!
//! Real pointer and key input go through the private compositor
//! (RemoteDesktop, `remote_input`); inside an SDK sandbox, which has none,
//! the probe checks the gesture location, the accessible state and the
//! class actions and skips the input half, saying so. LushText's sidebar
//! (`slice-large-directory-collapse` D5/D6) owns the click this way, keeps
//! the expander's accessible state in step with its own collapse, and leaves
//! the class actions (an assistive technology's `DoAction`) as GTK's direct
//! collapse.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use crate::observation::Recorder;
use crate::probes::remote_input::{
    BUTTON_PRIMARY, RemoteInput, fullscreen_window, keysym, point_on_screen,
};
use crate::session::{Presented, flush_events, settle, wait_until};
use crate::{AxiomId, Observation};

/// The expander's three class actions.
pub const CLASS_ACTIONS: [&str; 3] = [
    "listitem.expand",
    "listitem.collapse",
    "listitem.toggle-expand",
];

/// What a key binding does to an expandable row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyEffect {
    /// Expands a collapsed row.
    Expand,
    /// Collapses an expanded row.
    Collapse,
    /// Toggles the row.
    Toggle,
}

/// One class key binding: its name, modifiers, keysym and effect (LTR).
pub type KeyBinding = (&'static str, &'static [u32], u32, KeyEffect);

const SHIFT: &[u32] = &[keysym::SHIFT_L];
const CTRL: &[u32] = &[keysym::CONTROL_L];
const CTRL_SHIFT: &[u32] = &[keysym::CONTROL_L, keysym::SHIFT_L];
const NONE: &[u32] = &[];

/// Every class key binding of `GtkTreeExpander` (`gtktreeexpander.c:654-690`).
pub const KEY_BINDINGS: [KeyBinding; 18] = [
    ("plus", NONE, 0x2b, KeyEffect::Expand),
    ("KP_Add", NONE, 0xffab, KeyEffect::Expand),
    ("asterisk", NONE, 0x2a, KeyEffect::Expand),
    ("KP_Multiply", NONE, 0xffaa, KeyEffect::Expand),
    ("minus", NONE, 0x2d, KeyEffect::Collapse),
    ("KP_Subtract", NONE, 0xffad, KeyEffect::Collapse),
    ("slash", NONE, 0x2f, KeyEffect::Collapse),
    ("KP_Divide", NONE, 0xffaf, KeyEffect::Collapse),
    ("Shift+Right", SHIFT, 0xff53, KeyEffect::Expand),
    ("Shift+KP_Right", SHIFT, 0xff98, KeyEffect::Expand),
    ("Ctrl+Shift+Right", CTRL_SHIFT, 0xff53, KeyEffect::Expand),
    ("Ctrl+Shift+KP_Right", CTRL_SHIFT, 0xff98, KeyEffect::Expand),
    ("Shift+Left", SHIFT, 0xff51, KeyEffect::Collapse),
    ("Shift+KP_Left", SHIFT, 0xff96, KeyEffect::Collapse),
    ("Ctrl+Shift+Left", CTRL_SHIFT, 0xff51, KeyEffect::Collapse),
    (
        "Ctrl+Shift+KP_Left",
        CTRL_SHIFT,
        0xff96,
        KeyEffect::Collapse,
    ),
    ("Ctrl+space", CTRL, 0x20, KeyEffect::Toggle),
    ("Ctrl+KP_Space", CTRL, KP_SPACE, KeyEffect::Toggle),
];

/// The `expander` icon child of `expander`, if GTK created one.
#[must_use]
pub fn expander_icon(expander: &gtk4::TreeExpander) -> Option<gtk4::Widget> {
    let mut child = expander.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "expander" {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

/// The name [`install_release_claim`] gives its gesture, so
/// [`click_gestures`] counts only the toolkit's own.
pub const RELEASE_CLAIM_NAME: &str = "gtk-lush-axioms-release-claim";

/// How many `GtkGestureClick`s `widget` itself carries, not counting the
/// probe's own release claim (`RELEASE_CLAIM_NAME`).
#[must_use]
pub fn click_gestures(widget: &impl IsA<gtk4::Widget>) -> u32 {
    let controllers = widget.observe_controllers();
    let mut count = 0;
    for index in 0..controllers.n_items() {
        if controllers.item(index).is_some_and(|object| {
            object
                .downcast_ref::<gtk4::GestureClick>()
                .is_some_and(|gesture| gesture.name().as_deref() != Some(RELEASE_CLAIM_NAME))
        }) {
            count += 1;
        }
    }
    count
}

/// Install on `expander` the capture-phase gesture LushText's rows use: it
/// lets the press through and claims the sequence on a release inside the
/// current `expander` icon, counting each claim, while `enabled` holds.
pub fn install_release_claim(
    expander: &gtk4::TreeExpander,
    enabled: &Rc<Cell<bool>>,
    claims: &Rc<Cell<u32>>,
) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_name(Some(RELEASE_CLAIM_NAME));
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let (enabled, claims) = (Rc::clone(enabled), Rc::clone(claims));
    gesture.connect_released(move |gesture, _, x, y| {
        if !enabled.get() {
            return;
        }
        let Some(expander) = gesture.widget().and_downcast::<gtk4::TreeExpander>() else {
            return;
        };
        let Some(icon) = expander_icon(&expander) else {
            return;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "graphene points are f32; event coordinates fit it"
        )]
        let point = gtk4::graphene::Point::new(x as f32, y as f32);
        if let Some(inside) = expander.compute_point(&icon, &point)
            && icon.contains(f64::from(inside.x()), f64::from(inside.y()))
        {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            claims.set(claims.get() + 1);
        }
    });
    expander.add_controller(gesture);
}

/// The probe's tree: two directories of four files each, then a file, in a
/// list whose rows are built like LushText's: an overlay holding a tree
/// expander (whose child holds a label) and, as an overlay, a focusable
/// button.
#[derive(Clone)]
pub struct DisclosureFixture {
    /// The flattened model.
    pub tree: gtk4::TreeListModel,
    /// The selection.
    pub selection: gtk4::SingleSelection,
    /// The list.
    pub list: gtk4::ListView,
    /// The window content.
    pub column: gtk4::Box,
    /// Whether the release claim acts.
    pub claim_enabled: Rc<Cell<bool>>,
    /// Releases the claim took.
    pub claims: Rc<Cell<u32>>,
    /// `activate` emissions of the list.
    pub activations: Rc<Cell<u32>>,
}

/// Build the fixture, not yet shown.
#[must_use]
pub fn disclosure_fixture() -> DisclosureFixture {
    let root = gtk4::gio::ListStore::new::<gtk4::StringObject>();
    for name in ["dir-a", "dir-b", "file-c"] {
        root.append(&gtk4::StringObject::new(name));
    }
    let tree = gtk4::TreeListModel::new(root, false, false, |item| {
        let name = item.downcast_ref::<gtk4::StringObject>()?.string();
        name.starts_with("dir-").then(|| {
            let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
            for index in 0..4 {
                store.append(&gtk4::StringObject::new(&format!("{name}/file-{index}")));
            }
            store.upcast()
        })
    });
    let claim_enabled = Rc::new(Cell::new(false));
    let claims = Rc::new(Cell::new(0));
    let factory = gtk4::SignalListItemFactory::new();
    {
        let (claim_enabled, claims) = (Rc::clone(&claim_enabled), Rc::clone(&claims));
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let expander = gtk4::TreeExpander::new();
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            let label = gtk4::Label::new(None);
            label.set_hexpand(true);
            label.set_xalign(0.0);
            content.append(&label);
            expander.set_child(Some(&content));
            install_release_claim(&expander, &claim_enabled, &claims);
            let overlay = gtk4::Overlay::new();
            overlay.set_child(Some(&expander));
            let button = gtk4::Button::with_label("Focus");
            button.set_halign(gtk4::Align::End);
            overlay.add_overlay(&button);
            item.set_child(Some(&overlay));
        });
    }
    factory.connect_bind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(row) = item.item().and_downcast::<gtk4::TreeListRow>() else {
            return;
        };
        if let Some(expander) = item.child().as_ref().and_then(row_expander) {
            expander.set_list_row(Some(&row));
            if let Some(label) = expander.child().and_then(|content| content.first_child())
                && let Some(label) = label.downcast_ref::<gtk4::Label>()
                && let Some(name) = row.item().and_downcast::<gtk4::StringObject>()
            {
                label.set_text(&name.string());
            }
        }
    });
    factory.connect_unbind(|_, object| {
        if let Some(expander) = object
            .downcast_ref::<gtk4::ListItem>()
            .and_then(gtk4::ListItem::child)
            .as_ref()
            .and_then(row_expander)
        {
            expander.set_list_row(None::<&gtk4::TreeListRow>);
        }
    });
    let selection = gtk4::SingleSelection::new(Some(tree.clone()));
    selection.set_autoselect(false);
    let list = gtk4::ListView::new(Some(selection.clone()), Some(factory));
    list.add_css_class("navigation-sidebar");
    let activations = Rc::new(Cell::new(0));
    {
        let activations = Rc::clone(&activations);
        list.connect_activate(move |_, _| activations.set(activations.get() + 1));
    }
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.set_halign(gtk4::Align::Start);
    column.set_valign(gtk4::Align::Start);
    column.set_size_request(400, 400);
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    column.append(&scroller);
    DisclosureFixture {
        tree,
        selection,
        list,
        column,
        claim_enabled,
        claims,
        activations,
    }
}

impl DisclosureFixture {
    /// The expander bound to flattened position `position`.
    #[must_use]
    pub fn expander_at(&self, position: u32) -> Option<gtk4::TreeExpander> {
        let row = self
            .tree
            .item(position)
            .and_downcast::<gtk4::TreeListRow>()?;
        let mut child = self.list.first_child();
        while let Some(widget) = child {
            if let Some(expander) = widget.first_child().as_ref().and_then(row_expander)
                && expander.list_row().as_ref() == Some(&row)
            {
                return Some(expander);
            }
            child = widget.next_sibling();
        }
        None
    }

    /// The flattened row at `position`.
    #[must_use]
    pub fn row(&self, position: u32) -> Option<gtk4::TreeListRow> {
        self.tree.item(position).and_downcast()
    }
}

/// The list's row widget holding `expander` (the overlay's parent).
fn row_widget(expander: &gtk4::TreeExpander) -> gtk4::Widget {
    expander
        .parent()
        .and_then(|overlay| overlay.parent())
        .unwrap_or_else(|| expander.clone().upcast())
}

/// The X keysym of Tab.
const TAB: u32 = 0xff09;

/// The type name of the window's focus widget, if any.
fn focus_type(window: &impl IsA<gtk4::Window>) -> Option<String> {
    gtk4::prelude::GtkWindowExt::focus(window.as_ref())
        .map(|widget| widget.type_().name().to_owned())
}

/// The tree expander of a row's content (the overlay a list item holds).
fn row_expander(content: &gtk4::Widget) -> Option<gtk4::TreeExpander> {
    content
        .downcast_ref::<gtk4::Overlay>()?
        .child()
        .and_downcast::<gtk4::TreeExpander>()
}

/// Move to and click the centre of `widget` (or double-click it).
fn click_centre(input: &RemoteInput, widget: &gtk4::Widget, presses: u32) -> Result<(), String> {
    let (x, y) = point_on_screen(
        widget,
        f64::from(widget.width()) / 2.0,
        f64::from(widget.height()) / 2.0,
    )
    .ok_or_else(|| "the widget is not on screen".to_owned())?;
    input.move_to(x, y)?;
    for _ in 0..presses {
        input.click(BUTTON_PRIMARY)?;
    }
    settle(LAYOUT_SETTLE);
    Ok(())
}

/// Whether the keyboard focus is `widget` or inside it.
fn focus_within(window: &impl IsA<gtk4::Window>, widget: &impl IsA<gtk4::Widget>) -> bool {
    gtk4::prelude::GtkWindowExt::focus(window.as_ref())
        .is_some_and(|focus| focus.is_ancestor(widget) || &focus == widget.as_ref())
}

/// The X keysym of `KP_Space`, which the compositor's virtual keymap cannot
/// type.
const KP_SPACE: u32 = 0xff80;

/// Whether `effect` changed a row from `before` to `after`.
const fn took_effect(effect: KeyEffect, before: bool, after: bool) -> bool {
    match effect {
        KeyEffect::Expand => !before && after,
        KeyEffect::Collapse => before && !after,
        KeyEffect::Toggle => before != after,
    }
}

/// Press each binding with focus given by `focus`, returning the names that
/// changed the directory row.
fn reachable_keys(
    input: &RemoteInput,
    fixture: &DisclosureFixture,
    focus: &dyn Fn(),
) -> Result<Vec<&'static str>, String> {
    let mut reached = Vec::new();
    for (name, modifiers, key, effect) in KEY_BINDINGS {
        if key == KP_SPACE {
            // The virtual keymap has no keycode for it: Mutter would log a
            // warning and send nothing. It is source-read instead.
            continue;
        }
        let row = fixture
            .row(0)
            .ok_or_else(|| "the directory row is gone".to_owned())?;
        row.set_expanded(effect == KeyEffect::Collapse);
        settle(LAYOUT_SETTLE);
        focus();
        settle(Duration::from_millis(60));
        let before = row.is_expanded();
        input.chord(modifiers, key)?;
        settle(LAYOUT_SETTLE);
        if took_effect(effect, before, row.is_expanded()) {
            reached.push(name);
        }
    }
    Ok(reached)
}

/// Probe A114. See the module documentation.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one probe walks the four routes in order on one presented fixture; splitting \
              it would pass the fixture, the input session and the recorder through helpers \
              that read worse than the sequence"
)]
pub fn probe_a114() -> Observation {
    Recorder::run(AxiomId::new(114), |recorder| {
        let fixture = disclosure_fixture();
        let shown = Presented::checked_window(
            recorder,
            fullscreen_window(&fixture.column),
            "control: the list was presented",
        )?;
        let window = shown.window().clone();
        recorder.control(
            wait_until(Duration::from_secs(5), || window.is_fullscreen()),
            "control: the window is fullscreen",
        )?;
        settle(LAYOUT_SETTLE);
        let row = recorder.require(fixture.row(0), "control: the directory row exists")?;
        let expander = recorder.require(
            fixture.expander_at(0),
            "control: the directory row is bound",
        )?;
        let icon = recorder.require(expander_icon(&expander), "control: the icon exists")?;

        // Where the click gesture lives.
        let on_expander = click_gestures(&expander);
        let on_icon = click_gestures(&icon);
        recorder.measure("expander_click_gestures", on_expander);
        recorder.measure("icon_click_gestures", on_icon);
        recorder.axiom(
            on_expander == 0 && on_icon == 1,
            "A114: the toggle GestureClick is on the expander icon, not on the expander",
        )?;

        // Accessible state at bind and on notify::expanded. The lane runs
        // with `GTK_A11Y=none`, under which GTK records no accessible state at
        // all; a scratch widget tells the two cases apart, and without
        // recorded states the half is skipped, saying why.
        let scratch = gtk4::Label::new(None);
        scratch.update_state(&[gtk4::accessible::State::Expanded(Some(true))]);
        let states_recorded =
            gtk4::test_accessible_has_state(&scratch, gtk4::AccessibleState::Expanded);
        recorder.measure("accessible_states_recorded", states_recorded);
        if states_recorded {
            let expander_state_at_bind =
                gtk4::test_accessible_has_state(&expander, gtk4::AccessibleState::Expanded);
            let icon_state_at_bind =
                gtk4::test_accessible_has_state(&icon, gtk4::AccessibleState::Expanded);
            expander.reset_state(gtk4::AccessibleState::Expanded);
            row.set_expanded(true);
            flush_events();
            let expander_state_after_notify =
                gtk4::test_accessible_has_state(&expander, gtk4::AccessibleState::Expanded);
            let icon_state_after_notify =
                gtk4::test_accessible_has_state(&icon, gtk4::AccessibleState::Expanded);
            recorder.measure(
                "accessible_expanded_present(expander_at_bind,icon_at_bind,expander_after_notify,icon_after_notify)",
                format!(
                    "{expander_state_at_bind},{icon_state_at_bind},{expander_state_after_notify},{icon_state_after_notify}"
                ),
            );
            recorder.axiom(
                expander_state_at_bind
                    && !icon_state_at_bind
                    && !expander_state_after_notify
                    && icon_state_after_notify,
                "A114: GTK sets the expander's accessible expanded state at bind, and on \
                 notify::expanded updates only the icon's",
            )?;
        } else {
            recorder.measure(
                "accessible_state_half_skipped",
                "GTK records no accessible state in this session (GTK_A11Y=none); run with \
                 GTK_A11Y=test to observe it",
            );
        }
        row.set_expanded(false);
        settle(LAYOUT_SETTLE);

        // Class actions: each is enabled and takes no parameter (activating
        // it without one acts), which is what the generic AT-SPI widget
        // vtable lists.
        let mut acting = Vec::new();
        for (name, start) in [
            ("listitem.expand", false),
            ("listitem.collapse", true),
            ("listitem.toggle-expand", false),
        ] {
            row.set_expanded(start);
            flush_events();
            let activated = expander.activate_action(name, None).is_ok();
            if activated && row.is_expanded() != start {
                acting.push(name);
            }
        }
        recorder.measure(
            "class_actions_acting_without_a_parameter",
            format!("{acting:?}"),
        );
        recorder.axiom(
            CLASS_ACTIONS.iter().all(|name| acting.contains(name)),
            "A114: the expander's three class actions are enabled and take no parameter, so \
             the generic AT-SPI widget vtable lists them",
        )?;
        row.set_expanded(false);
        flush_events();
        let shadowed = Rc::new(Cell::new(0u32));
        let group = gtk4::gio::SimpleActionGroup::new();
        let shadow = gtk4::gio::SimpleAction::new("collapse", None);
        {
            let shadowed = Rc::clone(&shadowed);
            shadow.connect_activate(move |_, _| shadowed.set(shadowed.get() + 1));
        }
        group.add_action(&shadow);
        expander.insert_action_group("listitem", Some(&group));
        row.set_expanded(true);
        flush_events();
        let activated = expander.activate_action("listitem.collapse", None).is_ok();
        let collapsed_at_once = !row.is_expanded();
        expander.insert_action_group("listitem", None::<&gtk4::gio::ActionGroup>);
        recorder.measure("collapse_action_activated", activated);
        recorder.measure("collapsed_inside_the_activation", collapsed_at_once);
        recorder.measure("inserted_group_activations", shadowed.get());
        recorder.axiom(
            activated && collapsed_at_once && shadowed.get() == 0,
            "A114: the class action resolves before an inserted listitem group and collapses \
             the row at once",
        )?;
        settle(LAYOUT_SETTLE);

        // The SDK sandbox has no session bus, so no RemoteDesktop: the
        // input half is skipped there, saying why, and the half above (where
        // the gesture lives, the class actions) still decides the verdict.
        if super::remote_input::in_sdk_sandbox() {
            recorder.measure(
                "input_half_skipped",
                "the SDK sandbox has no RemoteDesktop (GTK_LUSH_AXIOMS_SANDBOX=1)",
            );
            return Ok(());
        }
        let input = match RemoteInput::start() {
            Ok(input) => input,
            Err(error) => {
                recorder.measure("remote_desktop_error", error);
                return recorder
                    .control(false, "control: the compositor accepts RemoteDesktop input");
            }
        };

        // A click on the icon: GTK's toggle, then the release claim. Another
        // row is selected first both times, so a click that selects the
        // directory row shows.
        fixture.selection.set_selected(2);
        settle(LAYOUT_SETTLE);
        let before_unclaimed = fixture.selection.selected_item();
        let unclaimed = click_centre(&input, &icon, 1).map(|()| row.is_expanded());
        recorder.measure("unclaimed_click", format!("{unclaimed:?}"));
        recorder.control(
            unclaimed == Ok(true),
            "control: an unclaimed click on the icon expands the row",
        )?;
        // Compared as the selected item, not its position: expanding the row
        // moves the rows after it.
        let selected_unclaimed = fixture.selection.selected_item() == before_unclaimed;
        let focus_unclaimed = focus_within(&window, &row_widget(&expander));
        row.set_expanded(false);
        fixture.selection.set_selected(2);
        settle(LAYOUT_SETTLE);
        fixture.claim_enabled.set(true);
        let icon = recorder.require(expander_icon(&expander), "control: the icon exists")?;
        let before_claimed = fixture.selection.selected_item();
        let claimed = click_centre(&input, &icon, 1).map(|()| row.is_expanded());
        let selected_claimed = fixture.selection.selected_item() == before_claimed;
        let focus_claimed = focus_within(&window, &row_widget(&expander));
        recorder.measure("claimed_click_expanded", format!("{claimed:?}"));
        recorder.measure("claims", fixture.claims.get());
        recorder.measure(
            "selection_kept_its_item_after_click(unclaimed,claimed)",
            format!("{selected_unclaimed},{selected_claimed}"),
        );
        recorder.measure(
            "focus_on_row_after_click(unclaimed,claimed)",
            format!("{focus_unclaimed},{focus_claimed}"),
        );
        recorder.axiom(
            claimed == Ok(false) && fixture.claims.get() == 1,
            "A114: a capture-phase gesture claiming the release inside the icon denies GTK's \
             toggle",
        )?;
        recorder.axiom(
            selected_claimed == selected_unclaimed && focus_claimed == focus_unclaimed,
            "A114: the click leaves selection and focus as GTK's own toggle does (the \
             selection stays on its item)",
        )?;

        // After the icon is re-created (a rebind).
        let old_icon = icon;
        expander.set_list_row(None::<&gtk4::TreeListRow>);
        expander.set_list_row(Some(&row));
        settle(LAYOUT_SETTLE);
        let new_icon = recorder.require(expander_icon(&expander), "control: the icon exists")?;
        recorder.control(
            new_icon != old_icon,
            "control: rebinding the row re-created the icon",
        )?;
        let recreated = click_centre(&input, &new_icon, 1).map(|()| row.is_expanded());
        recorder.measure("recreated_icon_click_expanded", format!("{recreated:?}"));
        recorder.axiom(
            recreated == Ok(false) && fixture.claims.get() == 2,
            "A114: the claim still denies the toggle after GTK re-creates the icon",
        )?;

        // A double-press on a file row still activates it.
        let file_row = recorder.require(
            fixture.expander_at(2).and_then(|file| file.child()),
            "control: the file row is bound",
        )?;
        let before = fixture.activations.get();
        let _ = click_centre(&input, &file_row, 2);
        let activations = fixture.activations.get() - before;
        recorder.measure("double_press_activations", activations);
        recorder.axiom(
            activations >= 1,
            "A114: a double-press on a row still activates it with the claim installed",
        )?;
        fixture.claim_enabled.set(false);

        // Keys from the focus the list gives the row, and from the expander
        // once Tab moves the focus into the row. The row widget offers
        // grab_focus to its child (the overlay), which does not pass it on,
        // so the row widget keeps it; focus traversal (Tab) descends the
        // overlay to the expander, which takes it itself when its child holds
        // no focusable widget (gtktreeexpander.c:314-346, focusable at :746).
        let list = fixture.list.clone();
        let focus_row = move || {
            list.scroll_to(0, gtk4::ListScrollFlags::FOCUS, None);
        };
        focus_row();
        settle(Duration::from_millis(60));
        let focused_row = focus_type(&window);
        recorder.measure("focus_after_list_focus", format!("{focused_row:?}"));
        let row_keys = reachable_keys(&input, &fixture, &focus_row);
        let list = fixture.list.clone();
        let tab_input = &input;
        let focus_expander = move || {
            list.scroll_to(0, gtk4::ListScrollFlags::FOCUS, None);
            settle(Duration::from_millis(60));
            let _ = tab_input.chord(NONE, TAB);
        };
        focus_expander();
        settle(Duration::from_millis(60));
        let focused_tab = focus_type(&window);
        recorder.measure("focus_after_tab_from_the_row", format!("{focused_tab:?}"));
        let expander_keys = reachable_keys(&input, &fixture, &focus_expander);
        recorder.measure("keys_reaching_from_list_focus", format!("{row_keys:?}"));
        recorder.measure("keys_reaching_after_tab", format!("{expander_keys:?}"));
        let (Ok(row_keys), Ok(expander_keys)) = (row_keys, expander_keys) else {
            return recorder.control(false, "control: the keys were injected");
        };
        recorder.axiom(
            focused_row.as_deref() == Some("GtkListItemWidget") && row_keys.is_empty(),
            "A114: the focus the list gives a row whose child is an overlay stays on the row \
             widget, and no class key binding reaches the expander from it",
        )?;
        recorder.axiom(
            focused_tab.as_deref() == Some("GtkTreeExpander"),
            "A114: Tab from the focused row moves the focus to the expander",
        )?;
        recorder.axiom(
            expander_keys.contains(&"plus") && expander_keys.contains(&"minus"),
            "A114: the class key bindings fire with the focus on the expander",
        )?;
        let stop = gtk4::EventControllerKey::new();
        stop.set_propagation_phase(gtk4::PropagationPhase::Capture);
        stop.connect_key_pressed(|_, _, _, _| glib::Propagation::Stop);
        fixture.list.add_controller(stop);
        let stopped = reachable_keys(&input, &fixture, &focus_expander);
        recorder.measure("keys_reaching_under_a_capture_stop", format!("{stopped:?}"));
        recorder.axiom(
            stopped.is_ok_and(|keys| keys.is_empty()),
            "A114: a capture-phase key controller on an ancestor returning Stop prevents them",
        )?;
        drop(input);
        drop(shown);
        flush_events();
        Ok(())
    })
}
