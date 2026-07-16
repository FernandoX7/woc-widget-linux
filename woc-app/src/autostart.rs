//! XDG autostart reconciliation. The desktop file itself is the source of truth.

use crate::strings;
use std::{
    env, fs, io,
    io::Write,
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const FILE_NAME: &str = "io.github.fernandox7.wocplayercount.desktop";
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct Autostart {
    path: PathBuf,
    executable: PathBuf,
}

impl Autostart {
    pub fn discover() -> io::Result<Self> {
        let executable = env::current_exe()?;
        let config = if let Some(value) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            PathBuf::from(value)
        } else {
            env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".config"))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "neither XDG_CONFIG_HOME nor HOME is set",
                    )
                })?
        };
        Ok(Self::at(
            config.join("autostart").join(FILE_NAME),
            executable,
        ))
    }

    pub fn at(path: impl Into<PathBuf>, executable: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            executable: executable.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reports the actual entry state, including whether it still launches this binary.
    pub fn is_enabled(&self) -> io::Result<bool> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_file() => {
                Ok(fs::read_to_string(&self.path)? == self.contents()?)
            }
            Ok(_) => Ok(false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Reconciles desired state against disk. Repeated calls are no-ops.
    pub fn set_enabled(&self, enabled: bool) -> io::Result<bool> {
        if enabled {
            if self.is_enabled()? {
                return Ok(false);
            }
            let parent = self.path.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "autostart path has no parent")
            })?;
            fs::create_dir_all(parent)?;
            let metadata = fs::symlink_metadata(parent)?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "autostart parent is not a real directory",
                ));
            }
            let temporary = parent.join(format!(
                ".{FILE_NAME}.{}.{}.tmp",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            let result = (|| {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&temporary)?;
                file.write_all(self.contents()?.as_bytes())?;
                file.sync_all()?;
                fs::rename(&temporary, &self.path)
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result?;
            fs::File::open(parent)?.sync_all()?;
            Ok(true)
        } else {
            match fs::remove_file(&self.path) {
                Ok(()) => {
                    if let Some(parent) = self.path.parent() {
                        fs::File::open(parent)?.sync_all()?;
                    }
                    Ok(true)
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error),
            }
        }
    }

    fn contents(&self) -> io::Result<String> {
        Ok(format!(
            "[Desktop Entry]\nType=Application\nName={}\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            strings::PRODUCT_NAME,
            desktop_exec_arg(&self.executable)?
        ))
    }
}

fn desktop_exec_arg(path: &Path) -> io::Result<String> {
    let raw = std::str::from_utf8(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "autostart executable path is not UTF-8",
        )
    })?;
    if raw.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "autostart executable path contains a control character",
        ));
    }
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    Ok(format!("\"{escaped}\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> (PathBuf, Autostart) {
        let root = env::temp_dir().join(format!(
            "woc-autostart-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("autostart").join(FILE_NAME);
        (
            root,
            Autostart::at(&path, "/opt/WoC Player Count/woc-widget"),
        )
    }

    #[test]
    fn enable_and_disable_are_idempotent_and_leave_no_orphan() {
        let (root, entry) = fixture();
        assert!(!entry.is_enabled().unwrap());
        assert!(entry.set_enabled(true).unwrap());
        assert!(!entry.set_enabled(true).unwrap());
        assert!(entry.is_enabled().unwrap());
        assert!(fs::read_to_string(entry.path())
            .unwrap()
            .contains("Exec=\"/opt/WoC Player Count/woc-widget\""));
        assert!(entry.set_enabled(false).unwrap());
        assert!(!entry.set_enabled(false).unwrap());
        assert!(!entry.path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_entry_is_read_as_disabled_and_reconciled() {
        let (root, entry) = fixture();
        fs::create_dir_all(entry.path().parent().unwrap()).unwrap();
        fs::write(entry.path(), "[Desktop Entry]\nExec=/old/binary\n").unwrap();
        assert!(!entry.is_enabled().unwrap());
        assert!(entry.set_enabled(true).unwrap());
        assert!(entry.is_enabled().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn predictable_legacy_temp_symlink_is_never_followed() {
        let (root, entry) = fixture();
        fs::create_dir_all(entry.path().parent().unwrap()).unwrap();
        let target = root.join("target");
        fs::write(&target, "keep").unwrap();
        let legacy_temp = entry.path().with_extension("desktop.tmp");
        symlink(&target, &legacy_temp).unwrap();
        entry.set_enabled(true).unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "keep");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn exec_argument_escapes_field_codes_and_shell_metacharacters() {
        assert_eq!(
            desktop_exec_arg(Path::new("/opt/$HOME/`woc`%f")).unwrap(),
            "\"/opt/\\$HOME/\\`woc\\`%%f\""
        );
        assert!(desktop_exec_arg(Path::new("/opt/woc\nInjected=true")).is_err());
    }
}
