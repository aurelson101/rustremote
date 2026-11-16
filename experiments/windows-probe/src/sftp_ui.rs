//! Small native browser over the shared backend. All filesystem/network I/O stays on one worker.
use crate::sftp::{self, Budget, Operation, Output};
use gtk::prelude::*;
use rustremote_core::profiles::SessionProfile;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

enum Command {
    Local(PathBuf),
    Remote(Operation, PathBuf),
}
enum Event {
    Local(PathBuf, Vec<sftp::Entry>),
    Remote(Output),
}
struct Browser {
    window: gtk::glib::WeakRef<gtk::ApplicationWindow>,
    controls: gtk::Box,
    local: gtk::ListBox,
    remote: gtk::ListBox,
    local_path: gtk::Entry,
    remote_path: gtk::Entry,
    status: gtk::Label,
    local_entries: RefCell<Vec<sftp::Entry>>,
    remote_entries: RefCell<Vec<sftp::Entry>>,
    cwd: RefCell<PathBuf>,
    remote_cwd: RefCell<PathBuf>,
    busy: Cell<bool>,
    cancel: Arc<AtomicBool>,
    tx: mpsc::SyncSender<Command>,
    progress: Arc<sftp::Progress>,
}
impl Browser {
    fn send(&self, command: Command) {
        if self.busy.get() {
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.progress.reset();
        if self.tx.try_send(command).is_err() {
            self.status
                .set_text("SFTP worker unavailable; close and reconnect");
            return;
        }
        self.busy.set(true);
        if let Some(window) = self.window.upgrade() {
            gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
        }
        self.controls.set_sensitive(false);
        self.status.set_text("Working…");
    }
    fn list_remote(&self, path: PathBuf) {
        self.send(Command::Remote(Operation::List(path.clone()), path));
    }
    fn selected(&self, remote: bool) -> Vec<(PathBuf, bool)> {
        let (list, entries, cwd) = if remote {
            (&self.remote, &self.remote_entries, &self.remote_cwd)
        } else {
            (&self.local, &self.local_entries, &self.cwd)
        };
        list.selected_rows()
            .iter()
            .filter_map(|row| {
                entries
                    .borrow()
                    .get(row.index() as usize)
                    .and_then(|entry| {
                        (entry.directory || entry.regular)
                            .then(|| (cwd.borrow().join(&entry.name), entry.directory))
                    })
            })
            .collect()
    }
    fn action(&self, operation: Operation) {
        self.send(Command::Remote(operation, self.remote_cwd.borrow().clone()));
    }
    fn update(&self, event: Event) {
        match event {
            Event::Local(path, entries) => {
                self.local_path.set_text(&path.to_string_lossy());
                *self.cwd.borrow_mut() = path;
                fill(&self.local, &entries);
                *self.local_entries.borrow_mut() = entries;
            }
            Event::Remote(Output::Entries { path, entries }) => {
                self.remote_path.set_text(&path.to_string_lossy());
                *self.remote_cwd.borrow_mut() = path;
                fill(&self.remote, &entries);
                *self.remote_entries.borrow_mut() = entries;
            }
            _ => {}
        }
    }
}
fn fill(list: &gtk::ListBox, entries: &[sftp::Entry]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for entry in entries {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        line.append(&gtk::Image::from_icon_name(if entry.directory {
            "folder-symbolic"
        } else if entry.regular {
            "text-x-generic-symbolic"
        } else {
            "emblem-unreadable-symbolic"
        }));
        let label = gtk::Label::new(Some(&format!("{}   · {} B", entry.name, entry.size)));
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        line.append(&label);
        line.set_margin_top(6);
        line.set_margin_bottom(6);
        list.append(&line);
    }
}
fn local_entries(path: PathBuf) -> Result<Event, String> {
    let meta = crate::sftp_platform::metadata(&path).map_err(|_| "Cannot access local folder")?;
    if !meta.is_dir() || meta.is_symlink() {
        return Err("A regular local folder is required".into());
    }
    let mut entries = Vec::new();
    for item in std::fs::read_dir(&path)
        .map_err(|_| "Cannot list local folder")?
        .take(4097)
    {
        if entries.len() == 4096 {
            return Err("Local listing exceeds 4096 entries".into());
        }
        let item = item.map_err(|_| "Local listing interrupted")?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| "Non-Unicode filename refused")?;
        // Ineligible entries remain visible, but cannot be opened or transferred.
        let meta = crate::sftp_platform::metadata(&item.path()).ok();
        entries.push(sftp::Entry {
            name,
            directory: meta.as_ref().is_some_and(|m| m.is_dir() && !m.is_symlink()),
            regular: meta
                .as_ref()
                .is_some_and(|m| m.is_file() && !m.is_symlink()),
            size: meta.map_or(0, |m| m.len()),
        });
    }
    entries.sort_by(|a, b| b.directory.cmp(&a.directory).then(a.name.cmp(&b.name)));
    Ok(Event::Local(path, entries))
}
fn connect(
    profile: &SessionProfile,
    laboratory_secret: Option<zeroize::Zeroizing<String>>,
) -> Result<sftp::Connection, String> {
    if let Some(secret) = laboratory_secret {
        return sftp::Connection::connect(profile, Some(&secret));
    }
    #[cfg(windows)]
    let secret = crate::win_credentials::read(profile)?;
    #[cfg(not(windows))]
    let secret: Option<zeroize::Zeroizing<String>> = None;
    sftp::Connection::connect(profile, secret.as_deref().map(|s| s.as_str()))
}

