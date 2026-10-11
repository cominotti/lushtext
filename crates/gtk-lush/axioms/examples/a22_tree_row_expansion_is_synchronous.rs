// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A22: `TreeListRow::set_expanded(true)` calls the create
//! function before it returns, so the row's expanded state, its `children()`,
//! and the flattened model's item count are already final when the call
//! returns. Row `a` gets a child store, row `b` gets `None` and stays
//! collapsed. Append a child to `a`'s store as a directory scan would, and
//! collapse `a` again; the readout shows what each call left behind.

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::LazyTree;
use gtk_lush_axioms::fixtures::a22::{CHILDLESS, EXPANDABLE};
use gtk4::prelude::*;

/// A list view showing `fixture`'s flattened model with tree expanders.
fn tree_view(fixture: &LazyTree) -> gtk4::ListView {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let expander = gtk4::TreeExpander::new();
        expander.set_child(Some(&gtk4::Label::new(None)));
        item.set_child(Some(&expander));
    });
    factory.connect_bind(|_, item| {
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
            .map(|object| object.string().to_string())
            .unwrap_or_default();
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

/// What the last `set_expanded` call saw when it returned.
#[derive(Default)]
struct LastCall(RefCell<String>);

/// Run `set_expanded(expanded)` on the row named `name` and record what
/// the fixture looked like when the call returned.
fn expand(fixture: &LazyTree, last: &LastCall, name: &str, expanded: bool) {
    let Some((_, row)) = fixture.find(name) else {
        last.0.replace(format!("no row {name}"));
        return;
    };
    let calls_before = fixture.create_calls();
    row.set_expanded(expanded);
    last.0.replace(format!(
        "set_expanded({expanded}) on {name} returned: create calls +{}, expanded {}, \
         children {}, n_items {}",
        fixture.create_calls() - calls_before,
        row.is_expanded(),
        row.children().is_some(),
        fixture.tree.n_items(),
    ));
}

fn build(ui: &support::SampleUi) {
    let fixture = LazyTree::new();
    ui.set_fixture(&tree_view(&fixture));
    let last = Rc::new(LastCall::default());
    for (label, name, expanded) in [
        ("Expand a (gets a store)", EXPANDABLE, true),
        ("Expand b (gets None)", CHILDLESS, true),
        ("Collapse a", EXPANDABLE, false),
    ] {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control(label, move || expand(&fixture, &last, name, expanded));
    }
    let appender = fixture.clone();
    let append_log = last.clone();
    ui.add_control("Append a child to a's store", move || {
        let text = match appender.append_child() {
            Some(object) => {
                let found = appender.find(&object.string());
                format!(
                    "appended {}: found at {:?} by a linear walk, n_items {}",
                    object.string(),
                    found.map(|(position, _)| position),
                    appender.tree.n_items()
                )
            }
            None => "a has no store yet: expand it first".to_owned(),
        };
        append_log.0.replace(text);
    });
    ui.set_readout(move || {
        format!(
            "create calls {}   n_items {}\n{}",
            fixture.create_calls(),
            fixture.tree.n_items(),
            last.0.borrow()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(22), build)
}
