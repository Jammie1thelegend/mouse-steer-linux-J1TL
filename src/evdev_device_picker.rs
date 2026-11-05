
use evdev::{EventType, RelativeAxisCode, KeyCode, Device, InputEvent};

use std::fs;
use std::thread;
use std::time;

use log::{error, warn, info};
use thiserror::Error;

use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};

use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

#[derive(Error, Debug)]
pub enum DevicePickerError {
    #[error("Failed to find a compatible input device. Make sure you are running the application with root priviledges.")]
    NoDeviceError,
}

pub fn select_input_device(filter_evtype: EventType, filter_rels: Vec<RelativeAxisCode>, filter_keys: Vec<KeyCode>, devname:&str)-> Result<(Device, String), DevicePickerError> {
    // find all input devices that can be used as a specific type of device

    let dev_input_paths: Vec<_> = fs::read_dir("/dev/input/by-id/")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.path().into_os_string().to_str().map(String::from)).collect();

    let mut devices: Vec<Device> = Vec::new();
    let mut paths:Vec<String> = Vec::new();
    for p in dev_input_paths {
        let d = Device::open(&p).ok().filter(|device| device.supported_events().contains(filter_evtype));
        match d {
            Some(d) => {

                if d.supported_relative_axes().map_or(false, |keys| {
                    let mut correct_rels = 0;

                    for fr in &filter_rels {if keys.contains(*fr) {correct_rels += 1}}
                    
                    if correct_rels == filter_rels.len() {true}
                    else {false}
                    }
                ) == false {if filter_rels[0] == RelativeAxisCode::REL_MISC {} else {continue;}}
                
                if d.supported_keys().map_or(false, |keys| {
                    let mut correct_keys = 0;
                    for fk in &filter_keys {if keys.contains(*fk) {correct_keys += 1}};
                    if correct_keys == filter_keys.len() {true}
                    else {false}
                    }
                ) == false {continue}

                devices.push(d); 
                paths.push(p);

                },
            _ => continue
        }
    }
    
    if devices.is_empty() {
        error!("{}", DevicePickerError::NoDeviceError);
        return Err(DevicePickerError::NoDeviceError);
    }

    if devices.len() == 1 {warn!("Only one compatible {} found! ({})", devname, devices[0].name().unwrap_or("Unknown Device"));}

    // await user input to confirm device is at least able to to required functions, despite the fact that it should be able to (some devices are not real)
    if filter_rels[0] == RelativeAxisCode::REL_MISC {
        println!("Awaiting input to choose device, please do the following inputs on your {}: press {:?}. (Does not need to happen at the same time)", devname, filter_keys);
    }
    else {
        println!("Awaiting input to choose device, please do the following inputs on your {}: move {:?}, and press {:?}. (Does not need to happen at the same time)", devname, filter_rels, filter_keys);
    }

    let rt = Runtime::new().unwrap();

    let path = rt.block_on(get_device_interactive(paths, filter_rels, filter_keys));
    
    let input_device = Device::open(&path).unwrap();
    info!("Using \"{}\" as {} input device", input_device.name().unwrap_or("Unknown Device"), devname);
    Ok((input_device, path))

}

struct ThrVariables {
    path: String, 
    req_rels: Vec<RelativeAxisCode>, 
    req_keys: Vec<KeyCode>, 
    req_num: i32,
    tx: tokio::sync::mpsc::Sender<String>,
    dev_found: Arc<AtomicBool>
}


async fn get_device_interactive(dev_paths: Vec<String>, filter_rels: Vec<RelativeAxisCode>, filter_keys: Vec<KeyCode>) -> String {

    let mut async_thrs: Vec<JoinHandle<()>> = vec![];

    let req_num = filter_keys.len() as i32 + filter_rels.len() as i32;

    let (dftx, mut dfrx) = tokio::sync::mpsc::channel(32); // device found channel
    let dev_found = Arc::new(AtomicBool::new(false));

    for p in dev_paths {

        let thr_variables = ThrVariables {
            path: p.to_string(),
            req_rels: filter_rels.clone(),
            req_keys: filter_keys.clone(),
            req_num: req_num,
            tx: dftx.clone(),
            dev_found: dev_found.clone()
        };

        async_thrs.push(spawn_async_thr(thr_variables).await.unwrap());
    }

    //thread::sleep(time::Duration::from_millis(10000));

    let path: String = dfrx.recv().await.unwrap();

    dev_found.store(true, Ordering::Release);

    for a in async_thrs {
        let mut count = 0;
        while a.is_finished() == false {
            thread::sleep(time::Duration::from_millis(10));
            count += 1;
            if count >= 100 {
                a.abort();
            }
        }
    }

    return path
}


async fn spawn_async_thr(mut thr_variables: ThrVariables) -> Result<JoinHandle<()>, std::io::Error> {
    let device = Device::open(&thr_variables.path)?;

    let mut events: evdev::EventStream = device.into_event_stream()?;

    let async_thr = tokio::spawn(async move {

        let mut req_count = 0;

        if thr_variables.req_rels[0] == RelativeAxisCode::REL_MISC {req_count += 1}

        'thread_loop: loop {
            if thr_variables.dev_found.load(Ordering::Relaxed) == true {
                break 'thread_loop
            }

            let ev = match events.next_event().await {
                Ok(e) => {e}, 
                Err(_) => {InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_MISC.0, 0)}
            };

            let ev_code = ev.code();

            if ev.event_type() == EventType::RELATIVE {
                
                let mut rem_req_rels: Vec<RelativeAxisCode> = vec![];

                for rr in &thr_variables.req_rels {
                    if ev_code == rr.0 {req_count += 1; println!("{:?} moved", rr);  continue};
                    rem_req_rels.push(*rr); // * dereferences
                }

                thr_variables.req_rels = rem_req_rels;
                
            }
            if ev.event_type() == EventType::KEY {
                let mut rem_req_keys: Vec<KeyCode> = vec![];

                for rk in &thr_variables.req_keys {
                    if ev_code == rk.0 {req_count += 1; println!("{:?} pressed", rk);  continue};
                    rem_req_keys.push(*rk); // * dereferences
                }
                thr_variables.req_keys = rem_req_keys;
            }
            

            if req_count == thr_variables.req_num {thr_variables.tx.send(thr_variables.path).await.unwrap(); break 'thread_loop}

        }
    });

    return Ok(async_thr)
}