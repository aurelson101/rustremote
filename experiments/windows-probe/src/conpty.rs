//! Native Windows transport proof. Fixed local commands only; no network or secrets.
use std::{
    fs::File,
    io::{Read, Write},
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::{
        Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole},
        Pipes::CreatePipe,
        SystemInformation::GetSystemDirectoryW,
        Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
            GetExitCodeProcess, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTUPINFOEXW,
            TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};
const MAX_OUTPUT: usize = 64 * 1024;
fn os_error(step: &str) -> String {
    format!("{step}: {}", std::io::Error::last_os_error())
}
fn pipe() -> Result<(OwnedHandle, OwnedHandle), String> {
    let (mut read, mut write) = (null_mut(), null_mut());
    // The handles are returned only on success and each is owned exactly once.
    if unsafe { CreatePipe(&mut read, &mut write, null(), 0) } == 0 {
        return Err(os_error("CreatePipe"));
    }
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}
struct Console(HPCON);
impl Drop for Console {
    fn drop(&mut self) {
        unsafe {
            ClosePseudoConsole(self.0);
        }
    }
}
struct Attributes {
    _storage: Vec<usize>,
    ptr: LPPROC_THREAD_ATTRIBUTE_LIST,
}
impl Attributes {
    fn new(console: HPCON) -> Result<Self, String> {
        let mut bytes = 0;
        // First call obtains the size. Storage is word-aligned and is never resized.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(os_error("Attribute size"));
        }
        let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let ptr = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(ptr, 1, 0, &mut bytes) } == 0 {
            return Err(os_error("Attribute initialization"));
        }
        let attributes = Self {
            _storage: storage,
            ptr,
        };
        if unsafe {
            UpdateProcThreadAttribute(
                ptr,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                console as *const _,
                size_of::<HPCON>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(os_error("ConPTY attribute"));
        }
        Ok(attributes)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.ptr);
        }
    }
}
struct Child {
    process: OwnedHandle,
    _thread: OwnedHandle,
}
impl Drop for Child {
    fn drop(&mut self) {
        // Only this probe's process is terminated; never enumerate or kill others.
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
            unsafe {
                TerminateProcess(self.process.as_raw_handle(), 1);
                WaitForSingleObject(self.process.as_raw_handle(), 2000);
            }
        }
    }
}
fn spawn(console: HPCON) -> Result<Child, String> {
    let attributes = Attributes::new(console)?;
    let mut directory = vec![0u16; 32768];
    let len =
        unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
    if len == 0 || len >= directory.len() {
        return Err(os_error("System directory"));
    }
    directory.truncate(len);
    let mut cwd = directory.clone();
    cwd.push(0);
    directory.extend("\\cmd.exe".encode_utf16());
    let mut exe = directory.clone();
    exe.push(0);
    let mut command: Vec<u16> = format!("\"{}\" /d /q", String::from_utf16_lossy(&directory))
        .encode_utf16()
        .chain([0])
        .collect();
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.lpAttributeList = attributes.ptr;
    let mut information = PROCESS_INFORMATION::default();
    if unsafe {
        CreateProcessW(
            exe.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT,
            null(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut information,
        )
    } == 0
    {
        return Err(os_error("CreateProcessW"));
    }
    Ok(unsafe {
        Child {
            process: OwnedHandle::from_raw_handle(information.hProcess),
            _thread: OwnedHandle::from_raw_handle(information.hThread),
        }
    })
}
pub fn run() -> Result<String, String> {
    let start = Instant::now();
    let (input_read, input_write) = pipe()?;
    let (output_read, output_write) = pipe()?;
    let mut handle = 0;
    let result = unsafe {
        CreatePseudoConsole(
            COORD { X: 80, Y: 24 },
            input_read.as_raw_handle(),
            output_write.as_raw_handle(),
            0,
            &mut handle,
        )
    };
    if result < 0 {
        return Err(format!("CreatePseudoConsole HRESULT {result:#x}"));
    }
    let console = Console(handle);
    drop(input_read);
    drop(output_write);
    // Drain output concurrently, including while ClosePseudoConsole completes.
    // Retain a bounded transcript; discard excess instead of blocking the producer.
    let reader = std::thread::spawn(move || -> Result<(Vec<u8>, bool), String> {
        let mut file = File::from(output_read);
        let mut data = Vec::new();
        let mut overflow = false;
        let mut buffer = [0u8; 4096];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let left = MAX_OUTPUT.saturating_sub(data.len());
                    data.extend_from_slice(&buffer[..n.min(left)]);
                    overflow |= n > left;
                }
                Err(e) if e.raw_os_error() == Some(109) => break, // ERROR_BROKEN_PIPE
                Err(e) => return Err(format!("ConPTY read: {e}")),
            }
        }
        Ok((data, overflow))
    });
    let operation = (|| {
        let child = spawn(console.0)?;
        if unsafe { ResizePseudoConsole(console.0, COORD { X: 100, Y: 30 }) } < 0 {
            return Err("ConPTY resize failed".into());
        }
        let mut input = File::from(input_write);
        // The complete expected marker never appears in the input, so terminal
        // input echo alone cannot produce a false success. UTF-8 is tested too.
        input.write_all("set RR_A=RUSTREMOTE_CONPTY_\r\nset RR_B=OK_été\r\necho %RR_A%%RR_B%\r\nexit /b 0\r\n".as_bytes()).map_err(|_| "ConPTY write failed")?;
        if unsafe { WaitForSingleObject(child.process.as_raw_handle(), 10_000) } != WAIT_OBJECT_0 {
            return Err("ConPTY child timeout (10s)".into());
        }
        let mut exit = 1;
        if unsafe { GetExitCodeProcess(child.process.as_raw_handle(), &mut exit) } == 0 || exit != 0
        {
            return Err("ConPTY child failed".into());
        }
        Ok(())
    })();
    drop(console);
    let output = reader.join().map_err(|_| "ConPTY reader interrupted")?;
    operation?;
    let (output, overflow) = output?;
    if overflow {
        return Err("ConPTY output exceeded 64 KiB".into());
    }
    let text = String::from_utf8_lossy(&output);
    if !text.contains("RUSTREMOTE_CONPTY_OK_été") {
        return Err("ConPTY UTF-8 output marker missing".into());
    }
    Ok(format!(
        "CONPTY_OK: input/output UTF-8, resize 100x30, exit 0, {} bytes, {} ms",
        output.len(),
        start.elapsed().as_millis()
    ))
}
