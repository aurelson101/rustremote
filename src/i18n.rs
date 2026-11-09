//! UI catalog. Source text is French; English is the default. User entries and
//! terminal/editor contents are never translated. Language changes apply on restart.
use gtk::prelude::*;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
static FRENCH: AtomicBool = AtomicBool::new(false);
fn catalog() -> &'static Vec<(&'static str, &'static str)> {
    static DATA: OnceLock<Vec<(&str, &str)>> = OnceLock::new();
    DATA.get_or_init(|| {
        include_str!("../assets/locales/en.tsv")
            .lines()
            .filter_map(|s| s.split_once('\t'))
            .collect()
    })
}
pub fn initialize() {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")));
    let french = root
        .and_then(|p| {
            crate::preferences::Preferences::load(&p.join("rustremote/settings.json")).ok()
        })
        .is_some_and(|p| p.language == "fr");
    // Explicit override also lets the isolated French accessibility labs keep their labels.
    let french = match std::env::var("RUSTREMOTE_LANGUAGE").as_deref() {
        Ok("fr") => true,
        Ok("en") => false,
        _ => french,
    };
    FRENCH.store(french, Ordering::Relaxed);
}
pub fn text(value: &str) -> String {
    if FRENCH.load(Ordering::Relaxed) {
        return value.into();
    }
    if value.contains('\n') {
        return value.split('\n').map(text).collect::<Vec<_>>().join("\n");
    }
    let trimmed = value.trim();
    if trimmed != value
        && let Some((_, en)) = catalog().iter().find(|(fr, _)| *fr == trimmed)
    {
        return value.replacen(trimmed, en, 1);
    }
    if let Some((bytes, entries)) = value.split_once(" octets · ")
        && bytes.parse::<u64>().is_ok()
        && let Some(entries) = entries.strip_suffix(" entrée(s)")
        && entries.parse::<u64>().is_ok()
    {
        return format!("{bytes} bytes · {entries} entries");
    }
    if let Some((fr, en)) = catalog()
        .iter()
        .find(|(fr, _)| fr.ends_with(": ") && value.starts_with(fr))
    {
        let rest = value[fr.len()..]
            .replace("octets vérifiés/transférés", "bytes checked/transferred")
            .replace("octets", "bytes")
            .replace("opérations", "operations")
            .replace("entrée(s)", "entries")
            .replace("élément(s)", "items")
            .replace("actualisez le listing", "refresh the listing");
        return format!("{en}{rest}");
    }
    catalog()
        .iter()
        .find_map(|(fr, en)| (*fr == value).then_some(*en))
        .unwrap_or(value)
        .to_string()
}
pub fn dropdown(values: &[&str]) -> gtk::DropDown {
    let items = values.iter().map(|v| text(v)).collect::<Vec<_>>();
    gtk::DropDown::from_strings(&items.iter().map(String::as_str).collect::<Vec<_>>())
}
fn property(widget: &gtk::Widget, name: &'static str) {
    if let Some(spec) = widget.find_property(name) {
        if spec.value_type() != String::static_type()
            || !spec
                .flags()
                .contains(gtk::glib::ParamFlags::READABLE | gtk::glib::ParamFlags::WRITABLE)
        {
            return;
        }
        if let Some(value) = widget.property::<Option<String>>(name) {
            let translated = text(&value);
            if translated != value {
                widget.set_property(name, Some(translated));
            }
        }
        if !widget.has_css_class("rr-localized") {
            widget.connect_notify_local(Some(name), move |w, _| {
                if let Some(value) = w.property::<Option<String>>(name) {
                    let translated = text(&value);
                    if translated != value {
                        w.set_property(name, Some(translated));
                    }
                }
            });
        }
    }
}
pub fn translate(widget: &gtk::Widget) {
    if widget.has_css_class("ui-text")
        && let Some(view) = widget.downcast_ref::<gtk::TextView>()
    {
        let b = view.buffer();
        let value = b.text(&b.start_iter(), &b.end_iter(), false);
        b.set_text(&text(&value));
    }
    // Do not translate filenames, folder names, hostnames or selected user text.
    if widget.has_css_class("user-content") {
        return;
    }
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && !widget.has_css_class("rr-localized")
    {
        expander.connect_expanded_notify(|e| {
            if e.is_expanded()
                && let Some(child) = e.child()
                && child.root().is_some()
            {
                translate(&child);
            }
        });
    }
    if widget.is::<gtk::ProgressBar>() {
        property(widget, "text");
    }
    for name in ["label", "title", "placeholder-text", "tooltip-text"] {
        property(widget, name);
    }
    widget.add_css_class("rr-localized");
    let mut child = widget.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        translate(&w);
    }
}
pub fn present(window: &impl IsA<gtk::Window>) {
    translate(window.as_ref().upcast_ref());
    window.present();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn english_default_catalog_has_unique_sources_and_preserves_user_input() {
        let mut seen = std::collections::HashSet::new();
        for (fr, en) in catalog() {
            assert!(seen.insert(fr), "Duplicate translation: {fr}");
            assert!(!en.is_empty());
        }
        assert_eq!(text("Paramètres"), "Settings");
        assert_eq!(text("my-host.example.invalid"), "my-host.example.invalid");
    }
}
