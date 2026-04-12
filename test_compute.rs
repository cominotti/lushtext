use gtk4::prelude::*;
fn main() {
    let w = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let p = w.compute_point(&w, &graphene::Point::new(0.0, 0.0));
}
