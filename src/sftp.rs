//! SFTP natif bloquant, appelé uniquement depuis les workers du navigateur.
use crate::profiles::{ProtocolOptions, SessionProfile};
use ssh2::{CheckResult, KnownHostFileKind, OpenFlags, OpenType, RenameFlags};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
pub struct Connection {
    _session: ssh2::Session,
    sftp: ssh2::Sftp,
    tcp: crate::ssh_route::Stream,
    _proxy: Option<crate::ssh_route::Proxy>,
}
#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub directory: bool,
    pub regular: bool,
    pub size: u64,
}
pub enum Operation {
    Batch(Vec<Operation>),
    List(PathBuf),
    Read(PathBuf),
    Save {
        path: PathBuf,
        original: Vec<u8>,
        text: Vec<u8>,
    },
    Mkdir(PathBuf),
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    Permissions {
        path: PathBuf,
        mode: u32,
    },
    Delete(PathBuf),
    Resume {
        upload: bool,
        local: PathBuf,
        remote: PathBuf,
    },
    Upload {
        local: PathBuf,
        remote: PathBuf,
    },
    Download {
        remote: PathBuf,
        local: PathBuf,
    },
}
pub enum Output {
    Entries(Vec<Entry>),
    Text { path: PathBuf, bytes: Vec<u8> },
    Done(String),
}
#[derive(Default)]
pub struct Progress {
    bytes: AtomicU64,
    entries: AtomicUsize,
}
impl Progress {
    pub fn reset(&self) {
        self.bytes.store(0, Ordering::Relaxed);
        self.entries.store(0, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> (u64, usize) {
        (
            self.bytes.load(Ordering::Relaxed),
            self.entries.load(Ordering::Relaxed),
        )
    }
}
pub struct Budget {
    bytes: u64,
    entries: usize,
    start: Instant,
    cancel: Arc<AtomicBool>,
    progress: Option<Arc<Progress>>,
}
impl Budget {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            bytes: 0,
            entries: 0,
            start: Instant::now(),
            cancel,
            progress: None,
        }
    }
    pub fn track(mut self, progress: Arc<Progress>) -> Self {
        self.progress = Some(progress);
        self
    }
    fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("Opération SFTP annulée".into())
        } else if self.start.elapsed() > Duration::from_secs(30) {
            Err("Limite SFTP de 30 secondes atteinte".into())
        } else {
            Ok(())
        }
    }
    fn entry(&mut self, depth: usize) -> Result<(), String> {
        self.check()?;
        self.entries += 1;
        if let Some(p) = &self.progress {
            p.entries.store(self.entries, Ordering::Relaxed);
        }
        if depth > 8 || self.entries > MAX_ENTRIES {
            Err("Transfert limité à 128 entrées et 8 niveaux".into())
        } else {
            Ok(())
        }
    }
    fn bytes(&mut self, count: usize) -> Result<(), String> {
        self.check()?;
        self.bytes += count as u64;
        if let Some(p) = &self.progress {
            p.bytes.store(self.bytes, Ordering::Relaxed);
        }
        if self.bytes > MAX_BYTES {
            Err("Transfert limité à 16 Mio".into())
        } else {
            Ok(())
        }
    }
}
pub fn component(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 255
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        Err("Nom simple requis, sans séparateur ni caractère de contrôle".into())
    } else {
        Ok(())
    }
}
fn path_ok(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.as_os_str().len() > 4096
        || path.to_string_lossy().chars().any(char::is_control)
    {
        Err("Chemin SFTP invalide".into())
    } else {
        Ok(())
    }
}
fn partial_path(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Nom de destination invalide")?;
    component(name)?;
    let name = format!(".{name}.rustremote-part");
    component(&name)?;
    Ok(path.with_file_name(name))
}
fn publish(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes()).map_err(error)?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes()).map_err(error)?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(
            "Destination existante ou publication atomique impossible ; aucun écrasement".into(),
        );
    }
    Ok(())
}
fn error(_: impl std::fmt::Display) -> String {
    "Erreur SFTP : opération refusée, connexion interrompue ou permissions insuffisantes".into()
}

