use crate::profiles::SessionProfile;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    rc::Rc,
};
use vte::prelude::*;

/// Un enfant exclusivement créé ici ; pas de PID issu d'un profil ou d'une entrée utilisateur.
#[derive(Clone)]
pub struct Session(Rc<SessionInner>);
struct SessionInner {
    pid: Cell<Option<gtk::glib::Pid>>,
    cancelled: Cell<bool>,
    cancel: gtk::gio::Cancellable,
    terminal: gtk::glib::WeakRef<vte::Terminal>,
    pidfd: RefCell<Option<OwnedFd>>,
    sftp_windows: RefCell<Vec<gtk::glib::WeakRef<gtk::Window>>>,
    profile: SessionProfile,
    forward: bool,
    transfer_store: RefCell<Option<crate::store::Store>>,
    preferences: Rc<RefCell<crate::preferences::Preferences>>,
    control: Option<tempfile::TempDir>,
    credential: RefCell<Option<std::sync::Arc<crate::credentials::Secret>>>,
    auth: RefCell<Option<crate::ssh_auth::Bridge>>,
}
impl Session {
    pub fn zoom(&self, direction: i32) -> Option<f64> {
        let terminal = self.0.terminal.upgrade()?;
        let scale = if direction == 0 {
            1.0
        } else {
            (terminal.font_scale() + f64::from(direction) * 0.1).clamp(0.6, 2.0)
        };
        terminal.set_font_scale(scale);
        Some(scale)
    }
    pub fn transfer_store(&self, store: Option<crate::store::Store>) {
        self.0.transfer_store.replace(store);
    }
    pub fn configure(&self, preferences: &crate::preferences::Preferences) {
        self.0.preferences.replace(preferences.clone());
        if let Some(t) = self.0.terminal.upgrade() {
            configure(&t, preferences);
        }
    }
    pub fn active(&self) -> bool {
        !self.0.cancelled.get()
    }
    pub fn can_sftp(&self) -> bool {
        self.active()
            && !self.0.forward
            && matches!(
                self.0.profile.options,
                crate::profiles::ProtocolOptions::Ssh { .. }
            )
    }
    pub fn selected(&self, tabs: &gtk::Notebook) -> bool {
        self.0.terminal.upgrade().is_some_and(|t| {
            tabs.current_page()
                .and_then(|i| tabs.nth_page(Some(i)))
                .is_some_and(|p| t.is_ancestor(&p))
        })
    }
    pub fn can_dispatch(&self) -> bool {
        use std::os::unix::fs::FileTypeExt;
        self.can_sftp()
            && self.0.control.as_ref().is_some_and(|directory| {
                std::fs::symlink_metadata(directory.path().join("control"))
                    .is_ok_and(|m| m.file_type().is_socket())
            })
    }
    pub fn is_tunnel(&self) -> bool {
        self.0.forward
    }
    pub fn target(&self) -> String {
        format!(
            "{} — {}@{}:{}",
            self.0.profile.name, self.0.profile.username, self.0.profile.host, self.0.profile.port
        )
    }
    pub fn send_command(&self, value: &str) -> Result<(), String> {
        crate::tools::command(value, 1)?;
        if !self.can_dispatch() {
            return Err("La session SSH n’est pas encore authentifiée ou n’est plus active".into());
        }
        let terminal = self.0.terminal.upgrade().ok_or("Terminal fermé")?;
        terminal.feed_child(format!("{value}\r").as_bytes());
        Ok(())
    }
    pub fn open_sftp(&self) {
        if !self.can_sftp() {
            return;
        }
        if let Some(window) = self
            .0
            .sftp_windows
            .borrow()
            .iter()
            .filter_map(|w| w.upgrade())
            .find(|w| w.is_visible())
        {
            crate::i18n::present(&window);
            return;
        }
        if let Some(t) = self.0.terminal.upgrade()
            && let Some(parent) = t.root().and_downcast::<gtk::Window>()
        {
            let w = crate::sftp_ui::open(
                &parent,
                &self.0.profile,
                self.0.credential.borrow().clone(),
                self.0.transfer_store.borrow().clone(),
                self.0.preferences.clone(),
            );
            self.0
                .sftp_windows
                .borrow_mut()
                .retain(|w| w.upgrade().is_some());
            self.0.sftp_windows.borrow_mut().push(w.downgrade());
        }
    }
    pub fn present(&self, tabs: &gtk::Notebook) {
        if let Some(t) = self.0.terminal.upgrade() {
            for i in 0..tabs.n_pages() {
                if let Some(page) = tabs.nth_page(Some(i))
                    && t.is_ancestor(&page)
                {
                    tabs.set_current_page(Some(i));
                    break;
                }
            }
            t.grab_focus();
        }
    }
    pub fn disconnect(&self) {
        self.0.transfer_store.take();
        self.0.auth.borrow_mut().take();
        self.0.credential.borrow_mut().take();
        self.0.cancelled.set(true);
        self.0.cancel.cancel();
        for window in self
            .0
            .sftp_windows
            .borrow()
            .iter()
            .filter_map(|w| w.upgrade())
        {
            window.close();
        }
        if let Some(fd) = self.0.pidfd.borrow_mut().take() {
            // pidfd : aucune possibilité de signaler un PID réutilisé par le système.
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    libc::SIGHUP,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
        if let Some(terminal) = self.0.terminal.upgrade() {
            terminal.set_input_enabled(false);
        }
    }
}
impl Drop for SessionInner {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(fd) = self.pidfd.get_mut().take() {
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    libc::SIGHUP,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
    }
}

pub fn open(
    tabs: &gtk::Notebook,
    profile: &SessionProfile,
    status: &gtk::Label,
    preferences: &crate::preferences::Preferences,
    credential: Option<std::sync::Arc<crate::credentials::Secret>>,
) -> Result<Session, String> {
    open_internal(tabs, profile, status, preferences, credential, None)
}
pub fn open_tunnel(
    tabs: &gtk::Notebook,
    profile: &SessionProfile,
    status: &gtk::Label,
    preferences: &crate::preferences::Preferences,
    credential: Option<std::sync::Arc<crate::credentials::Secret>>,
    forward: &crate::tools::Forward,
) -> Result<Session, String> {
    open_internal(
        tabs,
        profile,
        status,
        preferences,
        credential,
        Some(forward),
    )
}
fn open_internal(
    tabs: &gtk::Notebook,
    profile: &SessionProfile,
    status: &gtk::Label,
    preferences: &crate::preferences::Preferences,
    credential: Option<std::sync::Arc<crate::credentials::Secret>>,
    forward: Option<&crate::tools::Forward>,
) -> Result<Session, String> {
    let (binary, mut args) = profile.command()?;
    let control = if matches!(
        profile.options,
        crate::profiles::ProtocolOptions::Ssh { .. }
    ) {
        let directory = tempfile::Builder::new()
            .prefix("rr-ssh-control-")
            .tempdir()
            .map_err(|_| "Contrôle d’authentification SSH indisponible")?;
        args.splice(
            0..0,
            [
                "-o".into(),
                "ControlMaster=yes".into(),
                "-o".into(),
                "ControlPersist=no".into(),
                "-o".into(),
                format!("ControlPath={}", directory.path().join("control").display()),
            ],
        );
        Some(directory)
    } else {
        None
    };
    if let Some(forward) = forward {
        if !matches!(
            profile.options,
            crate::profiles::ProtocolOptions::Ssh { .. }
        ) {
            return Err("Un tunnel nécessite un profil SSH".into());
        }
        if let Some(i) = args.iter().position(|v| v == "-F") {
            args.drain(i..i + 2);
        }
        args.splice(0..0, forward.arguments()?);
    }
    if !matches!(
        profile.options,
        crate::profiles::ProtocolOptions::Ssh { .. }
            | crate::profiles::ProtocolOptions::Serial { .. }
    ) {
        return Err("Ce protocole n’est pas un terminal.".into());
    }
    if !std::path::Path::new(binary).is_file() {
        return Err("Client SSH ou série absent du système.".into());
    }
    let auth = if matches!(
        profile.options,
        crate::profiles::ProtocolOptions::Ssh { .. }
    ) {
        if let Some(secret) = credential.clone() {
            let parent = tabs
                .root()
                .and_downcast::<gtk::Window>()
                .ok_or("Fenêtre du terminal absente")?;
            args.splice(
                0..0,
                [
                    "-o",
                    "NumberOfPasswordPrompts=1",
                    "-o",
                    "BatchMode=no",
                    "-o",
                    "PasswordAuthentication=yes",
                ]
                .map(String::from),
            );
            Some(crate::ssh_auth::Bridge::new(&parent, secret)?)
        } else {
            None
        }
    } else {
        None
    };
    let terminal = vte::Terminal::new();
    terminal.set_hexpand(true);
    terminal.set_vexpand(true);
    configure(&terminal, preferences);
    terminal.set_allow_hyperlink(false);
    terminal.set_enable_sixel(false);
    terminal.set_audible_bell(false);
    terminal.update_property(&[gtk::accessible::Property::Label(&format!(
        "Terminal — {}",
        profile.name
    ))]);
    let session = Session(Rc::new(SessionInner {
        pid: Cell::new(None),
        pidfd: RefCell::new(None),
        sftp_windows: RefCell::new(Vec::new()),
        profile: profile.clone(),
        forward: forward.is_some(),
        transfer_store: RefCell::new(None),
        preferences: Rc::new(RefCell::new(preferences.clone())),
        control,
        credential: RefCell::new(credential),
        auth: RefCell::new(auth),
        cancelled: Cell::new(false),
        cancel: gtk::gio::Cancellable::new(),
        terminal: terminal.downgrade(),
    }));
    let page = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let local_status = gtk::Label::new(Some("Ouverture du terminal…"));
    local_status.set_hexpand(true);
    local_status.set_xalign(0.0);
    page.append(&local_status);
    let copy = gtk::Button::with_label("Copier la sélection");
    let t = terminal.downgrade();
    let s = local_status.downgrade();
    copy.connect_clicked(move |_| {
        if let (Some(t), Some(s)) = (t.upgrade(), s.upgrade()) {
            copy_selected(&t, &s);
        }
    });
    let s = local_status.downgrade();
    terminal.connect_copy_clipboard(move |t| {
        t.stop_signal_emission_by_name("copy-clipboard");
        if let Some(s) = s.upgrade() {
            copy_selected(t, &s);
        }
    });
    let actions = gtk::gio::SimpleActionGroup::new();
    for (name, _title) in [
        ("copy", "Copier"),
        ("paste", "Coller"),
        ("select-all", "Tout sélectionner"),
    ] {
        let action = gtk::gio::SimpleAction::new(name, None);
        let t = terminal.downgrade();
        let s = local_status.downgrade();
        action.connect_activate(move |_, _| {
            if let (Some(t), Some(s)) = (t.upgrade(), s.upgrade()) {
                match name {
                    "copy" => copy_selected(&t, &s),
                    "paste" => paste_checked(&t, &s),
                    _ => t.select_all(),
                }
            }
        });
        if name == "copy" {
            action.set_enabled(terminal.has_selection());
            let a = action.clone();
            terminal.connect_selection_changed(move |t| a.set_enabled(t.has_selection()));
        } else if name == "paste" {
            terminal
                .bind_property("input-enabled", &action, "enabled")
                .sync_create()
                .build();
        }
        actions.add_action(&action);
    }
    terminal.insert_action_group("terminal", Some(&actions));
    page.insert_action_group("terminal", Some(&actions));
    let context_click = gtk::GestureClick::new();
    context_click.set_button(3);
    context_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let target = page.downgrade();
    let region = terminal.downgrade();
    context_click.connect_pressed(move |gesture, _, x, y| {
        if let (Some(target), Some(region)) = (target.upgrade(), region.upgrade())
            && let Some(bounds) = region.compute_bounds(&target)
            && x >= f64::from(bounds.x())
            && x <= f64::from(bounds.x() + bounds.width())
            && y >= f64::from(bounds.y())
            && y <= f64::from(bounds.y() + bounds.height())
        {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let popup = gtk::Popover::new();
            let body = gtk::Box::new(gtk::Orientation::Vertical, 3);
            for (name, title) in [
                ("copy", "Copier"),
                ("paste", "Coller"),
                ("select-all", "Tout sélectionner"),
            ] {
                let item = gtk::Button::with_label(title);
                item.add_css_class("menu-item");
                if let Some(action) = actions.lookup_action(name) {
                    action
                        .bind_property("enabled", &item, "sensitive")
                        .sync_create()
                        .build();
                    let weak = popup.downgrade();
                    item.connect_clicked(move |_| {
                        action.activate(None);
                        if let Some(p) = weak.upgrade() {
                            p.popdown();
                        }
                    });
                }
                body.append(&item);
            }
            popup.set_child(Some(&body));
            popup.set_parent(&target);
            popup.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popup.connect_closed(crate::dialogs::defer_unparent);
            crate::i18n::translate(popup.upcast_ref());
            popup.popup();
        }
    });
    // Capturer sur le parent avant les contrôleurs souris internes de VTE.
    page.add_controller(context_click);

    tools.append(&copy);
    let select = gtk::Button::with_label("Tout sélectionner");
    let t = terminal.downgrade();
    select.connect_clicked(move |_| {
        if let Some(t) = t.upgrade() {
            t.select_all();
        }
    });
    tools.append(&select);
    let paste = gtk::Button::with_label("Coller dans le terminal…");
    terminal
        .bind_property("input-enabled", &paste, "sensitive")
        .sync_create()
        .build();
    let t = terminal.downgrade();
    let s = local_status.downgrade();
    paste.connect_clicked(move |_| {
        if let (Some(t), Some(s)) = (t.upgrade(), s.upgrade()) {
            paste_checked(&t, &s);
        }
    });
    tools.append(&paste);
    if session.can_sftp() {
        let sftp = gtk::Button::with_label("Ouvrir SFTP");
        terminal
            .bind_property("input-enabled", &sftp, "sensitive")
            .sync_create()
            .build();
        let child = Rc::downgrade(&session.0);
        sftp.connect_clicked(move |_| {
            if let Some(child) = child.upgrade() {
                Session(child).open_sftp();
            }
        });
        tools.append(&sftp);
    }
    let s = local_status.downgrade();
    terminal.connect_paste_clipboard(move |terminal| {
        terminal.stop_signal_emission_by_name("paste-clipboard");
        if let Some(s) = s.upgrade() {
            paste_checked(terminal, &s);
        }
    });
    let t = terminal.downgrade();
    let s = local_status.downgrade();
    crate::clipboard::controlled_primary_paste(&terminal, move |clipboard| {
        if let (Some(t), Some(s)) = (t.upgrade(), s.upgrade()) {
            paste_from(&t, &s, clipboard);
        }
    });
    let disconnect = gtk::Button::with_label("Déconnecter le terminal");
    terminal
        .bind_property("input-enabled", &disconnect, "sensitive")
        .sync_create()
        .build();
    let child = session.clone();
    disconnect.connect_clicked(move |_| child.disconnect());
    tools.append(&disconnect);
    page.append(&tools);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&terminal)
        .vexpand(true)
        .hexpand(true)
        .build();
    page.append(&scroll);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.append(&crate::icons::image(
        if matches!(
            profile.options,
            crate::profiles::ProtocolOptions::Serial { .. }
        ) {
            "switch"
        } else {
            "ssh"
        },
    ));
    let title = gtk::Label::new(Some(&format!("{} · {:?}", profile.name, profile.kind())));
    header.append(&title);
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.set_tooltip_text(Some("Fermer et déconnecter ce terminal"));
    header.append(&close);
    let n = tabs.downgrade();
    let p = page.downgrade();
    let child = session.clone();
    close.connect_clicked(move |_| {
        child.disconnect();
        if let (Some(n), Some(p)) = (n.upgrade(), p.upgrade())
            && let Some(i) = n.page_num(&p)
        {
            n.remove_page(Some(i));
        }
    });
    let child = session.clone();
    page.connect_unrealize(move |_| child.disconnect());
    let child = Rc::downgrade(&session.0);
    let s = local_status.downgrade();
    let global = status.downgrade();
    let name = profile.name.clone();
    terminal.connect_child_exited(move |terminal, exit| {
        if let Some(child) = child.upgrade() {
            child.auth.borrow_mut().take();
            child.credential.borrow_mut().take();
            child.pid.set(None);
            child.pidfd.replace(None);
            child.cancelled.set(true);
            for window in child
                .sftp_windows
                .borrow()
                .iter()
                .filter_map(|w| w.upgrade())
            {
                window.close();
            }
        }
        terminal.set_input_enabled(false);
        let message = format!("{name} · session terminée (statut {exit})");
        if let Some(s) = s.upgrade() {
            s.set_text(&message);
        }
        if let Some(s) = global.upgrade() {
            s.set_text(&message);
        }
    });
    crate::i18n::translate(page.upcast_ref());
    crate::i18n::translate(header.upcast_ref());
    let index = tabs.append_page(&page, Some(&header));
    tabs.set_tab_reorderable(&page, true);
    tabs.set_current_page(Some(index));
    terminal.grab_focus();
    let mut argv = vec![binary.to_string()];
    argv.extend(args);
    let argv = argv.iter().map(String::as_str).collect::<Vec<_>>();
    let mut environment = std::env::vars()
        .filter(|(key, _)| {
            ![
                "LD_LIBRARY_PATH",
                "LD_PRELOAD",
                "GDK_PIXBUF_MODULE_FILE",
                "GIO_MODULE_DIR",
                "GTK_PATH",
                "SSH_ASKPASS",
                "SSH_ASKPASS_REQUIRE",
                "VNC_VIA_CMD",
                "RUSTREMOTE_ASKPASS_SOCKET",
                "SSH_ASKPASS_PROMPT",
            ]
            .contains(&key.as_str())
        })
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>();
    environment.retain(|s| !s.starts_with("TERM="));
    environment.push("TERM=xterm-256color".into());
    if let Some(auth) = session.0.auth.borrow().as_ref() {
        let helper = std::env::current_exe()
            .map_err(|_| "Composant askpass introuvable")?
            .with_file_name("rustremote-askpass");
        environment.push(format!("SSH_ASKPASS={}", helper.display()));
        environment.push("SSH_ASKPASS_REQUIRE=force".into());
        environment.push(format!("RUSTREMOTE_ASKPASS_SOCKET={}", auth.path.display()));
    }
    let env = environment.iter().map(String::as_str).collect::<Vec<_>>();
    let child = session.clone();
    let t = terminal.downgrade();
    let s = local_status.downgrade();
    let g = status.downgrade();
    let ssh = matches!(
        profile.options,
        crate::profiles::ProtocolOptions::Ssh { .. }
    );
    terminal.spawn_async(
        vte::PtyFlags::DEFAULT,
        None,
        &argv,
        &env,
        gtk::glib::SpawnFlags::from_bits_retain(
            (vte::ffi::VTE_SPAWN_NO_PARENT_ENVV | vte::ffi::VTE_SPAWN_NO_SYSTEMD_SCOPE) as u32,
        ),
        || {},
        15000,
        Some(&session.0.cancel),
        move |result| match result {
            Ok(pid) => {
                child.0.pid.set(Some(pid));
                if let Some(auth) = child.0.auth.borrow().as_ref() {
                    auth.pid
                        .store(pid.0 as u32, std::sync::atomic::Ordering::Relaxed);
                }
                let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid.0, 0) };
                if fd >= 0 {
                    child
                        .0
                        .pidfd
                        .replace(Some(unsafe { OwnedFd::from_raw_fd(fd as i32) }));
                }
                if child.0.cancelled.get() {
                    child.disconnect();
                } else {
                    if let Some(s) = s.upgrade() {
                        s.set_text(if child.0.forward { "Tunnel SSH lancé · écoute loopback demandée, déconnecter pour l’arrêter" } else if ssh {
                            "Terminal actif · authentification et empreinte dans le terminal"
                        } else {
                            "Liaison série active · port local, sans authentification SSH"
                        });
                    }
                    if let Some(g) = g.upgrade() {
                        g.set_text(if ssh {
                            "Terminal intégré actif · validation SSH contrôlée par OpenSSH"
                        } else {
                            "Série locale active · liaison non chiffrée"
                        });
                    }
                }
            }
            Err(_) => {
                child.0.cancelled.set(true);
                if let Some(s) = s.upgrade() {
                    s.set_text("Impossible d’ouvrir le terminal");
                }
                if let Some(t) = t.upgrade() {
                    t.set_input_enabled(false);
                }
            }
        },
    );
    terminal.grab_focus();
    Ok(session)
}