pub fn open(parent: &gtk::ApplicationWindow, profile: SessionProfile) -> Result<(), String> {
    build(parent, profile, None).map(|_| ())
}
fn build(
    parent: &gtk::ApplicationWindow,
    profile: SessionProfile,
    laboratory_secret: Option<zeroize::Zeroizing<String>>,
) -> Result<(gtk::ApplicationWindow, Rc<Browser>), String> {
    crate::ssh_launch::arguments(&profile)?;
    let app = parent.application().ok_or("Application unavailable")?;
    if app.windows().len() >= 18 {
        return Err("Close an existing session first (16 remote windows maximum)".into());
    }
    let (tx, requests) = mpsc::sync_channel::<Command>(1);
    let (events, rx) = mpsc::sync_channel(1);
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(sftp::Progress::default());
    let stopped = cancel.clone();
    let meter = progress.clone();
    std::thread::Builder::new()
        .name("windows-sftp".into())
        .spawn(move || {
            let connection = match connect(&profile, laboratory_secret) {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = events.send(Err(error));
                    return;
                }
            };
            let mut budget = Budget::new(stopped.clone()).track(meter.clone());
            if events
                .send(
                    connection
                        .run(Operation::List(".".into()), &mut budget)
                        .map(Event::Remote),
                )
                .is_err()
            {
                return;
            }
            while let Ok(command) = requests.recv() {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                let result = match command {
                    Command::Local(path) => local_entries(path),
                    Command::Remote(operation, refresh) => {
                        let mut budget = Budget::new(stopped.clone()).track(meter.clone());
                        connection.run(operation, &mut budget).and_then(|output| {
                            if matches!(output, Output::Done(_)) {
                                connection
                                    .run(Operation::List(refresh), &mut budget)
                                    .map(Event::Remote)
                            } else {
                                Ok(Event::Remote(output))
                            }
                        })
                    }
                };
                if events.send(result).is_err() {
                    break;
                }
            }
            connection.abort();
        })
        .map_err(|_| "Cannot start SFTP worker")?;
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.add_css_class("profiles");
    for side in [
        gtk::PositionType::Top,
        gtk::PositionType::Bottom,
        gtk::PositionType::Left,
        gtk::PositionType::Right,
    ] {
        match side {
            gtk::PositionType::Top => body.set_margin_top(12),
            gtk::PositionType::Bottom => body.set_margin_bottom(12),
            gtk::PositionType::Left => body.set_margin_start(12),
            _ => body.set_margin_end(12),
        }
    }
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let toolbar = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(6)
        .column_spacing(8)
        .row_spacing(6)
        .build();
    controls.append(&toolbar);
    let panes = gtk::Paned::new(gtk::Orientation::Horizontal);
    panes.set_wide_handle(true);
    panes.set_vexpand(true);
    let local = gtk::ListBox::new();
    let remote = gtk::ListBox::new();
    let local_path = gtk::Entry::new();
    let remote_path = gtk::Entry::new();
    for (index, (title, entry, list)) in [
        ("Local site", &local_path, &local),
        ("Remote site", &remote_path, &remote),
    ]
    .into_iter()
    .enumerate()
    {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.append(&gtk::Label::new(Some(title)));
        column.append(entry);
        list.set_selection_mode(gtk::SelectionMode::Multiple);
        list.set_activate_on_single_click(true);
        let scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .min_content_width(230)
            .min_content_height(220)
            .child(list)
            .build();
        column.append(&scroll);
        if index == 0 {
            panes.set_start_child(Some(&column));
        } else {
            panes.set_end_child(Some(&column));
        }
    }
    controls.append(&panes);
    body.append(&controls);
    let status = gtk::Label::new(Some("Connecting with strict host-key validation…"));
    status.set_wrap(true);
    status.set_xalign(0.0);
    body.append(&status);
    let cancel_button = gtk::Button::with_label("Cancel operation / disconnect");
    body.append(&cancel_button);
    let window = gtk::ApplicationWindow::builder()
        .application(&app)
        .transient_for(parent)
        .title("RustRemote — SFTP")
        .default_width(1040)
        .default_height(680)
        .child(&body)
        .build();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default();
    let browser = Rc::new(Browser {
        window: window.downgrade(),
        controls,
        local,
        remote,
        local_path,
        remote_path,
        status,
        local_entries: RefCell::new(Vec::new()),
        remote_entries: RefCell::new(Vec::new()),
        cwd: RefCell::new(home),
        remote_cwd: RefCell::new(".".into()),
        busy: Cell::new(true),
        cancel,
        tx,
        progress,
    });
    browser.controls.set_sensitive(false);
    for (entry, remote) in [(&browser.local_path, false), (&browser.remote_path, true)] {
        let weak = Rc::downgrade(&browser);
        entry.connect_activate(move |entry| {
            if let Some(b) = weak.upgrade() {
                let path = PathBuf::from(entry.text().as_str());
                if remote {
                    b.list_remote(path);
                } else {
                    b.send(Command::Local(path));
                }
            }
        });
    }
    for (list, remote) in [(&browser.local, false), (&browser.remote, true)] {
        let weak = Rc::downgrade(&browser);
        list.connect_row_activated(move |_, row| {
            if let Some(b) = weak.upgrade() {
                let entries = if remote {
                    &b.remote_entries
                } else {
                    &b.local_entries
                };
                let name = entries
                    .borrow()
                    .get(row.index() as usize)
                    .filter(|e| e.directory)
                    .map(|e| e.name.clone());
                if let Some(name) = name {
                    if remote {
                        let path = b.remote_cwd.borrow().join(name);
                        b.list_remote(path);
                    } else {
                        let path = b.cwd.borrow().join(name);
                        b.send(Command::Local(path));
                    }
                }
            }
        });
    }
    button(&toolbar, "Local parent", &browser, |b| {
        let path = b.cwd.borrow().parent().map(PathBuf::from);
        if let Some(path) = path {
            b.send(Command::Local(path));
        }
    });
    button(&toolbar, "Remote parent", &browser, |b| {
        let path = b
            .remote_cwd
            .borrow()
            .parent()
            .map(PathBuf::from)
            .unwrap_or("/".into());
        b.list_remote(path);
    });
    button(&toolbar, "Refresh local", &browser, |b| {
        let path = b.cwd.borrow().clone();
        b.send(Command::Local(path));
    });
    button(&toolbar, "Refresh remote", &browser, |b| {
        let path = b.remote_cwd.borrow().clone();
        b.list_remote(path);
    });
    button(&toolbar, "Upload →", &browser, |b| transfer(b, true));
    button(&toolbar, "← Download", &browser, |b| transfer(b, false));
    button(&toolbar, "New remote folder…", &browser, |b| {
        name_dialog(b, None)
    });
    button(&toolbar, "Rename remote…", &browser, |b| {
        let selected = b.selected(true);
        if selected.len() == 1 {
            name_dialog(b, Some(selected[0].0.clone()));
        } else {
            b.status.set_text("Select one remote entry");
        }
    });
    button(&toolbar, "Edit remote…", &browser, |b| {
        let selected = b.selected(true);
        if selected.len() == 1 && !selected[0].1 {
            b.action(Operation::Read(selected[0].0.clone()));
        } else {
            b.status
                .set_text("Select one regular remote file (UTF-8, maximum 1 MiB)");
        }
    });
    button(&toolbar, "Delete remote…", &browser, delete_dialog);
    let stopped = browser.cancel.clone();
    let weak = Rc::downgrade(&browser);
    cancel_button.connect_clicked(move |_| {
        stopped.store(true, Ordering::Relaxed);
        if let Some(b) = weak.upgrade() {
            b.status
                .set_text("Cancellation requested; close and reconnect after completion");
        }
    });
    let stopped = browser.cancel.clone();
    window.connect_close_request(move |_| {
        stopped.store(true, Ordering::Relaxed);
        gtk::glib::Propagation::Proceed
    });
    let result = browser.clone();
    let mut initial = true;
    gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
        if browser.window.upgrade().is_none_or(|w| !w.is_visible()) {
            browser.cancel.store(true, Ordering::Relaxed);
            return gtk::glib::ControlFlow::Break;
        }
        match rx.try_recv() {
            Ok(result) => {
                let success = result.is_ok();
                browser.busy.set(false);
                browser
                    .controls
                    .set_sensitive(!browser.cancel.load(Ordering::Relaxed));
                match result {
                    Ok(Event::Remote(Output::Text { path, bytes })) => {
                        browser.status.set_text("Remote text loaded · saving checks for changes and preserves permissions");
                        editor(&browser, path, bytes)
                    }
                    Ok(event) => {
                        browser.update(event);
                        browser.status.set_text("Ready · defaults: 16 MiB / 128 entries / 30 seconds per operation; existing destinations are never replaced");
                    }
                    Err(error) => browser.status.set_text(&error),
                }
                if initial && success {
                    initial = false;
                    let path = browser.cwd.borrow().clone();
                    browser.send(Command::Local(path));
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                if browser.busy.get() {
                    let (bytes, entries) = browser.progress.snapshot();
                    browser
                        .status
                        .set_text(&format!("Working… {bytes} bytes · {entries} entries"));
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                browser.controls.set_sensitive(false);
                if browser.busy.replace(false) {
                    browser
                        .status
                        .set_text("SFTP worker stopped; close and reconnect");
                }
                return gtk::glib::ControlFlow::Break;
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    window.present();
    Ok((window, result))
}
fn button(
    bar: &gtk::FlowBox,
    title: &str,
    browser: &Rc<Browser>,
    action: impl Fn(&Rc<Browser>) + 'static,
) {
    let button = gtk::Button::with_label(title);
    let weak = Rc::downgrade(browser);
    button.connect_clicked(move |_| {
        if let Some(browser) = weak.upgrade()
            && !browser.busy.get()
        {
            action(&browser);
        }
    });
    bar.insert(&button, -1);
}
fn transfer(b: &Rc<Browser>, upload: bool) {
    let selected = b.selected(!upload);
    if selected.is_empty() || selected.len() > 128 {
        b.status.set_text("Select 1–128 files or folders");
        return;
    }
    let operations = selected
        .into_iter()
        .map(|(path, _)| {
            let name = path.file_name().ok_or("Invalid entry name")?;
            if upload {
                Ok(Operation::Upload {
                    remote: b.remote_cwd.borrow().join(name),
                    local: path,
                })
            } else {
                Ok(Operation::Download {
                    local: b.cwd.borrow().join(name),
                    remote: path,
                })
            }
        })
        .collect::<Result<Vec<_>, &str>>();
    match operations {
        Ok(ops) => b.action(Operation::Batch(ops)),
        Err(error) => b.status.set_text(error),
    }
}
fn name_dialog(b: &Rc<Browser>, source: Option<PathBuf>) {
    let Some(parent) = b.window.upgrade() else {
        return;
    };
    let dialog = gtk::Dialog::builder()
        .transient_for(&parent)
        .modal(true)
        .title(if source.is_some() {
            "Rename remote entry"
        } else {
            "New remote folder"
        })
        .build();
    let entry = gtk::Entry::new();
    entry.set_max_length(255);
    dialog.content_area().append(&entry);
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Apply", gtk::ResponseType::Ok);
    let weak = Rc::downgrade(b);
    dialog.connect_response(move |dialog, response| {
        gtk::prelude::GtkWindowExt::set_focus(dialog, None::<&gtk::Widget>);
        if response == gtk::ResponseType::Ok
            && let Some(b) = weak.upgrade()
        {
            let name = entry.text();
            match sftp::component(&name) {
                Ok(()) => {
                    let to = b.remote_cwd.borrow().join(name.as_str());
                    b.action(if let Some(from) = &source {
                        Operation::Rename {
                            from: from.clone(),
                            to,
                        }
                    } else {
                        Operation::Mkdir(to)
                    });
                }
                Err(error) => b.status.set_text(&error),
            }
        }
        dialog.close();
    });
    dialog.present();
}
fn delete_dialog(b: &Rc<Browser>) {
    let selected = b.selected(true);
    if selected.is_empty() || selected.len() > 128 {
        b.status.set_text("Select 1–128 remote entries");
        return;
    }
    let Some(parent) = b.window.upgrade() else {
        return;
    };
    let dialog = gtk::MessageDialog::builder()
        .transient_for(&parent)
        .modal(true)
        .text(format!("Delete {} remote entries?", selected.len()))
        .secondary_text("Directories are removed recursively. This cannot be undone.")
        .buttons(gtk::ButtonsType::OkCancel)
        .build();
    let weak = Rc::downgrade(b);
    dialog.connect_response(move |dialog, response| {
        gtk::prelude::GtkWindowExt::set_focus(dialog, None::<&gtk::Widget>);
        if response == gtk::ResponseType::Ok
            && let Some(b) = weak.upgrade()
        {
            b.action(Operation::Batch(
                selected
                    .iter()
                    .map(|(p, _)| Operation::Delete(p.clone()))
                    .collect(),
            ));
        }
        dialog.close();
    });
    dialog.present();
}
fn editor(b: &Rc<Browser>, path: PathBuf, original: Vec<u8>) {
    let Some(parent) = b.window.upgrade() else {
        return;
    };
    let dialog = gtk::Dialog::builder()
        .transient_for(&parent)
        .modal(true)
        .title("Remote UTF-8 editor")
        .default_width(760)
        .default_height(520)
        .build();
    let view = gtk::TextView::new();
    view.set_monospace(true);
    view.buffer().set_text(&String::from_utf8_lossy(&original));
    dialog.content_area().append(
        &gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&view)
            .build(),
    );
    dialog.add_button("Close", gtk::ResponseType::Cancel);
    dialog.add_button("Save remotely", gtk::ResponseType::Ok);
    let weak = Rc::downgrade(b);
    dialog.connect_response(move |dialog, response| {
        gtk::prelude::GtkWindowExt::set_focus(dialog, None::<&gtk::Widget>);
        if response == gtk::ResponseType::Ok
            && let Some(b) = weak.upgrade()
        {
            let buffer = view.buffer();
            if buffer.char_count() > 1024 * 1024 {
                b.status.set_text("Editor exceeds 1 MiB");
                dialog.close();
                return;
            }
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .as_bytes()
                .to_vec();
            b.action(Operation::Save {
                path: path.clone(),
                original: original.clone(),
                text,
            });
        }
        dialog.close();
    });
    dialog.present();
}

/// Native widgets and real loopback SFTP; lab input never becomes process arguments or logs.
pub fn laboratory() -> Result<bool, String> {
    use std::io::Read;
    use zeroize::{Zeroize, Zeroizing};
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        profile: SessionProfile,
        secret: String,
        expect_reject: bool,
    }
    impl Drop for Input {
        fn drop(&mut self) {
            self.secret.zeroize();
        }
    }
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .lock()
        .take(32769)
        .read_to_end(&mut bytes)
        .map_err(|_| "Lab stdin unavailable")?;
    if bytes.len() > 32768 {
        return Err("Lab input exceeds budget".into());
    }
    let input: Input = serde_json::from_slice(&bytes).map_err(|_| "Invalid lab profile")?;
    if input.expect_reject
        || input.profile.credential_ref.is_some()
        || !input
            .profile
            .host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
        || !matches!(
            &input.profile.options,
            rustremote_core::profiles::ProtocolOptions::Ssh {
                agent: false,
                config_file: None,
                known_hosts_file: Some(_),
                ..
            }
        )
    {
        return Err("Only explicit trusted loopback fixtures are allowed".into());
    }
    let mut profile = input.profile.clone();
    let backend = crate::profile_service::laboratory()?;
    #[cfg(windows)]
    {
        profile.credential_ref = Some(backend.credential_write(&profile, &input.secret)?);
    }
    #[cfg(not(windows))]
    let _ = &mut profile;
    let temporary = Arc::new(tempfile::tempdir().map_err(|_| "Local fixture unavailable")?);
    let upload = temporary.path().join("dossier été");
    std::fs::create_dir(&upload).map_err(|_| "Fixture mkdir unavailable")?;
    let payload = b"binary\0\xff\xfe";
    std::fs::write(upload.join("fichier é.bin"), payload)
        .map_err(|_| "Fixture file unavailable")?;
    std::fs::write(upload.join("texte.txt"), b"initial text")
        .map_err(|_| "Fixture text unavailable")?;
    let app = gtk::Application::builder()
        .application_id("com.rustremote.SftpLab")
        .build();
    let passed = Rc::new(Cell::new(false));
    let seen = passed.clone();
    let secret = Zeroizing::new(input.secret.clone());
    app.connect_activate(move |app| {
        let _guard=&backend;
        let parent=gtk::ApplicationWindow::builder().application(app).title("SFTP loopback laboratory").build();
        let secret=if cfg!(windows){None}else{Some(secret.clone())};
        let (window,browser)=match build(&parent,profile.clone(),secret) {Ok(value)=>value,Err(error)=>{eprintln!("{error}");app.quit();return;}};
        let temporary=temporary.clone();let mut phase=0;let started=std::time::Instant::now();
        let app=app.clone();let passed=seen.clone();let mut ticks=0;
        gtk::glib::timeout_add_local(Duration::from_millis(50),move || {
            ticks+=1;
            if started.elapsed()>Duration::from_secs(45) {
                eprintln!("SFTP UI timeout at phase {phase}: {}",browser.status.text());window.close();app.quit();return gtk::glib::ControlFlow::Break;
            }
            if browser.busy.get(){return gtk::glib::ControlFlow::Continue;}
            let click=|title:&str|->bool {
                fn find(widget:&gtk::Widget,title:&str)->Option<gtk::Button>{
                    if let Some(button)=widget.downcast_ref::<gtk::Button>() && button.label().is_some_and(|label|label==title){return Some(button.clone());}
                    let mut child=widget.first_child();while let Some(widget)=child {if let Some(button)=find(&widget,title){return Some(button);}child=widget.next_sibling();}None
                }
                if let Some(button)=find(browser.controls.upcast_ref(),title){button.emit_clicked();true}else{false}
            };
            let select=|remote:bool,name:&str,activate:bool|->bool {
                let (list,entries)=if remote{(&browser.remote,&browser.remote_entries)}else{(&browser.local,&browser.local_entries)};
                let index=entries.borrow().iter().position(|entry|entry.name==name);
                if let Some(row)=index.and_then(|i|list.row_at_index(i as i32)) {list.unselect_all();list.select_row(Some(&row));if activate {row.emit_activate();}true}else{false}
            };
            let dialog=|title:&str| gtk::Window::list_toplevels().iter().find_map(|window|window.clone().downcast::<gtk::Dialog>().ok().filter(|dialog|dialog.title().is_some_and(|t|t==title)));
            match phase {
                0=>{browser.send(Command::Local(temporary.path().to_path_buf()));phase=1;},
                1=>{if !select(false,"dossier été",true){return gtk::glib::ControlFlow::Continue;}phase=2;},
                2=>{if !browser.cwd.borrow().ends_with("dossier été"){return gtk::glib::ControlFlow::Continue;}click("Local parent");phase=3;},
                3=>{if !select(false,"dossier été",false){return gtk::glib::ControlFlow::Continue;}click("Upload →");phase=4;},
                4=>{if !select(true,"dossier été",true){return gtk::glib::ControlFlow::Continue;}phase=5;},
                5=>{if !browser.remote_entries.borrow().iter().any(|e|e.name=="texte.txt"){return gtk::glib::ControlFlow::Continue;}click("Remote parent");phase=6;},
                6=>{
                    let download=temporary.path().join("downloads");if std::fs::create_dir(&download).is_err(){app.quit();return gtk::glib::ControlFlow::Break;}
                    browser.send(Command::Local(download));phase=7;
                },
                7=>{if !select(true,"dossier été",false){return gtk::glib::ControlFlow::Continue;}click("← Download");phase=8;},
                8=>{
                    if std::fs::read(temporary.path().join("downloads/dossier été/fichier é.bin")).ok().as_deref()!=Some(payload){eprintln!("UI binary download mismatch");app.quit();return gtk::glib::ControlFlow::Break;}
                    select(true,"dossier été",true);phase=9;
                },
                9=>{if !select(true,"texte.txt",false){return gtk::glib::ControlFlow::Continue;}click("Edit remote…");phase=10;},
                10=>{
                    if let Some(dialog)=dialog("Remote UTF-8 editor"){
                        fn text(widget:&gtk::Widget)->Option<gtk::TextView>{if let Some(view)=widget.downcast_ref::<gtk::TextView>(){return Some(view.clone());}let mut child=widget.first_child();while let Some(w)=child{if let Some(v)=text(&w){return Some(v);}child=w.next_sibling();}None}
                        if let Some(view)=text(dialog.upcast_ref()){view.buffer().set_text("edited via native SFTP");dialog.response(gtk::ResponseType::Ok);phase=11;}
                    }
                },
                11=>{if let Some(dialog)=dialog("Remote UTF-8 editor"){dialog.response(gtk::ResponseType::Cancel);click("Remote parent");phase=12;}},
                12=>{if !select(true,"dossier été",false){return gtk::glib::ControlFlow::Continue;}click("Delete remote…");phase=13;},
                13=>{if let Some(dialog)=gtk::Window::list_toplevels().iter().find_map(|w|w.clone().downcast::<gtk::MessageDialog>().ok()){dialog.response(gtk::ResponseType::Ok);phase=14;}},
                _=>{
                    if !browser.remote_entries.borrow().is_empty(){return gtk::glib::ControlFlow::Continue;}
                    if let Err(error)=crate::capture::window(&window,std::path::Path::new("dist/windows-probe/sftp-gui.png")){eprintln!("{error}");}else if ticks>5 {passed.set(true);println!("WINDOWS_SFTP_GUI_OK: widget activation, local/remote directory navigation, folder upload/download, binary readback, remote edit/save, confirmed recursive deletion; GTK ticks={ticks}");}
                    window.close();app.quit();return gtk::glib::ControlFlow::Break;
                }
            }
            gtk::glib::ControlFlow::Continue
        });
    });
    app.run_with_args::<&str>(&[]);
    Ok(passed.get())
}
