//! Runtime paths for a self-contained `RustRemote.app` built by
//! `scripts/build-macos-app.py`. Does nothing outside such a bundle, so
//! `cargo run` and the `--homebrew-runtime` bundle keep using Homebrew.

use std::path::{Path, PathBuf};

const CACHE_TEMPLATE: &str = "lib/gdk-pixbuf-2.0/2.10.0/loaders.cache.in";
const LOADERS: &str = "lib/gdk-pixbuf-2.0/2.10.0/loaders";
const PLACEHOLDER: &str = "@RUSTREMOTE_PIXBUF_LOADERS@";

/// Must run first in `main`, before GTK or any other thread starts.
pub fn configure() {
    let Some(resources) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent()?.parent().map(|c| c.join("Resources")))
    else {
        return;
    };
    let Ok(template) = std::fs::read_to_string(resources.join(CACHE_TEMPLATE)) else {
        return;
    };
    let share = resources.join("share");
    let mut vars = vec![
        ("XDG_DATA_DIRS", share.clone()),
        ("GSETTINGS_SCHEMA_DIR", share.join("glib-2.0/schemas")),
        // Never load Homebrew's GIO/GTK modules into the bundled GLib.
        ("GIO_MODULE_DIR", resources.join("lib/gio/modules")),
        ("GTK_PATH", resources.join("lib/gtk-4.0")),
    ];
    match write_loader_cache(&template, &resources.join(LOADERS)) {
        Ok(cache) => vars.push(("GDK_PIXBUF_MODULE_FILE", cache)),
        Err(error) => eprintln!("RustRemote: image loaders unavailable: {error}"),
    }
    for (name, value) in vars {
        // SAFETY: called at the top of `main`, while the process is single-threaded.
        unsafe { std::env::set_var(name, value) };
    }
}

/// The signed bundle is read-only, so the absolute loader paths go to the cache.
fn write_loader_cache(template: &str, loaders: &Path) -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|home| Path::new(home).is_absolute())
        .ok_or_else(|| std::io::Error::other("HOME is not an absolute path"))?;
    let dir = Path::new(&home).join("Library/Caches/com.rustremote.Preview");
    std::fs::create_dir_all(&dir)?;
    let content = render_cache(template, loaders)?;
    let target = dir.join("loaders.cache");
    if std::fs::read_to_string(&target).ok().as_deref() != Some(content.as_str()) {
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        std::io::Write::write_all(&mut file, content.as_bytes())?;
        file.persist(&target).map_err(|e| e.error)?;
    }
    Ok(target)
}

fn render_cache(template: &str, loaders: &Path) -> std::io::Result<String> {
    let loaders = loaders
        .to_str()
        .filter(|path| !path.contains('"') && !path.contains('\\'))
        .ok_or_else(|| std::io::Error::other("unsupported bundle path"))?;
    Ok(template.replace(PLACEHOLDER, loaders))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_points_into_the_bundle_and_rejects_quoting() {
        let template = format!("\"{PLACEHOLDER}/libpixbufloader_svg.so\"\n\"svg\" 6\n");
        let rendered =
            render_cache(&template, Path::new("/Applications/RustRemote.app/L")).unwrap();
        assert_eq!(
            rendered,
            "\"/Applications/RustRemote.app/L/libpixbufloader_svg.so\"\n\"svg\" 6\n"
        );
        assert!(render_cache(&template, Path::new("/tmp/a\"b")).is_err());
    }
}