impl Connection {
    pub fn connect(profile: &SessionProfile, password: Option<&str>) -> Result<Self, String> {
        profile.validate()?;
        if profile.simulation {
            return Err("Un profil de simulation ne contacte aucun serveur".into());
        }
        let route = crate::ssh_route::Route::resolve(profile)?;
        let profile = &route.profile;
        let ProtocolOptions::Ssh {
            identity_file,
            agent,
            known_hosts_file,
            ..
        } = &profile.options
        else {
            return Err("SFTP nécessite un profil SSH".into());
        };
        let (tcp, proxy) = route.connect()?;
        let mut session = ssh2::Session::new().map_err(error)?;
        session.set_timeout(if proxy.is_some() { 15000 } else { 5000 });
        session.set_tcp_stream(tcp.try_clone().map_err(error)?);
        session.method_pref(ssh2::MethodType::Kex,"curve25519-sha256,curve25519-sha256@libssh.org,ecdh-sha2-nistp256,ecdh-sha2-nistp384,diffie-hellman-group-exchange-sha256").map_err(error)?;
        session
            .method_pref(
                ssh2::MethodType::HostKey,
                "ssh-ed25519,ecdsa-sha2-nistp256,ecdsa-sha2-nistp384,rsa-sha2-512,rsa-sha2-256",
            )
            .map_err(error)?;
        session.handshake().map_err(|_| "Connexion SSH SFTP refusée ; pour un bastion, chargez la clé dans l’agent et validez son empreinte dans le terminal")?;
        session.set_timeout(5000);
        let mut known = session.known_hosts().map_err(error)?;
        let known_path = known_hosts_file
            .as_ref()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ssh/known_hosts"))
            })
            .ok_or("Chemin known_hosts indisponible")?;
        use std::os::unix::fs::OpenOptionsExt;
        let input = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&known_path)
            .map_err(
                |_| "known_hosts illisible : validez d’abord l’empreinte depuis le terminal SSH",
            )?;
        if !input.metadata().map_err(error)?.is_file() {
            return Err("known_hosts doit être un fichier régulier".into());
        }
        let mut content = String::new();
        input
            .take(2 * 1024 * 1024 + 1)
            .read_to_string(&mut content)
            .map_err(error)?;
        if content.len() > 2 * 1024 * 1024 {
            return Err("known_hosts limité à 2 Mio".into());
        }
        for line in content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        {
            if line.len() > 8192 {
                return Err("Ligne known_hosts trop longue".into());
            }
            known.read_str(line,KnownHostFileKind::OpenSSH).map_err(|_|"Format known_hosts non pris en charge ; aucune empreinte acceptée implicitement")?;
        }
        let (key, _) = session.host_key().ok_or("Clé d’hôte absente")?;
        if !matches!(
            known.check_port(&route.key_host, route.key_port, key),
            CheckResult::Match
        ) {
            return Err("SFTP refusé : clé d’hôte inconnue ou modifiée ; vérifiez l’empreinte dans le terminal SSH".into());
        }
        let keys: Result<(), String> = (|| {
            for key in &route.identities {
                if session.authenticated() {
                    break;
                }
                let metadata = fs::metadata(key).map_err(error)?;
                if !metadata.is_file() || metadata.len() > 256 * 1024 {
                    return Err("Clé SSH : fichier régulier de 256 Kio maximum requis".into());
                }
                let _ = session.userauth_pubkey_file(&profile.username, None, key, None);
            }
            if !session.authenticated() && *agent {
                use base64::Engine;
                let expected = if let Some(key) = identity_file {
                    let path = PathBuf::from(format!("{key}.pub"));
                    let file=fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(path).map_err(|_|"Pour utiliser cette clé protégée via l’agent, conservez son fichier .pub à côté de la clé")?;
                    if !file.metadata().map_err(error)?.is_file() {
                        return Err("Clé publique régulière requise".into());
                    }
                    let mut text = String::new();
                    file.take(16385).read_to_string(&mut text).map_err(error)?;
                    if text.len() > 16384 {
                        return Err("Clé publique trop volumineuse".into());
                    }
                    Some(
                        base64::engine::general_purpose::STANDARD
                            .decode(
                                text.split_whitespace()
                                    .nth(1)
                                    .ok_or("Clé publique invalide")?,
                            )
                            .map_err(|_| "Clé publique invalide")?,
                    )
                } else {
                    None
                };
                if let Ok(mut agent) = session.agent()
                    && agent.connect().is_ok()
                    && agent.list_identities().is_ok()
                    && let Ok(identities) = agent.identities()
                {
                    for key in identities
                        .iter()
                        .filter(|key| expected.as_ref().is_none_or(|blob| key.blob() == blob))
                        .take(3)
                    {
                        if agent.userauth(&profile.username, key).is_ok() {
                            break;
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(message) = keys
            && password.is_none()
        {
            return Err(message);
        }
        if !session.authenticated()
            && let Some(password) = password
        {
            session
                .userauth_password(&profile.username, password)
                .map_err(|_| "Authentification SFTP par mot de passe refusée")?;
        }
        if !session.authenticated() {
            return Err("SFTP nécessite une clé, SSH Agent ou le mot de passe du profil".into());
        }
        let sftp = session.sftp().map_err(error)?;
        Ok(Self {
            _session: session,
            sftp,
            tcp,
            _proxy: proxy,
        })
    }
    pub fn abort(&self) {
        self.tcp.shutdown();
    }
    fn entries(&self, path: &Path, budget: &Budget) -> Result<Vec<Entry>, String> {
        path_ok(path)?;
        let mut folder = self.sftp.opendir(path).map_err(error)?;
        let mut result = Vec::new();
        loop {
            budget.check()?;
            match folder.readdir() {
                Ok((name, stat)) => {
                    let Some(name) = name.to_str() else {
                        continue;
                    };
                    if name == "." || name == ".." {
                        continue;
                    }
                    component(name)?;
                    if result.len() >= 1024 {
                        return Err("Listing limité à 1024 entrées".into());
                    }
                    result.push(Entry {
                        name: name.into(),
                        directory: stat.is_dir(),
                        regular: stat.is_file(),
                        size: stat.size.unwrap_or(0),
                    });
                }
                Err(e) if e.code() == ssh2::ErrorCode::Session(-16) => break,
                Err(e) => return Err(error(e)),
            }
        }
        result.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(result)
    }
    fn read(&self, path: &Path, limit: usize, budget: &mut Budget) -> Result<Vec<u8>, String> {
        path_ok(path)?;
        let stat = self.sftp.lstat(path).map_err(error)?;
        if !stat.is_file() {
            return Err("Seuls les fichiers réguliers sont acceptés ; liens refusés".into());
        }
        if stat.size.unwrap_or(u64::MAX) > limit as u64 {
            return Err("Fichier trop volumineux pour cette opération".into());
        }
        let mut file = self.sftp.open(path).map_err(error)?;
        let mut result = Vec::new();
        let mut chunk = [0; 16384];
        loop {
            budget.check()?;
            let n = file.read(&mut chunk).map_err(error)?;
            if n == 0 {
                break;
            }
            if result.len() + n > limit {
                return Err("Fichier trop volumineux".into());
            }
            budget.bytes(n)?;
            result.extend_from_slice(&chunk[..n]);
        }
        Ok(result)
    }
    fn write_new(&self, path: &Path, bytes: &[u8], budget: &mut Budget) -> Result<(), String> {
        let mut file = self
            .sftp
            .open_mode(
                path,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
                0o600,
                OpenType::File,
            )
            .map_err(error)?;
        for chunk in bytes.chunks(16384) {
            budget.bytes(chunk.len())?;
            file.write_all(chunk).map_err(error)?;
        }
        file.close().map_err(error)
    }
    fn deletion_plan(
        &self,
        path: &Path,
        depth: usize,
        budget: &mut Budget,
        plan: &mut Vec<(PathBuf, bool)>,
    ) -> Result<(), String> {
        path_ok(path)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Suppression de la racine refusée")?;
        component(name)?;
        budget.entry(depth)?;
        let stat = self.sftp.lstat(path).map_err(error)?;
        if stat.is_dir() {
            for entry in self.entries(path, budget)? {
                self.deletion_plan(&path.join(entry.name), depth + 1, budget, plan)?;
            }
        } else if !stat.is_file() {
            return Err("Suppression de liens ou fichiers spéciaux refusée".into());
        }
        plan.push((path.to_path_buf(), stat.is_dir()));
        Ok(())
    }
    pub fn run(&self, operation: Operation, budget: &mut Budget) -> Result<Output, String> {
        budget.check()?;
        match operation {
            Operation::Batch(operations) => {
                if operations.is_empty()
                    || operations.len() > MAX_ENTRIES
                    || operations.iter().any(|o| matches!(o, Operation::Batch(_)))
                {
                    return Err("Lot SFTP limité à 128 opérations sans imbrication".into());
                }
                let count = operations.len();
                for (completed, operation) in operations.into_iter().enumerate() {
                    if let Err(message) = self.run(operation, budget) {
                        return Err(format!(
                            "Lot interrompu après {completed}/{count} : {message}"
                        ));
                    }
                }
                Ok(Output::Done(format!(
                    "Lot terminé : {count} opérations · {} octets",
                    budget.bytes
                )))
            }
            Operation::List(path) => Ok(Output::Entries(self.entries(&path, budget)?)),
            Operation::Read(path) => {
                let bytes = self.read(&path, 1024 * 1024, budget)?;
                std::str::from_utf8(&bytes)
                    .map_err(|_| "L’éditeur accepte uniquement le texte UTF-8")?;
                Ok(Output::Text { path, bytes })
            }
            Operation::Save {
                path,
                original,
                text,
            } => {
                if text.len() > 1024 * 1024 {
                    return Err("Édition limitée à 1 Mio".into());
                }
                if self.read(&path, 1024 * 1024, budget)? != original {
                    return Err(
                        "Le fichier distant a changé : rechargez-le avant d’enregistrer".into(),
                    );
                }
                let tag = tempfile::NamedTempFile::new().map_err(error)?;
                let temporary = path.with_file_name(format!(
                    ".rustremote-{}",
                    tag.path().file_name().unwrap().to_string_lossy()
                ));
                self.write_new(&temporary, &text, budget)?;
                if let Err(e) = self.sftp.rename(
                    &temporary,
                    &path,
                    Some(RenameFlags::ATOMIC | RenameFlags::OVERWRITE),
                ) {
                    let _ = self.sftp.unlink(&temporary);
                    return Err(error(e));
                }
                Ok(Output::Text { path, bytes: text })
            }
            Operation::Mkdir(path) => {
                path_ok(&path)?;
                self.sftp.mkdir(&path, 0o700).map_err(error)?;
                Ok(Output::Done("Dossier distant créé".into()))
            }
            Operation::Rename { from, to } => {
                path_ok(&from)?;
                path_ok(&to)?;
                if from.parent() != to.parent() {
                    return Err("Renommage dans le même dossier uniquement".into());
                }
                for path in [&from, &to] {
                    component(
                        path.file_name()
                            .and_then(|s| s.to_str())
                            .ok_or("Nom distant invalide")?,
                    )?;
                }
                let stat = self.sftp.lstat(&from).map_err(error)?;
                if !stat.is_file() && !stat.is_dir() {
                    return Err("Liens et fichiers spéciaux refusés".into());
                }
                match self.sftp.lstat(&to) {
                    Ok(_) => return Err("Destination existante ; aucun écrasement".into()),
                    Err(e) if e.code() == ssh2::ErrorCode::SFTP(2) => {}
                    Err(e) => return Err(error(e)),
                }
                self.sftp
                    .rename(&from, &to, Some(RenameFlags::empty()))
                    .map_err(error)?;
                Ok(Output::Done(
                    "Élément distant renommé · actualisez le listing".into(),
                ))
            }
            Operation::Permissions { path, mode } => {
                path_ok(&path)?;
                if mode > 0o777 {
                    return Err("Permissions octales comprises entre 000 et 777 requises".into());
                }
                let mut stat = self.sftp.lstat(&path).map_err(error)?;
                if !stat.is_file() && !stat.is_dir() {
                    return Err("Liens et fichiers spéciaux refusés".into());
                }
                stat = ssh2::FileStat {
                    size: None,
                    uid: None,
                    gid: None,
                    perm: Some(mode),
                    atime: None,
                    mtime: None,
                };
                self.sftp.setstat(&path, stat).map_err(error)?;
                Ok(Output::Done("Permissions distantes modifiées".into()))
            }
            Operation::Delete(path) => {
                let mut plan = Vec::new();
                self.deletion_plan(&path, 0, budget, &mut plan)?;
                let mut removed = 0;
                for (path, directory) in plan {
                    budget.check()?;
                    let result = if directory {
                        self.sftp.rmdir(&path)
                    } else {
                        self.sftp.unlink(&path)
                    };
                    if result.is_err() {
                        return Err(format!(
                            "Suppression interrompue après {removed} élément(s) ; actualisez le listing"
                        ));
                    }
                    removed += 1;
                }
                Ok(Output::Done(format!(
                    "Suppression terminée : {removed} élément(s) · actualisez le listing"
                )))
            }
            Operation::Resume {
                upload,
                local,
                remote,
            } => {
                let saved_partial = !upload && !local.try_exists().map_err(error)?;
                let candidate = if saved_partial {
                    partial_path(&local)?
                } else {
                    local.clone()
                };
                self.resume(upload, &candidate, &remote, budget)?;
                if saved_partial {
                    publish(&candidate, &local)?;
                }
                Ok(Output::Done(format!(
                    "Reprise terminée : {} octets vérifiés/transférés",
                    budget.bytes
                )))
            }
            Operation::Upload { local, remote } => {
                if !local.is_absolute() {
                    return Err("Chemin local absolu requis".into());
                }
                path_ok(&remote)?;
                self.upload(&local, &remote, 0, budget)?;
                Ok(Output::Done(format!(
                    "Envoi terminé : {} octets, {} entrée(s)",
                    budget.bytes, budget.entries
                )))
            }
            Operation::Download { remote, local } => {
                if !local.is_absolute() {
                    return Err("Destination locale absolue requise".into());
                }
                if fs::symlink_metadata(&local).is_ok() {
                    return Err("Destination existante ; aucun écrasement".into());
                }
                let parent = local.parent().ok_or("Destination locale invalide")?;
                let staging = tempfile::Builder::new()
                    .prefix(".rustremote-")
                    .tempdir_in(parent)
                    .map_err(error)?;
                let payload = staging.path().join("payload");
                if let Err(message) = self.download(&remote, &payload, 0, budget) {
                    if payload
                        .symlink_metadata()
                        .is_ok_and(|m| m.is_file() && m.len() > 0)
                        && let Ok(partial) = partial_path(&local)
                        && fs::File::open(&payload).and_then(|f| f.sync_all()).is_ok()
                        && publish(&payload, &partial).is_ok()
                    {
                        return Err(format!(
                            "{message} · copie partielle conservée ; sélectionnez le fichier et Reprendre le téléchargement vers la même destination"
                        ));
                    }
                    return Err(message);
                }
                publish(&payload, &local)?;
                Ok(Output::Done(format!(
                    "Téléchargement terminé : {} octets, {} entrée(s)",
                    budget.bytes, budget.entries
                )))
            }
        }
    }
    fn resume(
        &self,
        upload: bool,
        local: &Path,
        remote: &Path,
        budget: &mut Budget,
    ) -> Result<(), String> {
        use std::os::unix::fs::OpenOptionsExt;
        if !local.is_absolute() {
            return Err("Chemin local absolu requis pour la reprise".into());
        }
        path_ok(remote)?;
        budget.entry(0)?;
        let remote_stat = self.sftp.lstat(remote).map_err(error)?;
        if !remote_stat.is_file() || !fs::symlink_metadata(local).map_err(error)?.is_file() {
            return Err("Reprise réservée aux fichiers réguliers existants, sans lien".into());
        }
        let mut local_file = fs::OpenOptions::new()
            .read(true)
            .append(!upload)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(local)
            .map_err(error)?;
        local_file
            .try_lock()
            .map_err(|_| "Le fichier local est déjà utilisé")?;
        let local_stat = local_file.metadata().map_err(error)?;
        if !local_stat.is_file() {
            return Err("Fichier local régulier requis".into());
        }
        let flags = if upload {
            OpenFlags::READ | OpenFlags::WRITE | OpenFlags::APPEND
        } else {
            OpenFlags::READ
        };
        let mut remote_file = self
            .sftp
            .open_mode(remote, flags, 0o600, OpenType::File)
            .map_err(error)?;
        let remote_stat = remote_file.stat().map_err(error)?;
        if !remote_stat.is_file() {
            return Err("Fichier distant régulier requis".into());
        }
        let remote_size = remote_stat.size.ok_or("Taille distante inconnue")?;
        let local_size = local_stat.len();
        let (source_size, prefix) = if upload {
            (local_size, remote_size)
        } else {
            (remote_size, local_size)
        };
        if source_size > MAX_BYTES || prefix > source_size {
            return Err(
                "Reprise refusée : limite 16 Mio ou destination plus longue que la source".into(),
            );
        }
        let mut left = prefix;
        let mut a = [0; 16384];
        let mut b = [0; 16384];
        while left > 0 {
            budget.check()?;
            let count = (left as usize).min(a.len());
            local_file.read_exact(&mut a[..count]).map_err(error)?;
            remote_file.read_exact(&mut b[..count]).map_err(error)?;
            budget.bytes(count)?;
            if a[..count] != b[..count] {
                return Err(
                    "Préfixe existant différent : reprise refusée, aucun octet ajouté".into(),
                );
            }
            left -= count as u64;
        }
        let now = remote_file.stat().map_err(error)?;
        if now.size != remote_stat.size
            || now.mtime != remote_stat.mtime
            || local_file.metadata().map_err(error)?.len() != local_size
        {
            return Err("Fichier modifié pendant la vérification : reprise refusée".into());
        }
        loop {
            budget.check()?;
            let count = if upload {
                local_file.read(&mut a).map_err(error)?
            } else {
                remote_file.read(&mut a).map_err(error)?
            };
            if count == 0 {
                break;
            }
            budget.bytes(count)?;
            if upload {
                remote_file.write_all(&a[..count]).map_err(error)?;
            } else {
                local_file.write_all(&a[..count]).map_err(error)?;
            }
        }
        let after = remote_file.stat().map_err(error)?;
        let local_after = local_file.metadata().map_err(error)?;
        if budget.bytes > MAX_BYTES
            || if upload {
                after.size != Some(source_size)
                    || local_after.len() != source_size
                    || local_after.modified().ok() != local_stat.modified().ok()
            } else {
                local_after.len() != source_size
                    || after.size != remote_stat.size
                    || after.mtime != remote_stat.mtime
            }
        {
            return Err("Reprise interrompue : taille ou source modifiée ; vérifiez la destination partielle".into());
        }
        if !upload {
            local_file.sync_all().map_err(error)?;
        }
        remote_file.close().map_err(error)?;
        Ok(())
    }
    fn upload(
        &self,
        local: &Path,
        remote: &Path,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.entry(depth)?;
        let metadata = fs::symlink_metadata(local).map_err(error)?;
        if metadata.is_dir() {
            self.sftp.mkdir(remote, 0o700).map_err(error)?;
            for entry in fs::read_dir(local).map_err(error)? {
                let entry = entry.map_err(error)?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "Nom local non UTF-8")?;
                component(&name)?;
                self.upload(&entry.path(), &remote.join(name), depth + 1, budget)?;
            }
        } else if metadata.is_file() {
            if metadata.len() > MAX_BYTES - budget.bytes {
                return Err("Transfert limité à 16 Mio".into());
            }
            use std::os::unix::fs::OpenOptionsExt;
            let mut input = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(local)
                .map_err(error)?;
            if !input.metadata().map_err(error)?.is_file() {
                return Err("Fichier régulier requis".into());
            }
            let mut output = self
                .sftp
                .open_mode(
                    remote,
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
                    0o600,
                    OpenType::File,
                )
                .map_err(error)?;
            let mut chunk = [0; 16384];
            loop {
                budget.check()?;
                let n = input.read(&mut chunk).map_err(error)?;
                if n == 0 {
                    break;
                }
                budget.bytes(n)?;
                output.write_all(&chunk[..n]).map_err(error)?;
            }
            output.close().map_err(error)?;
        } else {
            return Err("Liens et fichiers spéciaux refusés pour l’envoi".into());
        }
        Ok(())
    }
    fn download(
        &self,
        remote: &Path,
        local: &Path,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.entry(depth)?;
        path_ok(remote)?;
        let metadata = self.sftp.lstat(remote).map_err(error)?;
        if metadata.is_dir() {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(local)
                .map_err(error)?;
            for entry in self.entries(remote, budget)? {
                component(&entry.name)?;
                self.download(
                    &remote.join(&entry.name),
                    &local.join(&entry.name),
                    depth + 1,
                    budget,
                )?;
            }
        } else if metadata.is_file() {
            use std::os::unix::fs::OpenOptionsExt;
            let mut input = self.sftp.open(remote).map_err(error)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(local)
                .map_err(error)?;
            let mut chunk = [0; 16384];
            loop {
                budget.check()?;
                let n = input.read(&mut chunk).map_err(error)?;
                if n == 0 {
                    break;
                }
                budget.bytes(n)?;
                output.write_all(&chunk[..n]).map_err(error)?;
            }
            output.sync_all().map_err(error)?;
        } else {
            return Err("Liens et fichiers spéciaux distants refusés".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_publication_never_replaces_existing_files_and_progress_stops_on_cancel() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("from");
        let to = directory.path().join("to");
        fs::write(&from, b"partial").unwrap();
        fs::write(&to, b"existing").unwrap();
        assert!(publish(&from, &to).is_err());
        assert_eq!(fs::read(&to).unwrap(), b"existing");
        assert_eq!(fs::read(&from).unwrap(), b"partial");
        assert!(partial_path(Path::new("/")).is_err());
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Progress::default());
        let mut budget = Budget::new(cancel.clone()).track(progress.clone());
        budget.entry(0).unwrap();
        budget.bytes(123).unwrap();
        assert_eq!(progress.snapshot(), (123, 1));
        cancel.store(true, Ordering::Relaxed);
        assert!(budget.bytes(1).is_err());
        assert_eq!(progress.snapshot(), (123, 1));
    }
    #[test]
    fn transfer_names_and_limits() {
        for name in ["..", "a/b", "a\\b", "x\n"] {
            assert!(component(name).is_err());
        }
        assert!(component("été.txt").is_ok());
        let cancel = Arc::new(AtomicBool::new(false));
        let mut budget = Budget::new(cancel.clone());
        assert!(budget.entry(9).is_err());
        cancel.store(true, Ordering::Relaxed);
        assert!(budget.bytes(1).is_err());
    }
}
