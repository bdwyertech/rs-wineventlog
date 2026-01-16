use crate::{output::Output, privilege, xml};
use serde_json::Value as JsonValue;
use std::io::Write;
use windows::Win32::System::EventLog::*;
use windows::Win32::System::Threading::CreateEventW;
use windows::core::PCWSTR;

pub fn list_channels() -> Result<(), Box<dyn std::error::Error>> {
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

pub fn monitor(
    channels: &[String],
    mut output: Output,
    pretty: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let signal = unsafe { CreateEventW(None, false, false, None)? };

    let subscriptions: Vec<_> = channels
        .iter()
        .filter_map(|ch| {
            let wide: Vec<u16> = ch.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                match EvtSubscribe(
                    None,
                    Some(signal),
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
                        if e.code() == windows::Win32::Foundation::E_ACCESSDENIED {
                            eprintln!("Access denied — attempting to relaunch elevated");
                            let _ = privilege::try_elevate();
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

    loop {
        for sub in &subscriptions {
            unsafe {
                let mut events = [EVT_HANDLE::default(); 10];
                let mut returned = 0u32;
                let events_slice =
                    std::slice::from_raw_parts_mut(events.as_mut_ptr() as *mut isize, events.len());
                if EvtNext(*sub, events_slice, 100, 0, &mut returned).is_ok() {
                    for i in 0..returned as usize {
                        if let Some(json) = render_event(events[i], pretty) {
                            writeln!(output, "{}", json)?;
                        }
                        let _ = EvtClose(events[i]);
                    }
                }
            }
        }
    }
}

unsafe fn render_event(event: EVT_HANDLE, pretty: bool) -> Option<String> {
    unsafe {
        let mut used = 0u32;
        let _ = EvtRender(
            None,
            event,
            EvtRenderEventXml.0 as u32,
            0,
            None,
            &mut used,
            &mut 0,
        );
        let mut buffer = vec![0u16; (used / 2) as usize + 1];

        if EvtRender(
            None,
            event,
            EvtRenderEventXml.0 as u32,
            used,
            Some(buffer.as_mut_ptr() as *mut _),
            &mut used,
            &mut 0,
        )
        .is_ok()
        {
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            let xml = String::from_utf16_lossy(&buffer[..len]);
            let mut v = xml::parse_to_json(&xml)?;
            enrich_metadata(event, &mut v);
            if pretty {
                serde_json::to_string_pretty(&v).ok()
            } else {
                serde_json::to_string(&v).ok()
            }
        } else {
            None
        }
    }
}

unsafe fn enrich_metadata(event: EVT_HANDLE, json: &mut JsonValue) {
    unsafe {
        if let Some(obj) = json.as_object_mut() {
            for (key, flag) in [
                ("Keywords", EvtFormatMessageKeyword),
                ("Level", EvtFormatMessageLevel),
                ("Task", EvtFormatMessageTask),
                ("Opcode", EvtFormatMessageOpcode),
            ] {
                if obj.contains_key(key) {
                    if let Some(s) = format_message(event, flag) {
                        obj.insert(key.to_string(), JsonValue::String(s));
                    }
                }
            }
        }
    }
}

unsafe fn format_message(event: EVT_HANDLE, format_id: EVT_FORMAT_MESSAGE_FLAGS) -> Option<String> {
    unsafe {
        let mut buffer_size = 0u32;
        let _ = EvtFormatMessage(
            None,
            Some(event),
            0,
            None,
            format_id.0 as u32,
            None,
            &mut buffer_size,
        );
        if buffer_size == 0 {
            return None;
        }

        let mut buffer = vec![0u16; buffer_size as usize];
        if EvtFormatMessage(
            None,
            Some(event),
            0,
            None,
            format_id.0 as u32,
            Some(&mut buffer),
            &mut buffer_size,
        )
        .is_ok()
        {
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            Some(String::from_utf16_lossy(&buffer[..len]))
        } else {
            None
        }
    }
}
