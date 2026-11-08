use gtk::prelude::*;

/// Empêche le collage PRIMARY implicite de contourner les limites du lecteur contrôlé.
pub fn controlled_primary_paste(
    widget: &impl IsA<gtk::Widget>,
    paste: impl Fn(gtk::gdk::Clipboard) + 'static,
) {
    let paste = std::rc::Rc::new(paste);
    let display = widget.display();
    let callback = paste.clone();
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    key.connect_key_pressed(move |_, key, _, state| {
        if key == gtk::gdk::Key::Insert && state.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
            callback(display.primary_clipboard());
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    widget.add_controller(key);
    let display = widget.display();
    let middle = gtk::GestureClick::new();
    middle.set_button(2);
    middle.set_propagation_phase(gtk::PropagationPhase::Capture);
    middle.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        paste(display.primary_clipboard());
    });
    widget.add_controller(middle);
}

/// Lecture en flux : la taille et la durée restent bornées même avec un propriétaire lent.
pub async fn read_bounded(
    clipboard: &gtk::gdk::Clipboard,
    limit: usize,
) -> Result<String, &'static str> {
    let read = async {
        let (stream, _) = clipboard
            .read_future(
                &["text/plain;charset=utf-8", "text/plain"],
                gtk::glib::Priority::DEFAULT,
            )
            .await
            .map_err(|_| "Presse-papiers texte indisponible")?;
        let mut bytes = Vec::new();
        loop {
            let chunk = stream
                .read_bytes_future(4096, gtk::glib::Priority::DEFAULT)
                .await
                .map_err(|_| "Lecture du presse-papiers impossible")?;
            if chunk.is_empty() {
                break;
            }
            if bytes.len() + chunk.len() > limit {
                return Err("Texte du presse-papiers trop volumineux");
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes).map_err(|_| "Le presse-papiers n’est pas du texte UTF-8")
    };
    gtk::glib::future_with_timeout(std::time::Duration::from_secs(2), read)
        .await
        .unwrap_or(Err(
            "Lecture du presse-papiers interrompue après 2 secondes",
        ))
}
