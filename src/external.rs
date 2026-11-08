//! Client graphique RDP externe, secret transmis sur pipe, arrêt par pidfd.
use crate::{credentials::Secret, profiles::SessionProfile};
use std::{
    io::Write,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};
#[derive(Clone)]
pub struct Client(Arc<Inner>);
struct Inner {
    pidfd: OwnedFd,
    alive: Arc<AtomicBool>,
    exit: Arc<AtomicI32>,
}
impl Client {
    pub fn spawn(profile: &SessionProfile, password: Secret) -> Result<Self, String> {
        crate::credentials::validate_password(&password)?;
        let (binary, args) = profile.command()?;
        let mut command = Command::new(binary);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for key in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "SSH_ASKPASS",
            "SSH_ASKPASS_REQUIRE",
            "RUSTREMOTE_ASKPASS_SOCKET",
            "VNC_VIA_CMD",
        ] {
            command.env_remove(key);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Le client RDP n’a pas pu être lancé")?;
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id(), 0) };
        if fd < 0 {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Suivi sécurisé du client RDP indisponible".into());
        }
        let pidfd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        let result = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("pipe absent"))
            .and_then(|mut input| {
                input.write_all(password.as_bytes())?;
                input.write_all(b"\n")
            });
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Transmission du mot de passe RDP impossible".into());
        }
        let alive = Arc::new(AtomicBool::new(true));
        let state = alive.clone();
        let exit = Arc::new(AtomicI32::new(-1000));
        let result = exit.clone();
        std::thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            result.store(code, Ordering::Release);
            state.store(false, Ordering::Release);
        });
        Ok(Self(Arc::new(Inner { pidfd, alive, exit })))
    }
    pub fn active(&self) -> bool {
        self.0.alive.load(Ordering::Relaxed)
    }
    pub fn exit_code(&self) -> Option<i32> {
        let code = self.0.exit.load(Ordering::Acquire);
        (code != -1000).then_some(code)
    }
    pub fn disconnect(&self) {
        if self.active() {
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.0.pidfd.as_raw_fd(),
                    libc::SIGTERM,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        if self.alive.load(Ordering::Relaxed) {
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.pidfd.as_raw_fd(),
                    libc::SIGTERM,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
    }
}
