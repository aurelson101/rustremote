//! Capture only this GTK window, never the user's desktop or other applications.
use gtk::prelude::*;
use std::{io::Write, path::Path};

pub fn window(window: &gtk::ApplicationWindow, path: &Path) -> Result<(), String> {
    if !window.is_mapped() || window.width() <= 0 || window.height() <= 0 {
        return Err("Cannot capture an unmapped window".into());
    }
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    let node = snapshot.to_node().ok_or("Empty GTK snapshot")?;
    let renderer = window.renderer().ok_or("GTK renderer unavailable")?;
    let texture = renderer.render_texture(&node, None);
    let bytes = texture.save_to_png_bytes();
    // This optional test capture cannot silently overwrite an existing file.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Cannot create GTK capture (parent missing or file already exists)")?;
    file.write_all(&bytes)
        .map_err(|_| "Cannot write GTK capture".to_string())
}
