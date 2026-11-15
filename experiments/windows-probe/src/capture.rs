//! Capture only this GTK window, never the user's desktop or other applications.
use gtk::prelude::*;
use std::{io::Write, path::Path};

pub fn window(window: &gtk::ApplicationWindow, path: &Path) -> Result<(), String> {
    if !window.is_mapped() || window.width() <= 0 || window.height() <= 0 {
        return Err("Cannot capture an unmapped window".into());
    }
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    let node = snapshot.to_node().ok_or("Empty GTK snapshot")?;
    let renderer = window.renderer().ok_or("GTK renderer unavailable")?;
    let texture = renderer.render_texture(&node, None);
    let bytes = texture.save_to_png_bytes();
    // This optional test capture cannot silently overwrite an existing file.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Cannot create GTK capture (parent missing or file already exists)")?;
    file.write_all(&bytes)
        .map_err(|_| "Cannot write GTK capture".to_string())
}

/// Exercise an actual OS resize, rather than changing GTK's initial default size.
/// Only the visible window with this probe's title AND process ID may be touched.
#[cfg(windows)]
pub fn resize_own_probe_window(width: i32, height: i32) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::RECT,
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            FindWindowW, GetClientRect, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
        },
    };
    if !(320..=1600).contains(&width) || !(240..=1200).contains(&height) {
        return Err("Invalid probe resize budget".into());
    }
    let title: Vec<u16> = "RustRemote Windows probe"
        .encode_utf16()
        .chain([0])
        .collect();
    unsafe {
        let handle = FindWindowW(std::ptr::null(), title.as_ptr());
        let mut pid = 0;
        if handle.is_null()
            || IsWindowVisible(handle) == 0
            || GetWindowThreadProcessId(handle, &mut pid) == 0
            || pid != GetCurrentProcessId()
        {
            return Err("Cannot identify this process's probe window".into());
        }
        let mut outer = RECT::default();
        let mut client = RECT::default();
        if GetWindowRect(handle, &mut outer) == 0 || GetClientRect(handle, &mut client) == 0 {
            return Err("Cannot measure this process's probe window".into());
        }
        let border_width = (outer.right - outer.left) - (client.right - client.left);
        let border_height = (outer.bottom - outer.top) - (client.bottom - client.top);
        if !(0..=256).contains(&border_width) || !(0..=256).contains(&border_height) {
            return Err("Unexpected probe window decorations".into());
        }
        if SetWindowPos(
            handle,
            std::ptr::null_mut(),
            0,
            0,
            width + border_width,
            height + border_height,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
        ) == 0
        {
            return Err("Native probe window resize failed".into());
        }
    }
    Ok(())
}
