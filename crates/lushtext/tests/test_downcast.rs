use gtk4::prelude::*;
use lushtext_core::ui::sidebar::workspace_section::LushtextWorkspaceSection;

fn main() {
    gtk4::init().unwrap();
    let section = LushtextWorkspaceSection::default();
    
    let w = section.upcast::<gtk4::Widget>();
    if let Some(s) = w.downcast_ref::<LushtextWorkspaceSection>() {
        println!("Found section!");
    } else {
        println!("Failed to downcast!");
    }
}
