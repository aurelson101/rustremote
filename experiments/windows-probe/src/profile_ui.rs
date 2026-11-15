//! Native profile editor. Disk and vault operations never run on GTK's thread.
use crate::profile_service::{self, Backend, SecretChange};
use gtk::prelude::*;
use rustremote_core::profiles::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use zeroize::Zeroizing;

fn worker<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl FnOnce(Result<T, String>) + 'static,
) {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("windows-profiles".into())
        .spawn(move || {
            let _ = tx.send(job());
        })
        .is_err()
    {
        done(Err("Cannot start the profile worker".into()));
        return;
    }
    let mut done = Some(done);
    gtk::glib::timeout_add_local(Duration::from_millis(25), move || {
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return gtk::glib::ControlFlow::Continue,
            Err(_) => Err("Profile worker interrupted".into()),
        };
        if let Some(done) = done.take() {
            done(result);
        }
        gtk::glib::ControlFlow::Break
    });
}

struct Panel {
    backend: Arc<dyn Backend>,
    data: RefCell<Workspace>,
    busy: Cell<bool>,
    selected: Cell<Option<usize>>,
    window: gtk::glib::WeakRef<gtk::ApplicationWindow>,
    tabs: gtk::Notebook,
    list: gtk::ListBox,
    status: gtk::Label,
    encrypted: gtk::CheckButton,
    name: gtk::Entry,
    host: gtk::Entry,
    port: gtk::SpinButton,
    user: gtk::Entry,
    domain: gtk::Entry,
    folder: gtk::Entry,
    kind: gtk::DropDown,
    platform: gtk::DropDown,
    key: gtk::Entry,
    known: gtk::Entry,
    config: gtk::Entry,
    agent: gtk::CheckButton,
    password: gtk::PasswordEntry,
    forget: gtk::CheckButton,
    favorite: gtk::CheckButton,
    folder_source: gtk::Entry,
    folder_destination: gtk::Entry,
    folder_list: gtk::Label,
}
impl Panel {
    fn busy(&self, busy: bool) {
        self.busy.set(busy);
        if let Some(window) = self.window.upgrade() {
            if busy {
                gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
            }
            if let Some(child) = window.child() {
                child.set_sensitive(!busy);
            }
        }
    }
    fn refresh(&self) {
        self.folder_list
            .set_text(&self.data.borrow().folders.join("\n"));
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for profile in &self.data.borrow().profiles {
            let label = gtk::Label::new(Some(&format!(
                "{}{}   {} · {}:{}",
                if profile.favorite { "★ " } else { "" },
                profile.name,
                if profile.folder.is_empty() {
                    "Root"
                } else {
                    &profile.folder
                },
                profile.host,
                profile.port
            )));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_margin_top(10);
            label.set_margin_bottom(10);
            self.list.append(&label);
        }
    }
    fn fill(&self, index: Option<usize>) {
        self.selected.set(index);
        self.password.set_text("");
        self.forget.set_active(false);
        let data = self.data.borrow();
        let profile = index.and_then(|i| data.profiles.get(i));
        self.name.set_text(profile.map_or("", |p| p.name.as_str()));
        self.host.set_text(profile.map_or("", |p| p.host.as_str()));
        self.port.set_value(profile.map_or(22.0, |p| p.port as f64));
        self.user
            .set_text(profile.map_or("", |p| p.username.as_str()));
        self.folder
            .set_text(profile.map_or("", |p| p.folder.as_str()));
        self.favorite
            .set_active(profile.is_some_and(|p| p.favorite));
        self.platform
            .set_selected(match profile.map(|p| p.platform) {
                Some(Platform::Linux) => 1,
                Some(Platform::Windows) => 2,
                Some(Platform::MacOs) => 3,
                _ => 0,
            });
        self.domain.set_text("");
        self.key.set_text("");
        self.known.set_text("");
        self.config.set_text("");
        self.agent.set_active(true);
        match profile.map(|p| &p.options) {
            Some(ProtocolOptions::Ssh {
                identity_file,
                agent,
                known_hosts_file,
                config_file,
            }) => {
                self.kind.set_selected(0);
                self.agent.set_active(*agent);
                self.key.set_text(identity_file.as_deref().unwrap_or(""));
                self.known
                    .set_text(known_hosts_file.as_deref().unwrap_or(""));
                self.config.set_text(config_file.as_deref().unwrap_or(""));
            }
            Some(ProtocolOptions::Rdp { domain, .. }) => {
                self.kind.set_selected(1);
                self.domain.set_text(domain);
            }
            Some(_) => {
                self.kind.set_selected(gtk::INVALID_LIST_POSITION);
                self.status.set_text("This imported protocol is preserved but its Windows editor is not available yet");
            }
            None => self.kind.set_selected(0),
        }
        if profile.is_some_and(|p| p.credential_ref.is_some()) {
            self.status.set_text("Saved credential: leave the password blank to keep it. Identity changes require a new password.");
        }
    }
    fn profile(&self) -> Result<SessionProfile, String> {
        let data = self.data.borrow();
        let previous = self.selected.get().and_then(|i| data.profiles.get(i));
        let optional = |entry: &gtk::Entry| {
            let text = entry.text().trim().to_string();
            (!text.is_empty()).then_some(text)
        };
        let options = match self.kind.selected() {
            0 => ProtocolOptions::Ssh {
                identity_file: optional(&self.key),
                agent: self.agent.is_active(),
                known_hosts_file: optional(&self.known),
                config_file: optional(&self.config),
            },
            1 => match previous.map(|p| &p.options) {
                Some(options @ ProtocolOptions::Rdp { .. }) => {
                    let mut options = options.clone();
                    if let ProtocolOptions::Rdp { domain, .. } = &mut options {
                        *domain = self.domain.text().trim().into();
                    }
                    options
                }
                _ => ProtocolOptions::Rdp {
                    domain: self.domain.text().trim().into(),
                    width: 1280,
                    height: 720,
                    fullscreen: false,
                    clipboard: true,
                    clipboard_files: false,
                    settings: RdpSettings::default(),
                },
            },
            _ => return Err("Choose an available protocol".into()),
        };
        let profile = SessionProfile {
            name: self.name.text().trim().into(),
            host: self.host.text().trim().into(),
            port: self.port.value_as_int() as u16,
            username: if self.kind.selected() == 0 {
                ssh_identity(&self.user.text(), &self.domain.text())?
            } else {
                self.user.text().trim().into()
            },
            credential_ref: previous.and_then(|p| p.credential_ref.clone()),
            favorite: self.favorite.is_active(),
            folder: self.folder.text().trim().into(),
            platform: match self.platform.selected() {
                1 => Platform::Linux,
                2 => Platform::Windows,
                3 => Platform::MacOs,
                _ => Platform::Unknown,
            },
            simulation: previous.is_some_and(|p| p.simulation),
            options,
        };
        profile.validate()?;
        profile.validate_native_paths()?;
        Ok(profile)
    }
    fn persist(self: &Rc<Self>, data: Workspace, secret: Option<SecretChange>) {
        if self.busy.get() {
            return;
        }
        let saved_index = secret.as_ref().map(|change| change.index);
        let before = self.data.borrow().clone();
        let encrypted = self.encrypted.is_active();
        let backend = self.backend.clone();
        self.busy(true);
        self.status.set_text("Saving profiles…");
        let panel = self.clone();
        worker(
            move || profile_service::save(backend.as_ref(), &before, data, encrypted, secret),
            move |result| {
                panel.busy(false);
                match result {
                    Ok(saved) => {
                        *panel.data.borrow_mut() = saved.data;
                        panel.refresh();
                        panel.fill(saved_index);
                        panel.password.set_text("");
                        panel.forget.set_active(false);
                        panel
                            .status
                            .set_text(saved.warning.as_deref().unwrap_or("Profiles saved"));
                    }
                    Err(error) => panel.status.set_text(&error),
                }
            },
        );
    }
}