fn configure(terminal: &vte::Terminal, preferences: &crate::preferences::Preferences) {
    terminal.set_scrollback_lines(preferences.scrollback as i64);
    terminal.set_font(Some(&gtk::pango::FontDescription::from_string(&format!(
        "Monospace {}",
        preferences.font_size
    ))));
    let bg = if preferences.black_terminal {
        gtk::gdk::RGBA::new(0., 0., 0., 1.)
    } else {
        gtk::gdk::RGBA::new(0.05, 0.075, 0.11, 1.)
    };
    terminal.set_colors(
        Some(&gtk::gdk::RGBA::new(0.85, 0.89, 0.94, 1.)),
        Some(&bg),
        &[],
    );
}
fn copy_selected(terminal: &vte::Terminal, status: &gtk::Label) {
    if let Some(text) = terminal.text_selected(vte::Format::Text) {
        if text.len() > 65536 {
            status.set_text("Copie limitée à 64 Kio ; sélectionnez moins de texte");
            return;
        }
        terminal.clipboard().set_text(&text);
        status.set_text("Sélection copiée dans le presse-papiers");
    } else {
        status.set_text("Sélectionnez du texte dans le terminal");
    }
}

fn paste_checked(terminal: &vte::Terminal, status: &gtk::Label) {
    paste_from(terminal, status, terminal.clipboard());
}
fn paste_from(terminal: &vte::Terminal, status: &gtk::Label, clipboard: gtk::gdk::Clipboard) {
    if !terminal.is_input_enabled() {
        return;
    }
    let t = terminal.downgrade();
    let s = status.downgrade();
    gtk::glib::MainContext::default().spawn_local(async move {
        let result=crate::clipboard::read_bounded(&clipboard,65536).await;
        let (Some(t),Some(s))=(t.upgrade(),s.upgrade()) else{return};
        if !t.is_input_enabled() {return;}
        let text=match result {Ok(text)=>text,Err(error)=>{s.set_text(error);return;}};
        if text.chars().any(|c|c.is_control() && c!='\n' && c!='\r' && c!='\t'){s.set_text("Collage refusé : caractères de contrôle");return;}
        if text.contains(['\n','\r']) {
            let dialog=gtk::Window::builder().title("Confirmer le collage multiligne").modal(true).default_width(440).build();
            if let Some(parent)=t.root().and_downcast::<gtk::Window>(){dialog.set_transient_for(Some(&parent));dialog.set_application(parent.application().as_ref());dialog.set_destroy_with_parent(true);}
            let body=gtk::Box::new(gtk::Orientation::Vertical,12);body.add_css_class("dialog-content");
            let warning=gtk::Label::new(Some("Ce texte contient des retours à la ligne qui peuvent exécuter des commandes. Coller uniquement si vous avez vérifié son contenu."));warning.set_wrap(true);body.append(&warning);
            let preview=gtk::TextView::builder().editable(false).cursor_visible(false).monospace(true).wrap_mode(gtk::WrapMode::WordChar).accepts_tab(false).build();
            preview.buffer().set_text(&text);
            preview.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text("Texte du collage à confirmer"))]);
            body.append(&gtk::ScrolledWindow::builder().child(&preview).min_content_height(100).max_content_height(160).propagate_natural_height(true).build());
            let confirm=gtk::Button::with_label("Confirmer le collage dans cette session");let terminal=t.downgrade();let d=dialog.downgrade();
            confirm.connect_clicked(move |_|{if let Some(t)=terminal.upgrade() && t.is_input_enabled(){t.paste_text(&text);}
                if let Some(d)=d.upgrade(){d.close();}});body.append(&confirm);
            let cancel=gtk::Button::with_label("Annuler le collage");let d=dialog.downgrade();cancel.connect_clicked(move |_|{if let Some(d)=d.upgrade(){d.close();}});body.append(&cancel);
            dialog.connect_hide(|w|w.set_child(None::<&gtk::Widget>));dialog.set_child(Some(&body));dialog.present();
        } else if t.is_input_enabled() {t.paste_text(&text);}
    });
}
