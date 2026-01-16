use std::ffi::OsStr;
use std::os::windows::prelude::OsStrExt;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

pub fn try_elevate() -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let exe_w: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let args: String = std::env::args().skip(1).fold(String::new(), |mut acc, a| {
        if !acc.is_empty() {
            acc.push(' ');
        }
        if a.contains(' ') {
            acc.push('"');
            acc.push_str(&a);
            acc.push('"');
        } else {
            acc.push_str(&a);
        }
        acc
    });

    let verb = OsStr::new("runas");
    let verb_w: Vec<u16> = verb.encode_wide().chain(std::iter::once(0)).collect();
    let args_w: Vec<u16> = OsStr::new(&args)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let h = ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            if args.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(args_w.as_ptr())
            },
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        if (h.0 as isize) <= 32 {
            Err("ShellExecuteW failed".into())
        } else {
            Ok(())
        }
    }
}
