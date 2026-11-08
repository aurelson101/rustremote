use gtk::prelude::*;
pub fn image(name: &str) -> gtk::Image {
    let image = gtk::Image::from_resource(&format!("/com/rustremote/icons/{name}.svg"));
    image.set_size_request(20, 20);
    image.update_property(&[gtk::accessible::Property::Label("")]);
    image
}
