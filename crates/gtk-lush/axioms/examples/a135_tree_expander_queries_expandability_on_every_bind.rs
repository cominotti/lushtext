// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A135: `GtkTreeExpander` queries `is_expandable()` synchronously
//! on every bind (`set_list_row`), rebind, `hide-expander` change, and
//! `notify::children` after collapse. Watch the live create-call counter tick
//! when rebinding or collapsing `a`, while `b` (which returned `None`) never
//! queries again.

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a135::{CHILDLESS, EXPANDABLE, ExpanderTree};
use gtk4::prelude::*;

fn tree_view(fixture: &ExpanderTree, hide_expander: bool) -> gtk4::ListView {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let expander = gtk4::TreeExpander::new();
        expander.set_child(Some(&gtk4::Label::new(None)));
        item.set_child(Some(&expander));
    });
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let row = item.item().and_downcast::<gtk4::TreeListRow>();
        let Some(expander) = item.child().and_downcast::<gtk4::TreeExpander>() else {
            return;
        };
        let text = row
            .as_ref()
            .and_then(|row| row.item().and_downcast::<gtk4::StringObject>())
            .map_or_default(|object| object.string().to_string());
        expander.set_hide_expander(hide_expander);
        expander.set_list_row(row.as_ref());
        if let Some(label) = expander.child().and_downcast::<gtk4::Label>() {
            label.set_text(&text);
        }
    });
    gtk4::ListView::new(
        Some(gtk4::NoSelection::new(Some(fixture.tree.clone()))),
        Some(factory),
    )
}

fn build(ui: &support::SampleUi) {
    let fixture = ExpanderTree::new();
    let list = tree_view(&fixture, false);
    ui.set_fixture(&list);
    let last = Rc::new(RefCell::new(String::from("ready")));

    for (label, name, expanded) in [
        ("Expand a (installs store)", EXPANDABLE, true),
        ("Collapse a (finalizes + re-probes)", EXPANDABLE, false),
        ("Expand b (None; cached empty)", CHILDLESS, true),
    ] {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control(label, move || {
            let Some((_, row)) = fixture.find(name) else {
                last.replace(format!("missing row {name}"));
                return;
            };
            let before = fixture.create_calls();
            row.set_expanded(expanded);
            last.replace(format!(
                "set_expanded({expanded}) on {name}: +{} create calls, latest alive={}",
                fixture.create_calls() - before,
                fixture.latest_store_alive(),
            ));
        });
    }

    let (rebind_fixture, rebind_last, rebind_list) = (fixture.clone(), last.clone(), list);
    ui.add_control("Rebind factory (simulates scroll recycle)", move || {
        let before = rebind_fixture.create_calls();
        let model = rebind_list.model();
        rebind_list.set_model(None::<&gtk4::SelectionModel>);
        rebind_list.set_model(model.as_ref());
        rebind_last.replace(format!(
            "rebind: +{} create calls, latest alive={}",
            rebind_fixture.create_calls() - before,
            rebind_fixture.latest_store_alive(),
        ));
    });

    ui.set_readout(move || {
        format!(
            "create calls {}   returned stores {}   latest alive {}\n{}",
            fixture.create_calls(),
            fixture.returned_store_count(),
            fixture.latest_store_alive(),
            last.borrow(),
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(135), build)
}