fn action(
    label: &str,
    bar: &gtk::Box,
    panel: &Rc<Panel>,
    call: impl Fn(&Rc<Panel>) + 'static,
) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_widget_name(label);
    if label == "Save connection" {
        button.add_css_class("suggested-action");
    }
    let weak = Rc::downgrade(panel);
    button.connect_clicked(move |_| {
        if let Some(panel) = weak.upgrade()
            && !panel.busy.get()
        {
            call(&panel);
        }
    });
    bar.append(&button);
    button
}
fn row(grid: &gtk::Grid, index: i32, label: &str, widget: &impl IsA<gtk::Widget>) {
    let text = gtk::Label::new(Some(label));
    text.set_xalign(0.0);
    text.set_wrap(true);
    grid.attach(&text, 0, index, 1, 1);
    widget.set_hexpand(true);
    grid.attach(widget, 1, index, 1, 1);
}
fn fields() -> gtk::Grid {
    gtk::Grid::builder()
        .row_spacing(10)
        .column_spacing(16)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build()
}

fn build(
    parent: &gtk::ApplicationWindow,
    backend: Arc<dyn Backend>,
    data: Workspace,
    encrypted: bool,
) -> (gtk::ApplicationWindow, Rc<Panel>) {
    let tabs = gtk::Notebook::new();
    tabs.set_vexpand(true);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.add_css_class("profiles");
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let status = gtk::Label::new(Some(
        "Profiles are saved locally. Passwords remain in Windows Credential Manager.",
    ));
    status.set_wrap(true);
    status.set_selectable(true);
    let encryption =
        gtk::CheckButton::with_label("Encrypt profile metadata with Windows DPAPI (current user)");
    encryption.set_active(encrypted);
    root.append(&tabs);
    root.append(&encryption);
    root.append(&status);
    let window = gtk::ApplicationWindow::builder()
        .application(&parent.application().expect("application window"))
        .transient_for(parent)
        .title("RustRemote — Connections")
        .default_width(820)
        .default_height(760)
        .child(&root)
        .build();
    let panel = Rc::new(Panel {
        backend,
        data: RefCell::new(data),
        busy: Cell::new(false),
        selected: Cell::new(None),
        window: window.downgrade(),
        tabs: tabs.clone(),
        list: gtk::ListBox::new(),
        status: status.clone(),
        encrypted: encryption,
        name: gtk::Entry::new(),
        host: gtk::Entry::new(),
        port: gtk::SpinButton::with_range(1.0, 65535.0, 1.0),
        user: gtk::Entry::new(),
        domain: gtk::Entry::new(),
        folder: gtk::Entry::new(),
        kind: gtk::DropDown::from_strings(&["SSH / SFTP", "RDP / Windows"]),
        platform: gtk::DropDown::from_strings(&["Automatic", "Linux", "Windows", "macOS"]),
        key: gtk::Entry::new(),
        known: gtk::Entry::new(),
        config: gtk::Entry::new(),
        agent: gtk::CheckButton::with_label("Use SSH agent"),
        password: gtk::PasswordEntry::new(),
        forget: gtk::CheckButton::with_label("Forget saved password"),
        favorite: gtk::CheckButton::with_label("Favorite"),
        folder_source: gtk::Entry::new(),
        folder_destination: gtk::Entry::new(),
        folder_list: gtk::Label::new(None),
    });
    panel.password.set_show_peek_icon(true);
    let weak = Rc::downgrade(&panel);
    panel.kind.connect_selected_notify(move |_| {
        if let Some(p) = weak.upgrade() {
            let ssh = p.kind.selected() == 0;
            for field in [&p.key, &p.known, &p.config] {
                field.set_sensitive(ssh);
            }
            p.agent.set_sensitive(ssh);
            if p.selected.get().is_none() {
                if !ssh && p.port.value_as_int() == 22 {
                    p.port.set_value(3389.0);
                }
                if ssh && p.port.value_as_int() == 3389 {
                    p.port.set_value(22.0);
                }
            }
        }
    });
    let connections = gtk::Box::new(gtk::Orientation::Vertical, 12);
    connections.append(
        &gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&panel.list)
            .build(),
    );
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    connections.append(&bar);
    tabs.append_page(&connections, Some(&gtk::Label::new(Some("Connections"))));
    action("New connection", &bar, &panel, |p| {
        p.fill(None);
        p.tabs.set_current_page(Some(1));
    });
    action("Edit selected", &bar, &panel, |p| {
        if let Some(row) = p.list.selected_row() {
            p.fill(Some(row.index() as usize));
            p.tabs.set_current_page(Some(1));
        } else {
            p.status.set_text("Select a connection first");
        }
    });
    action("Delete selected…", &bar, &panel, |p| {
        let Some(row) = p.list.selected_row() else {
            p.status.set_text("Select a connection first");
            return;
        };
        let index = row.index() as usize;
        let dialog = gtk::MessageDialog::builder()
            .transient_for(&p.window.upgrade().unwrap())
            .modal(true)
            .text("Delete this saved connection?")
            .secondary_text("The remote server and files are not changed.")
            .buttons(gtk::ButtonsType::OkCancel)
            .build();
        let weak = Rc::downgrade(p);
        dialog.connect_response(move |d, response| {
            d.close();
            if response == gtk::ResponseType::Ok
                && let Some(p) = weak.upgrade()
                && !p.busy.get()
            {
                let mut data = p.data.borrow().clone();
                if index < data.profiles.len() {
                    data.profiles.remove(index);
                    p.persist(data, None);
                }
            }
        });
        dialog.present();
    });
    let form = fields();
    let entry_fields: [(&str, &gtk::Entry); 8] = [
        ("Name", &panel.name),
        ("Host", &panel.host),
        ("User", &panel.user),
        ("Domain / SSH realm", &panel.domain),
        ("Session folder (logical path)", &panel.folder),
        ("SSH private key (absolute path)", &panel.key),
        ("SSH known_hosts (optional)", &panel.known),
        ("SSH config (optional)", &panel.config),
    ];
    row(&form, 0, "Protocol", &panel.kind);
    row(&form, 1, "Platform", &panel.platform);
    for (index, (label, entry)) in entry_fields.iter().enumerate() {
        if index >= 5 {
            let path = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            path.append(*entry);
            let browse = gtk::Button::with_label("Browse…");
            let entry = entry.downgrade();
            let parent = window.downgrade();
            browse.connect_clicked(move |_| {
                let Some(parent) = parent.upgrade() else {
                    return;
                };
                let chooser = gtk::FileChooserNative::builder()
                    .transient_for(&parent)
                    .title("Choose an SSH file")
                    .action(gtk::FileChooserAction::Open)
                    .build();
                let entry = entry.clone();
                chooser.connect_response(move |chooser, response| {
                    if response == gtk::ResponseType::Accept
                        && let Some(path) = chooser.file().and_then(|file| file.path())
                        && let Some(path) = path.to_str()
                        && let Some(entry) = entry.upgrade()
                    {
                        entry.set_text(path);
                    }
                });
                chooser.show();
            });
            path.append(&browse);
            row(&form, index as i32 + 2, label, &path);
        } else {
            row(&form, index as i32 + 2, label, *entry);
        }
    }
    row(&form, 10, "Port", &panel.port);
    row(&form, 11, "Authentication", &panel.agent);
    row(&form, 12, "Password / key passphrase", &panel.password);
    row(&form, 13, "", &panel.forget);
    row(&form, 14, "", &panel.favorite);
    let savebar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    form.attach(&savebar, 0, 15, 2, 1);
    action("Save connection", &savebar, &panel, |p| match p.profile() {
        Err(error) => p.status.set_text(&error),
        Ok(profile) => {
            let mut data = p.data.borrow().clone();
            if !profile.folder.is_empty() && !data.folders.contains(&profile.folder) {
                p.status
                    .set_text("Create the destination folder in the Folders tab first");
                return;
            }
            let index = match p.selected.get() {
                Some(i) if i < data.profiles.len() => {
                    data.profiles[i] = profile;
                    i
                }
                _ => {
                    data.profiles.push(profile);
                    data.profiles.len() - 1
                }
            };
            if let Err(error) = data.validate() {
                p.status.set_text(&error);
                return;
            }
            let password = Zeroizing::new(p.password.text().to_string());
            p.password.set_text("");
            p.persist(
                data,
                Some(SecretChange {
                    index,
                    value: (!password.is_empty()).then_some(password),
                    forget: p.forget.is_active(),
                }),
            );
        }
    });
    action("New / clear", &savebar, &panel, |p| p.fill(None));
    tabs.append_page(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&form)
            .build(),
        Some(&gtk::Label::new(Some("Profile"))),
    );
    let folders = fields();
    let current = panel.folder_source.clone();
    let destination = panel.folder_destination.clone();
    current.set_placeholder_text(Some("Example: Windows/Production"));
    destination.set_placeholder_text(Some("New path, or existing parent when moving"));
    row(&folders, 0, "Folder path", &current);
    row(&folders, 1, "Destination", &destination);
    let folder_list = panel.folder_list.clone();
    folder_list.set_selectable(true);
    folder_list.set_xalign(0.0);
    folder_list.set_wrap(true);
    folders.attach(&folder_list, 0, 3, 2, 1);
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    folders.attach(&bar, 0, 2, 2, 1);
    for mode in 0..4 {
        let current = current.clone();
        let destination = destination.clone();
        action(
            ["Create folder", "Rename", "Move folder", "Delete empty"][mode],
            &bar,
            &panel,
            move |p| {
                let mut data = p.data.borrow().clone();
                let old = current.text();
                let target = destination.text();
                let result = match mode {
                    0 => data.add_folder(old.trim()),
                    1 => data.rename_folder(old.trim(), target.trim()),
                    2 => data.move_item(&format!("folder:{}", old.trim()), target.trim()),
                    _ => data.remove_empty_folder(old.trim()),
                };
                match result {
                    Ok(()) => p.persist(data, None),
                    Err(error) => p.status.set_text(&error),
                }
            },
        );
    }
    let weak = Rc::downgrade(&panel);
    tabs.connect_switch_page(move |_, _, page| {
        if page == 2
            && let Some(p) = weak.upgrade()
        {
            folder_list.set_text(&p.data.borrow().folders.join("\n"));
        }
    });
    tabs.append_page(
        &gtk::ScrolledWindow::builder().child(&folders).build(),
        Some(&gtk::Label::new(Some("Folders"))),
    );
    let io = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    root.append(&io);
    for import in [true, false] {
        action(
            if import {
                "Import JSON…"
            } else {
                "Export JSON…"
            },
            &io,
            &panel,
            move |p| {
                let chooser = gtk::FileChooserNative::builder()
                    .title(if import {
                        "Import profiles"
                    } else {
                        "Export profiles without secrets"
                    })
                    .transient_for(&p.window.upgrade().unwrap())
                    .action(if import {
                        gtk::FileChooserAction::Open
                    } else {
                        gtk::FileChooserAction::Save
                    })
                    .build();
                if !import {
                    chooser.set_current_name("rustremote-profiles.json");
                }
                let weak = Rc::downgrade(p);
                chooser.connect_response(move |chooser, response| {
                    if response != gtk::ResponseType::Accept {
                        return;
                    }
                    let Some(p) = weak.upgrade() else {
                        return;
                    };
                    if p.busy.get() {
                        return;
                    }
                    let Some(path) = chooser.file().and_then(|f| f.path()) else {
                        p.status.set_text("Select a local file");
                        return;
                    };
                    if import {
                        let before = p.data.borrow().clone();
                        p.busy(true);
                        let panel = p.clone();
                        worker(
                            move || {
                                profile_service::import(&path)
                                    .and_then(|incoming| before.merge(&incoming))
                            },
                            move |result| {
                                panel.busy(false);
                                match result {
                                    Ok(data) => panel.persist(data, None),
                                    Err(error) => panel.status.set_text(&error),
                                }
                            },
                        );
                    } else {
                        let data = p.data.borrow().clone();
                        p.busy(true);
                        let panel = p.clone();
                        worker(
                            move || profile_service::export(&path, &data),
                            move |result| {
                                panel.busy(false);
                                panel.status.set_text(
                                    &result
                                        .map(|_| "Profiles exported without secrets".into())
                                        .unwrap_or_else(|e| e),
                                );
                            },
                        );
                    }
                });
                chooser.show();
            },
        );
    }
    action("Apply encryption choice", &io, &panel, |p| {
        p.persist(p.data.borrow().clone(), None)
    });
    let keep = panel.clone();
    window.connect_close_request(move |_| {
        if keep.busy.get() {
            keep.status
                .set_text("Wait for the current save to finish before closing");
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    panel.refresh();
    panel.fill(None);
    window.present();
    (window, panel)
}

pub fn attach(body: &gtk::Box, parent: &gtk::ApplicationWindow, status: &gtk::Label) {
    let button = gtk::Button::with_label("Manage connections…");
    button.set_sensitive(cfg!(windows));
    body.append(&button);
    let parent = parent.downgrade();
    let status = status.downgrade();
    let existing = Rc::new(RefCell::new(
        gtk::glib::WeakRef::<gtk::ApplicationWindow>::new(),
    ));
    button.connect_clicked(move |button| {
        if let Some(window) = existing.borrow().upgrade() {
            window.present();
            return;
        }
        let output = status.clone();
        let parent = parent.clone();
        let button_weak = button.downgrade();
        let existing = existing.clone();
        button.set_sensitive(false);
        worker(
            || {
                let backend = profile_service::open()?;
                let (data, encrypted) = backend.load()?;
                Ok((backend, data, encrypted))
            },
            move |result| {
                if let Some(button) = button_weak.upgrade() {
                    button.set_sensitive(true);
                }
                match result {
                    Ok((backend, data, encrypted)) => {
                        if let Some(parent) = parent.upgrade() {
                            let (window, _) = build(&parent, backend, data, encrypted);
                            *existing.borrow_mut() = window.downgrade();
                        }
                    }
                    Err(error) => {
                        if let Some(output) = output.upgrade() {
                            output.set_text(&error);
                        }
                    }
                }
            },
        );
    });
}

fn click(root: &impl IsA<gtk::Widget>, name: &str) -> Result<(), String> {
    fn find(widget: &gtk::Widget, name: &str) -> Option<gtk::Button> {
        if widget.widget_name() == name
            && let Ok(button) = widget.clone().downcast::<gtk::Button>()
        {
            return Some(button);
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(button) = find(&widget, name) {
                return Some(button);
            }
            child = widget.next_sibling();
        }
        None
    }
    find(root.as_ref(), name)
        .ok_or("Smoke button missing")?
        .emit_clicked();
    Ok(())
}

pub fn smoke(parent: &gtk::ApplicationWindow, result: Rc<Cell<bool>>, capture: bool) {
    let parent = parent.downgrade();
    worker(
        || {
            let backend = profile_service::laboratory()?;
            let (data, encrypted) = backend.load()?;
            let files = tempfile::tempdir().map_err(|_| "GUI fixture files unavailable")?;
            Ok((backend, data, encrypted, files))
        },
        move |loaded| {
            let Some(parent) = parent.upgrade() else {
                return;
            };
            let (backend, data, encrypted, files) = match loaded {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("{error}");
                    if let Some(app) = parent.application() {
                        app.quit();
                    }
                    return;
                }
            };
            let (window, panel) = build(&parent, backend, data, encrypted);
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            let mut phase = 0;
            let mut reference = None;
            let mut io_started = false;
            let mut capture_frame = -1;
            let app = parent.application();
            gtk::glib::timeout_add_local(Duration::from_millis(80), move || {
                let observed_phase = phase;
                let failure = |message: &str| {
                    eprintln!("Profile GUI phase {observed_phase}: {message}");
                    if let Some(app) = &app {
                        app.quit();
                    }
                };
                if std::time::Instant::now() > deadline {
                    failure(&format!("timeout ({})", panel.status.text()));
                    return gtk::glib::ControlFlow::Break;
                }
                if panel.busy.get() {
                    return gtk::glib::ControlFlow::Continue;
                }
                let next: Result<(), String> = (|| {
                    match phase {
                        0 => {
                            panel.tabs.set_current_page(Some(2));
                            panel.folder_source.set_text("Linux/Lab");
                            click(&window, "Create folder")?;
                            phase = 1;
                        }
                        1 => {
                            if !panel.data.borrow().folders.contains(&"Linux/Lab".into()) {
                                return Err(panel.status.text().into());
                            }
                            panel.fill(None);
                            panel.tabs.set_current_page(Some(1));
                            panel.name.set_text("SSH lab");
                            panel.host.set_text("127.0.0.1");
                            panel.user.set_text("lab");
                            panel.folder.set_text("Linux/Lab");
                            panel
                                .password
                                .set_text(&files.path().file_name().unwrap().to_string_lossy());
                            panel.encrypted.set_active(true);
                            click(&window, "Save connection")?;
                            phase = 2;
                        }
                        2 => {
                            reference = panel
                                .data
                                .borrow()
                                .profiles
                                .first()
                                .and_then(|p| p.credential_ref.clone());
                            if reference.is_none() {
                                return Err(panel.status.text().into());
                            }
                            panel.name.set_text("SSH renamed");
                            panel.favorite.set_active(true);
                            click(&window, "Save connection")?;
                            phase = 3;
                        }
                        3 => {
                            if panel.data.borrow().profiles.first().is_none_or(|p| {
                                p.name != "SSH renamed" || p.credential_ref != reference
                            }) {
                                return Err("Editing lost the existing credential".into());
                            }
                            if capture {
                                crate::capture::window(
                                    &window,
                                    std::path::Path::new("dist/windows-probe/profiles-form.png"),
                                )?;
                            }
                            panel.host.set_text("127.0.0.2");
                            click(&window, "Save connection")?;
                            phase = 4;
                        }
                        4 => {
                            if panel
                                .data
                                .borrow()
                                .profiles
                                .first()
                                .is_none_or(|p| p.credential_ref.is_some() || p.host != "127.0.0.2")
                            {
                                return Err(
                                    "Changed identity retained the previous credential".into()
                                );
                            }
                            panel.fill(None);
                            panel.name.set_text("Windows lab");
                            panel.kind.set_selected(1);
                            panel.host.set_text("127.0.0.1");
                            panel.port.set_value(3389.0);
                            panel.user.set_text("lab");
                            panel.domain.set_text("LAB");
                            panel.folder.set_text("Linux/Lab");
                            click(&window, "Save connection")?;
                            phase = 5;
                        }
                        5 => {
                            if panel.data.borrow().profiles.len() != 2 {
                                return Err(panel.status.text().into());
                            }
                            panel.tabs.set_current_page(Some(2));
                            panel.folder_source.set_text("Linux");
                            panel.folder_destination.set_text("Archive");
                            click(&window, "Rename")?;
                            phase = 6;
                        }
                        6 => {
                            if !panel
                                .data
                                .borrow()
                                .profiles
                                .iter()
                                .all(|p| p.folder == "Archive/Lab")
                            {
                                return Err("Folder rename did not retain descendants".into());
                            }
                            if !io_started {
                                io_started = true;
                                let path = files.path().join("export.json");
                                let data = panel.data.borrow().clone();
                                let backend = panel.backend.clone();
                                panel.busy(true);
                                let p = panel.clone();
                                worker(
                                    move || {
                                        profile_service::export(&path, &data)?;
                                        let mut incoming = profile_service::import(&path)?;
                                        let current = backend.load()?;
                                        if !current.1 || current.0 != data {
                                            return Err("Persisted/encrypted data mismatch".into());
                                        }
                                        if incoming
                                            .profiles
                                            .iter()
                                            .any(|p| p.credential_ref.is_some())
                                        {
                                            return Err("Export retained credentials".into());
                                        }
                                        if data.merge(&incoming).is_ok() {
                                            return Err(
                                                "Duplicate import unexpectedly overwrote profiles"
                                                    .into(),
                                            );
                                        }
                                        for profile in &mut incoming.profiles {
                                            profile.name = format!("Imported {}", profile.name);
                                        }
                                        data.merge(&incoming)
                                    },
                                    move |result| {
                                        p.busy(false);
                                        match result {
                                            Ok(data) => p.persist(data, None),
                                            Err(error) => p.status.set_text(&error),
                                        }
                                    },
                                );
                                phase = 7;
                            }
                        }
                        7 => {
                            if panel.data.borrow().profiles.len() != 4 {
                                return Err(panel.status.text().into());
                            }
                            panel.tabs.set_current_page(Some(0));
                            capture_frame = window
                                .frame_clock()
                                .map_or(-1, |clock| clock.frame_counter());
                            phase = 8;
                        }
                        8 => {
                            if window
                                .frame_clock()
                                .is_none_or(|clock| clock.frame_counter() <= capture_frame)
                            {
                                return Ok(());
                            }
                            if capture {
                                crate::capture::window(
                                    &window,
                                    std::path::Path::new("dist/windows-probe/profiles.png"),
                                )?;
                            }
                            // Delete the test profiles through the same transactional service, clearing all lab credentials.
                            panel.persist(Workspace::default(), None);
                            phase = 9;
                        }
                        9 => {
                            if !panel.data.borrow().profiles.is_empty() {
                                return Err("Lab cleanup did not commit".into());
                            }
                            result.set(true);
                            println!(
                                "PROFILES_GUI_OK: create/edit SSH and RDP, domain, nested folders, rename, vault identity isolation, optional encryption, import/export, cleanup"
                            );
                            window.close();
                            if let Some(app) = &app {
                                app.quit();
                            }
                            phase = 10;
                        }
                        _ => {}
                    }
                    Ok(())
                })();
                if let Err(error) = next {
                    failure(&error);
                    return gtk::glib::ControlFlow::Break;
                }
                if phase == 10 {
                    gtk::glib::ControlFlow::Break
                } else {
                    gtk::glib::ControlFlow::Continue
                }
            });
        },
    );
}
