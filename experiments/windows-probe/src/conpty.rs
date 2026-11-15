//! Native Windows ConPTY transport. No automatic remote connection or credentials.
use crate::probe_output::{EXPECTED, MAX_OUTPUT, validate};
use std::{
    fs::File,
    io::{Read, Write},
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::{
        Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole},
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Pipes::CreatePipe,
        SystemInformation::GetSystemDirectoryW,
        Threading::{
            CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION,
            ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
            UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};
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
    _job: OwnedHandle,
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
    // Assign suspended child before it can spawn descendants. Closing the private
    // job kills only this session tree, including when the GUI process exits.
    let raw_job = unsafe { CreateJobObjectW(null(), null()) };
    if raw_job.is_null() {
        return Err(os_error("CreateJobObjectW"));
    }
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(os_error("SetInformationJobObject"));
    }
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
    // Without this flag, Windows may copy redirected parent stdio (e.g. CI tee)
    // into a ConPTY child. Explicit null stdio lets ConPTY create its own handles.
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.lpAttributeList = attributes.ptr;
    let mut information = PROCESS_INFORMATION::default();
    if unsafe {
        CreateProcessW(
            exe.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED,
            null(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut information,
        )
    } == 0
    {
        return Err(os_error("CreateProcessW"));
    }
    let child = unsafe {
        Child {
            process: OwnedHandle::from_raw_handle(information.hProcess),
            _thread: OwnedHandle::from_raw_handle(information.hThread),
            _job: job,
        }
    };
    if unsafe {
        AssignProcessToJobObject(child._job.as_raw_handle(), child.process.as_raw_handle())
    } == 0
    {
        return Err(os_error("AssignProcessToJobObject"));
    }
    if unsafe { ResumeThread(child._thread.as_raw_handle()) } == u32::MAX {
        return Err(os_error("ResumeThread"));
    }
    Ok(child)
}
pub fn run() -> Result<crate::ProbeResult, String> {
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
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || -> Result<(Vec<u8>, bool), String> {
        let mut file = File::from(output_read);
        let mut data = Vec::new();
        let mut overflow = false;
        let mut announced = false;
        let mut buffer = [0u8; 4096];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let left = MAX_OUTPUT.saturating_sub(data.len());
                    data.extend_from_slice(&buffer[..n.min(left)]);
                    overflow |= n > left;
                    if !announced
                        && std::str::from_utf8(&data).is_ok_and(|text| text.contains(EXPECTED))
                    {
                        let _ = ready_tx.try_send(());
                        announced = true;
                    }
                }
                Err(e) if e.raw_os_error() == Some(109) => break, // ERROR_BROKEN_PIPE
                Err(e) => return Err(format!("ConPTY read: {e}")),
            }
        }
        Ok((data, overflow))
    });
    let operation: Result<(), String> = (|| {
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
        // Process exit does not imply ConHost has rendered its final frame.
        // Wait for the reader's observation, never sleep blindly or close early.
        let _ = ready_rx.recv_timeout(Duration::from_secs(2));
        Ok(())
    })();
    drop(console);
    let output = reader.join().map_err(|_| "ConPTY reader interrupted")?;
    operation?;
    let (output, overflow) = output?;
    validate(&output, overflow)?;
    let screen = crate::terminal::render(&output, 30, 100)?;
    if !screen.plain.contains(EXPECTED) {
        return Err("ConPTY marker absent from rendered ANSI screen".into());
    }
    Ok(crate::ProbeResult {
        screen,
        report: format!(
            "CONPTY_OK: input/output UTF-8, ANSI screen, resize 100x30, exit 0, {} bytes, {} ms",
            output.len(),
            start.elapsed().as_millis()
        ),
    })
}

