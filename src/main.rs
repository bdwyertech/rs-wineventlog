#![cfg(windows)]

use roxmltree::Document;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::windows::prelude::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::System::EventLog::*;
use windows::Win32::System::Threading::CreateEventW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[derive(Deserialize)]
struct Config {
    output_file: Option<String>,
    channels: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "list-channels") {
        list_channels()?;
        return Ok(());
    }

    let exe_dir = std::env::current_exe()?.parent().unwrap().to_path_buf();
    let config_path = exe_dir.join("config.yaml");
    let config: Config = serde_yaml::from_str(&fs::read_to_string(config_path)?)?;

    let signal = unsafe { CreateEventW(None, false, false, None)? };

    const ACCESS_DENIED: i32 = 0x80070005u32 as i32;

    let subscriptions: Vec<_> = config
        .channels
        .iter()
        .filter_map(|ch| {
            let wide: Vec<u16> = ch.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                match EvtSubscribe(
                    None,
                    signal,
                    PCWSTR(wide.as_ptr()),
                    PCWSTR::null(),
                    None,
                    None,
                    None,
                    EvtSubscribeToFutureEvents.0 as u32,
                ) {
                    Ok(h) => {
                        println!("Subscribed to: {}", ch);
                        Some(h)
                    }
                    Err(e) => {
                        eprintln!("Failed to subscribe to {}: {:?}", ch, e);
                        if e.code().0 == ACCESS_DENIED {
                            eprintln!(
                                "Access denied subscribing to {} — attempting to relaunch elevated",
                                ch
                            );
                            if let Err(err) = try_elevate() {
                                eprintln!("Elevation failed: {}", err);
                            }
                            std::process::exit(1);
                        }
                        None
                    }
                }
            }
        })
        .collect();

    if subscriptions.is_empty() {
        return Err("No channels subscribed".into());
    }

    enum Output {
        File(std::fs::File),
        Stdout(io::StdoutLock<'static>),
    }

    let mut output = match &config.output_file {
        Some(path) => Output::File(OpenOptions::new().create(true).append(true).open(path)?),
        None => Output::Stdout(Box::leak(Box::new(io::stdout())).lock()),
    };

    loop {
        for sub in &subscriptions {
            unsafe {
                let mut events = [EVT_HANDLE::default(); 10];
                let mut returned = 0u32;

                let events_slice =
                    std::slice::from_raw_parts_mut(events.as_mut_ptr() as *mut isize, events.len());
                if EvtNext(*sub, events_slice, 100, 0, &mut returned).is_ok() {
                    for i in 0..returned as usize {
                        if let Some(json) = render_event(events[i]) {
                            match &mut output {
                                Output::File(f) => writeln!(f, "{}", json)?,
                                Output::Stdout(s) => writeln!(s, "{}", json)?,
                            }
                        }
                        let _ = EvtClose(events[i]);
                    }
                }
            }
        }
    }
}

unsafe fn render_event(event: EVT_HANDLE) -> Option<String> {
    unsafe {
        let mut used = 0u32;
        let mut props = 0u32;

        let _ = EvtRender(
            None,
            event,
            EvtRenderEventXml.0 as u32,
            0,
            None,
            &mut used,
            &mut props,
        );

        let mut buffer: Vec<u16> = vec![0; (used / 2) as usize + 1];

        if EvtRender(
            None,
            event,
            EvtRenderEventXml.0 as u32,
            used,
            Some(buffer.as_mut_ptr() as *mut _),
            &mut used,
            &mut props,
        )
        .is_ok()
        {
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            let xml = String::from_utf16_lossy(&buffer[..len]);

            // Parse XML and convert to JSON
            match Document::parse(&xml) {
                Ok(doc) => {
                    if let Some(root) = doc.root_element().first_element_child() {
                        let v = element_to_json(root);
                        serde_json::to_string(&v).ok()
                    } else {
                        // If root has no element children, convert the root element
                        let v = element_to_json(doc.root_element());
                        serde_json::to_string(&v).ok()
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        }
    }
}

fn try_elevate() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsStr;

    let exe = std::env::current_exe()?;
    let exe_w: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // Rebuild args as a single string (skip program name)
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
            Err("ShellExecuteW failed to launch elevated process".into())
        } else {
            Ok(())
        }
    }
}

fn element_to_json(node: roxmltree::Node) -> JsonValue {
    use std::collections::HashMap;

    let mut map = serde_json::Map::new();

    // include attributes
    for attr in node.attributes() {
        map.insert(
            format!("@{}", attr.name()),
            JsonValue::String(attr.value().to_string()),
        );
    }

    // collect element children
    let mut children: HashMap<String, Vec<JsonValue>> = HashMap::new();
    for child in node.children().filter(|n| n.is_element()) {
        let name = child.tag_name().name().to_string();
        children
            .entry(name)
            .or_default()
            .push(element_to_json(child));
    }

    for (k, v) in children {
        if v.len() == 1 {
            map.insert(k, v.into_iter().next().unwrap());
        } else {
            map.insert(k, JsonValue::Array(v));
        }
    }

    // text content (if any and no element children)
    if node.children().filter(|n| n.is_element()).count() == 0 {
        let text = node.text().unwrap_or("").trim();
        if !text.is_empty() {
            return JsonValue::String(text.to_string());
        }
    }

    JsonValue::Object(map)
}

fn list_channels() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let channel_enum = EvtOpenChannelEnum(None, 0)?;
        
        let mut buffer = vec![0u16; 512];
        loop {
            let mut used = 0u32;
            if EvtNextChannelPath(channel_enum, Some(&mut buffer), &mut used).is_ok() {
                let channel = String::from_utf16_lossy(&buffer[..used as usize - 1]);
                println!("{}", channel);
            } else {
                break;
            }
        }
        
        let _ = EvtClose(channel_enum);
    }
    Ok(())
}
