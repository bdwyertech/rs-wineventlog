use crate::{output::Output, privilege, xml};
use glob_match::glob_match;
use log::{error, info, warn};
use serde_json::Value as JsonValue;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use windows::Win32::System::EventLog::*;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, ResetEvent, WaitForSingleObject};
use windows::core::PCWSTR;

pub fn list_channels() -> Result<(), Box<dyn std::error::Error>> {
    for channel in get_available_channels()? {
        println!("{}", channel);
    }
    Ok(())
}

fn get_available_channels() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    unsafe {
        let channel_enum = EvtOpenChannelEnum(None, 0)?;
        let mut channels = Vec::new();
        let mut buffer = vec![0u16; 512];
        loop {
            let mut used = 0u32;
            if EvtNextChannelPath(channel_enum, Some(&mut buffer), &mut used).is_ok() {
                let channel = String::from_utf16_lossy(&buffer[..used as usize - 1]);
                channels.push(channel);
            } else {
                break;
            }
        }
        let _ = EvtClose(channel_enum);
        Ok(channels)
    }
}

pub fn monitor(
    channels: &[String],
    output: Output,
    pretty: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let available = get_available_channels()?;

    let mut valid_channels = Vec::new();
    for pattern in channels {
        if pattern.contains('*') || pattern.contains('?') {
            let matches: Vec<_> = available
                .iter()
                .filter(|ch| glob_match(pattern, ch))
                .cloned()
                .collect();
            if matches.is_empty() {
                warn!("No channels match pattern '{}'", pattern);
            } else {
                info!("Pattern '{}' matched {} channel(s)", pattern, matches.len());
                valid_channels.extend(matches);
            }
        } else if available.contains(pattern) {
            valid_channels.push(pattern.clone());
        } else {
            warn!("Channel '{}' does not exist, skipping", pattern);
        }
    }

    if valid_channels.is_empty() {
        return Err("No valid channels to subscribe to".into());
    }

    let output = Arc::new(Mutex::new(output));
    let mut handles = Vec::new();

    for ch in valid_channels {
        let output = Arc::clone(&output);
        let handle = thread::spawn(move || {
            if let Err(e) = monitor_channel(&ch, output, pretty) {
                error!("Error monitoring {}: {}", ch, e);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

fn monitor_channel(
    channel: &str,
    output: Arc<Mutex<Output>>,
    pretty: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create manual-reset event (TRUE for manual reset)
    let signal = unsafe { CreateEventW(None, true, true, None)? };
    let wide: Vec<u16> = channel.encode_utf16().chain(std::iter::once(0)).collect();

    let subscription = unsafe {
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
                info!("Subscribed to: {}", channel);
                h
            }
            Err(e) => {
                // Silently skip unsupported channels (Analytic/Debug)
                if e.code() == windows::Win32::Foundation::ERROR_NOT_SUPPORTED.to_hresult() {
                    return Ok(());
                }
                if e.code() == windows::Win32::Foundation::E_ACCESSDENIED {
                    error!("Access denied — attempting to relaunch elevated");
                    let _ = privilege::try_elevate();
                    std::process::exit(1);
                }
                return Err(e.into());
            }
        }
    };

    loop {
        unsafe {
            // Wait for signal (blocks until Windows signals new events)
            WaitForSingleObject(signal, INFINITE);

            // Drain all available events
            loop {
                let mut events = [EVT_HANDLE::default(); 10];
                let mut returned = 0u32;
                let events_slice =
                    std::slice::from_raw_parts_mut(events.as_mut_ptr() as *mut isize, events.len());

                // Use INFINITE timeout like Microsoft example
                if EvtNext(subscription, events_slice, INFINITE, 0, &mut returned).is_ok()
                    && returned > 0
                {
                    for i in 0..returned as usize {
                        if let Some(json) = render_event(events[i], pretty) {
                            if let Ok(mut out) = output.lock() {
                                if writeln!(*out, "{}", json).is_err() {
                                    error!("Failed to write event, output may be closed");
                                    let _ = EvtClose(events[i]);
                                    return Ok(());
                                }
                                let _ = out.flush();
                            }
                        }
                        let _ = EvtClose(events[i]);
                    }
                } else {
                    // No more events, break out of drain loop
                    break;
                }
            }

            // Manually reset the event after draining all events
            let _ = ResetEvent(signal);
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

            // Get provider name from parsed JSON
            let provider_name = v
                .get("Provider")
                .and_then(|p| p.get("@Name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());

            enrich_metadata(event, &mut v);

            // Add friendly message with provider metadata
            if let Some(prov) = provider_name {
                if let Some(msg) = format_event_message(event, &prov) {
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("Message".to_string(), JsonValue::String(msg));
                    }
                }
            }

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

unsafe fn format_event_message(event: EVT_HANDLE, provider_name: &str) -> Option<String> {
    unsafe {
        let provider_wide: Vec<u16> = provider_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // Open provider metadata
        let metadata =
            match EvtOpenPublisherMetadata(None, PCWSTR(provider_wide.as_ptr()), None, 0, 0) {
                Ok(m) => m,
                Err(_) => return None,
            };

        // Format message with provider metadata
        let mut msg_buffer_size = 0u32;
        let _ = EvtFormatMessage(
            Some(metadata),
            Some(event),
            0,
            None,
            EvtFormatMessageEvent.0 as u32,
            None,
            &mut msg_buffer_size,
        );

        if msg_buffer_size == 0 {
            let _ = EvtClose(metadata);
            return None;
        }

        let mut msg_buffer = vec![0u16; msg_buffer_size as usize];
        let result = if EvtFormatMessage(
            Some(metadata),
            Some(event),
            0,
            None,
            EvtFormatMessageEvent.0 as u32,
            Some(&mut msg_buffer),
            &mut msg_buffer_size,
        )
        .is_ok()
        {
            let len = msg_buffer
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(msg_buffer.len());
            Some(String::from_utf16_lossy(&msg_buffer[..len]))
        } else {
            None
        };

        let _ = EvtClose(metadata);
        result
    }
}
