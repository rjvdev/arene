//! Outils de stockage en fichiers : écriture atomique, listes, Git.
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// Verrou qui survit à une requête qui aurait planté en le tenant.
pub fn l<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Lecture stricte : fichier absent -> valeur par défaut ; fichier illisible -> erreur explicite.
pub fn lire_toml<T: DeserializeOwned + Default>(p: &Path) -> Result<T, String> {
    match fs::read_to_string(p) {
        Ok(s) => toml::from_str(&s).map_err(|e| format!("{} : {}", p.display(), e.to_string().replace('\n', " "))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("{} : {e}", p.display())),
    }
}

/// Écriture atomique : fichier temporaire puis renommage.
pub fn ecrire_atomique(p: &Path, contenu: &[u8]) -> Result<(), String> {
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let nom = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = p.with_file_name(format!(".{nom}.tmp"));
    let mut f = fs::File::create(&tmp).map_err(|e| format!("{} : {e}", tmp.display()))?;
    f.write_all(contenu).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    fs::rename(&tmp, p).map_err(|e| format!("{} : {e}", p.display()))
}

pub fn ecrire_toml<T: Serialize>(p: &Path, v: &T) -> Result<(), String> {
    let s = toml::to_string_pretty(v).map_err(|e| e.to_string())?;
    ecrire_atomique(p, s.as_bytes())
}

/// Liste récursive des fichiers d'un dossier, triée (fichiers cachés et temporaires exclus).
pub fn fichiers(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(true) {
                continue;
            }
            if p.is_dir() {
                out.extend(fichiers(&p));
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

pub fn sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

pub fn maintenant() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn nouveau_jeton() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ------------------------------------------------------------------ git
static GIT: OnceLock<mpsc::Sender<String>> = OnceLock::new();

/// Démarre le dépôt et un fil unique qui regroupe les commits (au plus un toutes les 5 secondes).
pub fn git_demarrer(racine: &Path) {
    let r = racine.to_path_buf();
    let git = |args: &[&str]| {
        let _ = Command::new("git").args(args).current_dir(&r).output();
    };
    if !r.join(".git").exists() {
        git(&["init", "-q"]);
    }
    git(&["config", "user.name", "Arène"]);
    git(&["config", "user.email", "arene@localhost"]);

    let (tx, rx) = mpsc::channel::<String>();
    let _ = GIT.set(tx);
    std::thread::spawn(move || {
        while let Ok(premier) = rx.recv() {
            let mut msgs = vec![premier];
            let fin = Instant::now() + Duration::from_secs(5);
            loop {
                let reste = fin.saturating_duration_since(Instant::now());
                if reste.is_zero() {
                    break;
                }
                match rx.recv_timeout(reste) {
                    Ok(m) => msgs.push(m),
                    Err(_) => break,
                }
            }
            let message = if msgs.len() == 1 {
                msgs.remove(0)
            } else {
                let n = msgs.len();
                let liste: Vec<String> = msgs.into_iter().take(50).collect();
                format!("{n} modifications\n\n{}", liste.join("\n"))
            };
            // uniquement ce que l'arène gère : jamais le code source
            let _ = Command::new("git")
                .args(["add", "-A", "--", "donnees/evolution", "contenu", "agents-locaux"])
                .current_dir(&r).output();
            let _ = Command::new("git").args(["commit", "-q", "-m", &message]).current_dir(&r).output();
        }
    });
}

pub fn git_commit(message: String) {
    if let Some(tx) = GIT.get() {
        let _ = tx.send(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct T {
        n: u32,
    }

    #[test]
    fn ecriture_atomique_et_lecture_stricte() {
        let d = std::env::temp_dir().join(format!("arene-test-{}", nouveau_jeton()));
        let p = d.join("x.toml");
        ecrire_toml(&p, &T { n: 4 }).unwrap();
        assert_eq!(lire_toml::<T>(&p).unwrap().n, 4);
        assert!(fichiers(&d).iter().all(|f| !f.to_string_lossy().contains(".tmp")));
        fs::write(&p, "n = = 3").unwrap();
        assert!(lire_toml::<T>(&p).is_err(), "un fichier cassé doit être signalé");
        assert_eq!(lire_toml::<T>(&d.join("absent.toml")).unwrap().n, 0);
        let _ = fs::remove_dir_all(d);
    }
}
