//! Authentification OpenSSH par askpass Rust, socket privé et vérification du processus pair.
use crate::{askpass_wire as wire, credentials::Secret};
use gtk::prelude::*;
use std::{
    cell::RefCell,
    os::{fd::AsRawFd, unix::net::UnixListener},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
struct Prompt {
    text: String,
    confirm: bool,
    reply: mpsc::Sender<Option<Secret>>,
}
pub struct Bridge {
    _directory: tempfile::TempDir,
    pub path: PathBuf,
    pub pid: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    source: Option<gtk::glib::SourceId>,
    dialogs: Rc<RefCell<Vec<gtk::glib::WeakRef<gtk::Window>>>>,
}
impl Bridge {
    pub fn new(parent: &gtk::Window, secret: Arc<Secret>) -> Result<Self, String> {
        let directory = tempfile::Builder::new()
            .prefix("rr-auth-")
            .tempdir()
            .map_err(|_| "Socket d’authentification indisponible")?;
        let path = directory.path().join("auth.sock");
        let listener = UnixListener::bind(&path).map_err(|_| "Création du socket impossible")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Initialisation du socket impossible")?;
        let helper = std::env::current_exe()
            .map_err(|_| "Chemin de l’application introuvable")?
            .with_file_name("rustremote-askpass");
        let helper = helper
            .canonicalize()
            .map_err(|_| "Le composant rustremote-askpass est absent")?;
        let stop = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel::<Prompt>();
        let cancel = stop.clone();
        let ssh_pid = pid.clone();
        std::thread::Builder::new()
            .name("ssh-auth".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                let mut used = false;
                let mut requests = 0;
                while !cancel.load(Ordering::Relaxed) && requests < 16 {
                    let mut event = libc::pollfd {
                        fd: listener.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    if unsafe { libc::poll(&mut event, 1, 250) } <= 0 {
                        continue;
                    }
                    let Ok((mut stream, _)) = listener.accept() else {
                        continue;
                    };
                    requests += 1;
                    let Ok((uid, pid)) = wire::peer(&stream) else {
                        continue;
                    };
                    if uid != unsafe { libc::geteuid() }
                        || std::fs::read_link(format!("/proc/{pid}/exe")).ok().as_ref()
                            != Some(&helper)
                    {
                        continue;
                    }
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while ssh_pid.load(Ordering::Relaxed) == 0
                        && Instant::now() < deadline
                        && !cancel.load(Ordering::Relaxed)
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    let ppid = std::fs::read_to_string(format!("/proc/{pid}/status"))
                        .ok()
                        .and_then(|s| {
                            s.lines().find_map(|l| {
                                l.strip_prefix("PPid:")
                                    .and_then(|v| v.trim().parse::<u32>().ok())
                            })
                        });
                    if ppid != Some(ssh_pid.load(Ordering::Relaxed))
                        || cancel.load(Ordering::Relaxed)
                    {
                        continue;
                    }
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    let Ok(request) = wire::receive(&mut stream) else {
                        continue;
                    };
                    let Some((&kind, text)) = request.split_first() else {
                        continue;
                    };
                    let Ok(text) = std::str::from_utf8(text) else {
                        continue;
                    };
                    let lower = text.to_ascii_lowercase();
                    let value = if kind == 0
                        && !used
                        && lower.contains("password")
                        && !lower.contains("passphrase")
                    {
                        used = true;
                        Some(Secret::new(secret.as_str().to_owned()))
                    } else {
                        let (reply, response) = mpsc::channel();
                        if tx
                            .send(Prompt {
                                text: text.into(),
                                confirm: kind == 1,
                                reply,
                            })
                            .is_err()
                        {
                            break;
                        }
                        let deadline = Instant::now() + Duration::from_secs(90);
                        loop {
                            match response.recv_timeout(Duration::from_millis(100)) {
                                Ok(value) => break value,
                                Err(mpsc::RecvTimeoutError::Timeout)
                                    if !cancel.load(Ordering::Relaxed)
                                        && Instant::now() < deadline => {}
                                _ => break None,
                            }
                        }
                    };
                    let mut packet = zeroize::Zeroizing::new(vec![u8::from(value.is_some())]);
                    if let Some(value) = value {
                        packet.extend_from_slice(value.as_bytes());
                    }
                    let _ = wire::send(&mut stream, &packet);
                }
            })
            .map_err(|_| "Démarrage du lecteur d’authentification impossible")?;
        let dialogs = Rc::new(RefCell::new(Vec::new()));
        let windows = dialogs.clone();
        let parent = parent.downgrade();
        let source = gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            if let Ok(prompt) = rx.try_recv() {
                let Some(parent) = parent.upgrade() else {
                    let _ = prompt.reply.send(None);
                    return gtk::glib::ControlFlow::Continue;
                };
                let window = gtk::Window::builder()
                    .title(if prompt.confirm {
                        "Vérifier la clé SSH"
                    } else {
                        "Authentification SSH"
                    })
                    .transient_for(&parent)
                    .destroy_with_parent(true)
                    .modal(true)
                    .default_width(560)
                    .build();
                window.set_application(parent.application().as_ref());
                let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
                body.add_css_class("dialog-content");
                let text = gtk::Label::new(Some(&prompt.text));
                text.set_wrap(true);
                text.set_selectable(true);
                body.append(&text);
                let password = gtk::PasswordEntry::builder().show_peek_icon(true).build();
                password.update_property(&[gtk::accessible::Property::Label(
                    "Réponse d’authentification SSH",
                )]);
                if !prompt.confirm {
                    body.append(&password);
                }
                let accept = gtk::Button::with_label(if prompt.confirm {
                    "Accepter cette empreinte"
                } else {
                    "Valider l’authentification"
                });
                accept.add_css_class("suggested-action");
                let w = window.downgrade();
                let reply = prompt.reply.clone();
                let confirm = prompt.confirm;
                accept.connect_clicked(move |_| {
                    let value = if confirm {
                        "yes".to_string()
                    } else {
                        password.text().to_string()
                    };
                    password.set_text("");
                    let _ = reply.send(Some(Secret::new(value)));
                    if let Some(w) = w.upgrade() {
                        w.close();
                    }
                });
                body.append(&accept);
                let cancel = gtk::Button::with_label("Annuler");
                let w = window.downgrade();
                cancel.connect_clicked(move |_| {
                    if let Some(w) = w.upgrade() {
                        w.close();
                    }
                });
                body.append(&cancel);
                window.connect_hide(move |w| {
                    let _ = prompt.reply.send(None);
                    w.set_child(None::<&gtk::Widget>);
                });
                window.set_child(Some(&body));
                windows
                    .borrow_mut()
                    .retain(|w: &gtk::glib::WeakRef<gtk::Window>| w.upgrade().is_some());
                windows.borrow_mut().push(window.downgrade());
                window.present();
            }
            gtk::glib::ControlFlow::Continue
        });
        Ok(Self {
            _directory: directory,
            path,
            pid,
            stop,
            source: Some(source),
            dialogs,
        })
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(source) = self.source.take() {
            source.remove();
        }
        for window in self.dialogs.borrow().iter().filter_map(|w| w.upgrade()) {
            window.close();
        }
    }
}
