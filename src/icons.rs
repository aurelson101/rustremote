use gtk::prelude::*;
pub fn image(name: &str) -> gtk::Image {
    let image = gtk::Image::from_resource(&format!("/com/rustremote/icons/{name}.svg"));
    image.set_size_request(20, 20);
    image.update_property(&[gtk::accessible::Property::Label("")]);
    image
}

/// Theme icon names with a bundled equivalent. macOS (and minimal Linux
/// desktops) ship no Adwaita theme, so these would render as missing icons.
const THEMED: &[(&str, &str)] = &[
    ("utilities-terminal", "ssh"),
    ("network-wired", "switch"),
    ("network-transmit-receive", "forward"),
    ("video-display", "vnc"),
    ("folder-remote", "folder"),
];

pub fn themed(name: &str) -> gtk::Image {
    match THEMED.iter().find(|(theme, _)| *theme == name) {
        Some((_, bundled)) => image(bundled),
        None => gtk::Image::from_icon_name(name),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn themed_icons_exist_in_the_bundled_resources() {
        let manifest = include_str!("../assets/icons.gresource.xml");
        for name in super::THEMED.iter().map(|(_, b)| b).chain(&["tunnel"]) {
            assert!(
                manifest.contains(&format!("alias=\"{name}.svg\"")),
                "{name}.svg missing from icons.gresource.xml"
            );
        }
    }
}