pub fn run_live(
    commands: std::sync::mpsc::Receiver<crate::live::Command>,
    state: std::sync::Arc<std::sync::Mutex<crate::live::State>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    use std::sync::{atomic::Ordering, mpsc};
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
    let reader_state = state.clone();
    // Never queue transcripts. Parse into a bounded screen; OSC buffer is fixed
    // (vt100/vte std feature disabled), zero scrollback, fixed-size cells.
    let reader = std::thread::spawn(move || -> Result<(), String> {
        let mut output = File::from(output_read);
        let mut bytes = [0u8; 4096];
        loop {
            match output.read(&mut bytes) {
                Ok(0) => return Ok(()),
                Ok(n) => {
                    let mut state = reader_state
                        .lock()
                        .map_err(|_| "Terminal state interrupted")?;
                    state.parser.process(&bytes[..n]);
                    state.revision = state.revision.wrapping_add(1);
                }
                Err(e) if e.raw_os_error() == Some(109) => return Ok(()),
                Err(_) => return Err("Terminal output pipe failed".into()),
            }
        }
    });
    // Writes can block when an application stops consuming input. Keep the
    // controller/stop path separate; closing the job/console breaks the pipe.
    let (write_tx, write_rx) = mpsc::sync_channel::<Vec<u8>>(8);
    let writer = std::thread::spawn(move || -> Result<(), String> {
        let mut input = File::from(input_write);
        while let Ok(bytes) = write_rx.recv() {
            input
                .write_all(&bytes)
                .map_err(|_| "Terminal input pipe failed")?;
        }
        Ok(())
    });
    let operation: Result<String, String> = (|| {
        let child = spawn(console.0)?;
        {
            let mut state = state.lock().map_err(|_| "Terminal state interrupted")?;
            state.status = "Local CMD — running".into();
            state.revision = state.revision.wrapping_add(1);
        }
        let mut dimensions = (80, 24);
        loop {
            if stop.load(Ordering::Acquire) {
                return Ok("Local terminal stopped".into());
            }
            if unsafe { WaitForSingleObject(child.process.as_raw_handle(), 0) } == WAIT_OBJECT_0 {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(child.process.as_raw_handle(), &mut code) } == 0 {
                    return Err(os_error("GetExitCodeProcess"));
                }
                // Let ConHost flush while its output is still drained.
                return Ok(format!("Local terminal exited ({code})"));
            }
            if reader.is_finished() || writer.is_finished() {
                return Err("Terminal I/O worker stopped".into());
            }
            match commands.recv_timeout(Duration::from_millis(20)) {
                Ok(crate::live::Command::Input(bytes)) => {
                    if !bytes.is_empty() && bytes.len() <= 4096 {
                        // Fail visibly rather than silently dropping typed commands.
                        write_tx
                            .try_send(bytes)
                            .map_err(|_| "Terminal input queue full; session stopped")?;
                    }
                }
                Ok(crate::live::Command::Resize(cols, rows)) => {
                    if !(20..=160).contains(&cols) || !(5..=60).contains(&rows) {
                        return Err("Invalid terminal dimensions".into());
                    }
                    if dimensions != (cols, rows) {
                        if unsafe {
                            ResizePseudoConsole(
                                console.0,
                                COORD {
                                    X: cols as i16,
                                    Y: rows as i16,
                                },
                            )
                        } < 0
                        {
                            return Err("ConPTY resize failed".into());
                        }
                        let mut state = state.lock().map_err(|_| "Terminal state interrupted")?;
                        state.parser.set_size(
                            std::num::NonZeroU16::new(rows).unwrap(),
                            std::num::NonZeroU16::new(cols).unwrap(),
                        );
                        state.revision = state.revision.wrapping_add(1);
                        dimensions = (cols, rows);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok("Local terminal closed".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        // Child's private job closes here, killing remaining descendants.
    })();
    drop(write_tx);
    // Close off the GTK thread with the output reader still draining: required
    // to avoid ClosePseudoConsole deadlocks on Windows Server 2022/older Win11.
    drop(console);
    let read_result = reader.join().map_err(|_| "Terminal reader interrupted")?;
    let _ = writer.join(); // Broken pipe during deliberate shutdown is expected.
    read_result?;
    operation
}
