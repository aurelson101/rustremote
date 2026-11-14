//! Durable transfer intent; no credentials, no automatic replay after restart.
use crate::{profiles::SessionProfile, sftp::Operation, store::Store};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::PathBuf,
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    host: String,
    port: u16,
    user: String,
}
impl From<&SessionProfile> for Target {
    fn from(p: &SessionProfile) -> Self {
        Self {
            host: p.host.clone(),
            port: p.port,
            user: p.username.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum State {
    Pending,
    Running,
    Completed,
    Failed,
    Interrupted,
}
impl State {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "En attente",
            Self::Running => "Transfert en cours",
            Self::Completed => "Terminé",
            Self::Failed => "Échec",
            Self::Interrupted => "Interrompu",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: u64,
    pub upload: bool,
    pub local: PathBuf,
    pub remote: PathBuf,
    pub state: State,
}
impl Job {
    pub fn operation(&self) -> Operation {
        if self.upload {
            Operation::Upload {
                local: self.local.clone(),
                remote: self.remote.clone(),
            }
        } else {
            Operation::Download {
                local: self.local.clone(),
                remote: self.remote.clone(),
            }
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Queue {
    version: u8,
    target: Target,
    pub revision: u64,
    pub jobs: Vec<Job>,
    next: u64,
}
impl Queue {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.jobs.len() > 256
            || self.jobs.iter().any(|j| {
                j.id == 0
                    || j.id >= self.next
                    || !j.local.is_absolute()
                    || j.local.as_os_str().len() > 4096
                    || j.remote.as_os_str().is_empty()
                    || j.remote.as_os_str().len() > 4096
                    || j.local.to_string_lossy().chars().any(char::is_control)
                    || j.remote.to_string_lossy().chars().any(char::is_control)
            })
            || self
                .jobs
                .iter()
                .map(|j| j.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.jobs.len()
        {
            return Err("File SFTP invalide ou limitée à 256 transferts".into());
        }
        Ok(())
    }
    pub fn pending(&self) -> bool {
        self.jobs.iter().any(|j| j.state == State::Pending)
    }
    pub fn unfinished(&self) -> bool {
        self.jobs.iter().any(|j| j.state != State::Completed)
    }
}
pub struct Engine {
    store: Store,
    pub queue: Queue,
    _lock: File,
}
impl Engine {
    pub fn open(store: Store, profile: &SessionProfile) -> Result<Self, String> {
        profile.validate()?;
        let store = store.transfer_store(&profile.name);
        let directory = store.path.parent().ok_or("Dossier de file invalide")?;
        std::fs::create_dir_all(directory).map_err(|_| "Dossier de file inaccessible")?;
        if std::fs::symlink_metadata(directory)
            .map_err(|_| "Dossier de file inaccessible")?
            .file_type()
            .is_symlink()
        {
            return Err("Dossier symbolique refusé".into());
        }
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Permissions de file impossibles")?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(store.path.with_extension("lock"))
            .map_err(|_| "Verrou de file indisponible")?;
        if !lock.metadata().map_err(|_| "Verrou invalide")?.is_file()
            || unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
        {
            return Err("File SFTP déjà utilisée ; attendez la fin de la fermeture".into());
        }
        let mut queue = if store.path.symlink_metadata().is_ok() {
            serde_json::from_slice::<Queue>(&store.read_bytes()?)
                .map_err(|_| "Format de file invalide")?
        } else {
            Queue {
                version: 1,
                target: Target::from(profile),
                revision: 0,
                jobs: Vec::new(),
                next: 1,
            }
        };
        queue.validate()?;
        if queue.target != Target::from(profile) {
            return Err("La cible de cette file a changé : reprise refusée".into());
        }
        for job in &mut queue.jobs {
            if job.state == State::Running {
                job.state = State::Interrupted;
            }
        }
        Ok(Self {
            store,
            queue,
            _lock: lock,
        })
    }
    fn commit(&mut self, mut next: Queue) -> Result<Queue, String> {
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or("Révision de file dépassée")?;
        next.validate()?;
        let bytes = serde_json::to_vec(&next).map_err(|_| "File non sérialisable")?;
        self.store.write_bytes(&bytes, false)?;
        self.queue = next;
        Ok(self.queue.clone())
    }
    pub fn enqueue(&mut self, operations: Vec<Operation>) -> Result<Queue, String> {
        let mut next = self.queue.clone();
        for op in operations {
            let (upload, local, remote) = match op {
                Operation::Upload { local, remote } => (true, local, remote),
                Operation::Download { local, remote } => (false, local, remote),
                _ => return Err("Seuls les transferts peuvent être mis en file".into()),
            };
            let id = next.next;
            next.next = next
                .next
                .checked_add(1)
                .ok_or("Identifiant de file dépassé")?;
            next.jobs.push(Job {
                id,
                upload,
                local,
                remote,
                state: State::Pending,
            });
        }
        self.commit(next)
    }
    pub fn claim(&mut self) -> Result<Option<Job>, String> {
        let mut next = self.queue.clone();
        let Some(job) = next.jobs.iter_mut().find(|j| j.state == State::Pending) else {
            return Ok(None);
        };
        job.state = State::Running;
        let claimed = job.clone();
        self.commit(next)?;
        Ok(Some(claimed))
    }
    pub fn finish(&mut self, id: u64, success: bool, cancelled: bool) -> Result<Queue, String> {
        let mut next = self.queue.clone();
        let job = next
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.state == State::Running)
            .ok_or("Transfert de file inconnu")?;
        job.state = if success {
            State::Completed
        } else if cancelled {
            State::Interrupted
        } else {
            State::Failed
        };
        self.commit(next)
    }
    pub fn retry(&mut self) -> Result<Queue, String> {
        let mut next = self.queue.clone();
        for j in &mut next.jobs {
            if matches!(j.state, State::Failed | State::Interrupted) {
                j.state = State::Pending;
            }
        }
        self.commit(next)
    }
    pub fn clear_completed(&mut self) -> Result<Queue, String> {
        let mut next = self.queue.clone();
        next.jobs.retain(|j| j.state != State::Completed);
        self.commit(next)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_survives_restart_without_replaying_running_jobs_and_locks_target() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path().join("profiles.json"), None);
        let mut profile = crate::profiles::tests::example();
        let mut engine = Engine::open(store.clone(), &profile).unwrap();
        engine
            .enqueue(vec![Operation::Upload {
                local: PathBuf::from("/tmp/sample"),
                remote: PathBuf::from("/sample"),
            }])
            .unwrap();
        assert!(Engine::open(store.clone(), &profile).is_err());
        let job = engine.claim().unwrap().unwrap();
        assert_eq!(job.state, State::Running);
        drop(engine);
        let mut engine = Engine::open(store.clone(), &profile).unwrap();
        assert_eq!(engine.queue.jobs[0].state, State::Interrupted);
        assert!(engine.claim().unwrap().is_none());
        engine.retry().unwrap();
        let job = engine.claim().unwrap().unwrap();
        engine.finish(job.id, true, false).unwrap();
        assert!(!engine.queue.unfinished());
        drop(engine);
        profile.host = "another.invalid".into();
        assert!(Engine::open(store, &profile).is_err());
    }
    #[test]
    fn invalid_paths_and_excess_queue_are_rejected_atomically() {
        let root = tempfile::tempdir().unwrap();
        let profile = crate::profiles::tests::example();
        let store = Store::new(root.path().join("profiles.json"), None);
        let mut engine = Engine::open(store.clone(), &profile).unwrap();
        assert!(
            engine
                .enqueue(vec![Operation::Download {
                    local: "relative".into(),
                    remote: "/file".into()
                }])
                .is_err()
        );
        assert!(engine.queue.jobs.is_empty());
        let operations = (0..257)
            .map(|n| Operation::Upload {
                local: format!("/tmp/file-{n}").into(),
                remote: format!("/file-{n}").into(),
            })
            .collect();
        assert!(engine.enqueue(operations).is_err());
        assert!(engine.queue.jobs.is_empty());
        assert!(!store.transfer_store(&profile.name).path.exists());
        use std::os::unix::fs::symlink;
        drop(engine);
        let queue_path = store.transfer_store(&profile.name).path;
        symlink("/dev/null", queue_path).unwrap();
        assert!(Engine::open(store, &profile).is_err());
    }
    #[test]
    fn encrypted_queue_never_stores_paths_or_credentials_in_plaintext() {
        let root = tempfile::tempdir().unwrap();
        let profile = crate::profiles::tests::example();
        let password = age::x25519::Identity::generate().to_string();
        let store = Store::new(root.path().join("profiles.age"), Some(password));
        let mut engine = Engine::open(store.clone(), &profile).unwrap();
        engine
            .enqueue(vec![Operation::Download {
                local: PathBuf::from("/tmp/private-name"),
                remote: PathBuf::from("/private-name"),
            }])
            .unwrap();
        drop(engine);
        let bytes = std::fs::read(store.transfer_store(&profile.name).path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("private-name"));
        let engine = Engine::open(store, &profile).unwrap();
        assert!(engine.queue.pending());
    }
}
