use evdev::{
    AbsInfo, AbsoluteAxisCode, EventSummary, Device, EventType, InputEvent, KeyCode, RelativeAxisCode, UinputAbsSetup, uinput::VirtualDevice
};
use std::fs;
use thiserror::Error;
use log::{info, warn, error, LevelFilter};
use env_logger::Builder;

use std::sync::mpsc;
use std::thread;

use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};

mod configuration;
use configuration::Config;

mod window;
use window::create_window;

const VJOYSTICK_NAME: &str = "mouse-steer-linux-J1TL";

// virtual joystick buttons, won't be used but increase chances of joystick being recognized
static KEYS: [KeyCode; 14] = [
    KeyCode::BTN_EAST,
    KeyCode::BTN_SOUTH,
    KeyCode::BTN_NORTH,
    KeyCode::BTN_WEST,
    KeyCode::BTN_DPAD_UP,
    KeyCode::BTN_DPAD_DOWN,
    KeyCode::BTN_DPAD_LEFT,
    KeyCode::BTN_DPAD_RIGHT,
    KeyCode::BTN_SELECT,
    KeyCode::BTN_START,
    KeyCode::BTN_TL,
    KeyCode::BTN_TR,
    KeyCode::BTN_TL2,
    KeyCode::BTN_TR2,
];

#[derive(Error, Debug)]
pub enum MouseSteerError {
    #[error("Failed to find a compatible input device. Make sure you are running the application with root priviledges.")]
    NoDeviceError,

    #[error("Failed to read a mouse input")]
    FailedToReadInput,
}

fn main() -> Result<(), MouseSteerError> {

    // initialize logger
    Builder::new()
        .filter_level(LevelFilter::Trace)  // This shows everything
        .init();

    let conf = load_config();
    info!("sensitivity: {}", conf.sensitivity);
    

    // select mouse device
    let mouse_and_path = select_input_device(EventType::RELATIVE, 
        vec![RelativeAxisCode::REL_X, RelativeAxisCode::REL_Y], 
        vec![KeyCode::BTN_LEFT], 
        "mouse").unwrap();
    
    let mut mouse = mouse_and_path.0;

    //select keyboard device
    let mut keyboard = select_input_device(EventType::KEY, 
        vec![RelativeAxisCode::REL_MISC], 
        vec![KeyCode::KEY_F5, KeyCode::KEY_F7, KeyCode::KEY_F8], 
        "keyboard").unwrap().0;
    

    // set up virtual joystick
    let axis_info = AbsInfo::new(conf.value(), conf.range_min(), conf.range_max(), conf.fuzz(), conf.flat(), conf.resolution());
    let mut joystick = create_joystick(axis_info, VJOYSTICK_NAME).unwrap();
    info!("Virtual joystick created");



    //create thread to detect keyboard presses
    let (tx, rx) = mpsc::channel();
    let mut thr_mouse = Device::open(&mouse_and_path.1).unwrap();
    thread::spawn(move || {
        let mut mouse_steer_active: bool = false;
        let mut quit_combo: [bool; 3] = [false, false, false];
        loop{
            for ev in keyboard.fetch_events().unwrap(){
                match ev.destructure() {
                    //example
                    EventSummary::Key(_, KeyCode::KEY_F6, 1) => {
                        if mouse_steer_active == true {
                        mouse_steer_active = false;
                        tx.send(false).unwrap();
                        warn!("mouse-steer-linux-J1TL off");
                        warn!("!! Program still running, press ctrl+C to stop.");
                        continue
                    }
                    warn!("mouse-steer-linux-J1TL on");

                    let _ = thr_mouse.send_events(&mouse_move_evs(i32::MIN, i32::MIN));
                    let _ = thr_mouse.send_events(&mouse_move_evs(i32::MIN, i32::MIN));
                    let _ = thr_mouse.grab();
                    let _ = thr_mouse.ungrab(); 
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.send_events(&mouse_move_evs(1920/2, 1080/2));
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.grab();
                    let _ = thr_mouse.ungrab(); 

                    mouse_steer_active = true;
                    tx.send(true).unwrap();
                        },
                    EventSummary::Key(_, KeyCode::KEY_F5, _) => {quit_combo[0] = match ev.value() {0 => false, 1 => true, _ => false}},
                    EventSummary::Key(_, KeyCode::KEY_F7, _) => {quit_combo[1] = match ev.value() {0 => false, 1 => true, _ => false}},
                    EventSummary::Key(_, KeyCode::KEY_F8, _) => {quit_combo[2] = match ev.value() {0 => false, 1 => true, _ => false}},
                    _ => {}
                }
            };
            if quit_combo == [true, true, true] {panic!("__ ___ FORCE QUIT ___ __");}
        }
    });
    thread::spawn(move || { create_window()});

    // fetch events and send them through to virtual joystick
    let min: i32 = conf.range_min();
    let max: i32 = conf.range_max();
    let mut mouse_x_pos: i32 = 0;
    let mut joystick_x_pos: i32;
    let mut mouse_steer_active: bool = false;

    let mut mouse_main_emit = Device::open(&mouse_and_path.1).unwrap();

    loop {
        for ev in mouse.fetch_events().unwrap(){
                match ev.destructure() {
                    EventSummary::RelativeAxis(_, RelativeAxisCode::REL_X, _) => {
                        mouse_steer_active = match rx.try_recv() {Ok(bool) => bool, Err(_e) => mouse_steer_active};
                        if mouse_steer_active == false {continue}
                        mouse_x_pos += ev.value();
                        joystick_x_pos = mouse_x_pos;

                        if joystick_x_pos < min {
                            let _ = mouse_main_emit.send_events(&mouse_move_evs(min - joystick_x_pos, 0));
                            joystick_x_pos = min
                        }
                        if joystick_x_pos > max {
                            let _ = mouse_main_emit.send_events(&mouse_move_evs(max - joystick_x_pos, 0));
                            joystick_x_pos = max
                        }
                        let ev = InputEvent::new(
                            EventType::ABSOLUTE.0,
                            AbsoluteAxisCode::ABS_X.0,
                            joystick_x_pos,
                        );
                        let _ = joystick.emit(&[ev]);
                        /* match joystick.emit(&[ev]) {
                          Ok(_) => {
                            info!("Moved joystick position to {}", joystick_x_pos);
                          },
                          Err(e) => {
                            warn!("Failed to emit joystick event: {}", e);
                            continue;
                          }
                        } */
                    },
                    EventSummary::Key(_, KeyCode::BTN_LEFT, 1) => {
                        if mouse_steer_active == false {continue}
                        
                        let _ = mouse_main_emit.grab(); 
                        warn!("Potentially changed mouse focus while tracking mouse, mouse grabbed until mouse-steer-linux-J1TL deactivated!");
                        while mouse_steer_active == true {mouse_steer_active = match rx.try_recv() {Ok(bool) => bool, Err(_e) => mouse_steer_active};};
                        let _ = mouse_main_emit.ungrab(); 
                    },
                    _ => {}
                }
            }
         }
    }

fn select_input_device(filter_evtype: EventType, filter_rels: Vec<RelativeAxisCode>, filter_keys: Vec<KeyCode>, devname:&str)-> Result<(Device, String), MouseSteerError> {
    // find all input devices that can be used as a specific type of device

    let dev_input_paths: Vec<_> = fs::read_dir("/dev/input/")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.path().into_os_string().to_str().map(String::from)).collect();

    let mut devices: Vec<Device> = Vec::new();
    let mut paths:Vec<String> = Vec::new();
    for p in dev_input_paths {
        let d = Device::open(&p).ok().filter(|device| device.supported_events().contains(filter_evtype));
        match d {
            Some(d) => {

                if filter_rel != RelativeAxisCode::REL_MISC {
                    if d.supported_relative_axes().map_or(false, |axes| axes.contains(filter_rel)) == false {continue}
                }
                if d.supported_keys().map_or(false, |keys| {
                    let mut correct_keys = 0;
                    for fk in &filter_keys {if keys.contains(*fk) {correct_keys += 1}}; // borrows then dereferences, just pleasing the compiler
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
        error!("{}", MouseSteerError::NoDeviceError);
        return Err(MouseSteerError::NoDeviceError);
    }
    if devices.len() == 1 {warn!("Only one compatible {} found! ({})", devname, devices[0].name().unwrap_or("Unknown Device"));}

    // await user input to confirm device is at least able to to required functions, despite the fact that it should be able to (some devices are not real)
    if filter_rel == RelativeAxisCode::REL_MISC {
        println!("Awaiting input to choose device, please do the following inputs on your {}: press {:?}. (Does not need to happen at the same time)", devname, filter_keys);
    }
    else {
        println!("Awaiting input to choose device, please do the following inputs on your {}: move {:?}, and press {:?}. (Does not need to happen at the same time)", devname, filter_rel, filter_keys);
    }

    let (dftx, dfrx) = mpsc::channel(); // device found channel
    let dev_found = Arc::new(AtomicBool::new(false));
    for p in paths{


        let dftx_clone = dftx.clone();
        let dev_found_clone = dev_found.clone();
        let mut dev = Device::open(&p).unwrap();
        let mut req_rel = filter_rel.clone().0;
        let mut req_keys: Vec<KeyCode> = filter_keys.clone();
        thread::spawn(move || {
            let req_num = req_keys.len() as i32 + 1; // +1 is req_rel's length
            let mut req_count = 0;

            if req_rel == RelativeAxisCode::REL_MISC.0 {req_count += 1};

            let path_perm = p.clone();

            'thread_loop: loop {
                for ev in dev.fetch_events().unwrap() {
                    if dev_found_clone.load(Ordering::Acquire) == true {break 'thread_loop}


                    let path_temp = path_perm.clone();
                    let ev_code = ev.code();

                    if ev.event_type() == EventType::RELATIVE {
                        if req_rel != RelativeAxisCode::REL_MISC.0 {if ev_code == req_rel {req_count += 1; println!("{:?} moved", RelativeAxisCode(req_rel)); req_rel = RelativeAxisCode::REL_MISC.0;}}
                        
                    }
                    if ev.event_type() == EventType::KEY {
                        let mut rem_req_keys: Vec<KeyCode> = vec![];

                        for rk in &req_keys {
                            if ev_code == rk.0 {req_count += 1; println!("{:?} pressed", rk); continue};
                            rem_req_keys.push(*rk); // * dereferences
                        }
                        req_keys = rem_req_keys;
                    }

                    if req_count == req_num {let _ = dftx_clone.send(path_temp); break 'thread_loop}

                }
            }
        });

    }

    let path: String = dfrx.recv().unwrap().to_string();

    dev_found.store(true, Ordering::Release);
    for mut d in devices {
        let _ = d.send_events(&[InputEvent::new(EventType::KEY.0, filter_keys[0].0, 1)]);
        let _ = d.send_events(&[InputEvent::new(EventType::KEY.0, filter_keys[0].0, 0)]);
    }
    
    
    let input_device = Device::open(&path).unwrap();
    info!("Using \"{}\" as {} input device", input_device.name().unwrap_or("Unknown Device"), devname);
    Ok((input_device, path))

}


fn create_joystick(abs_info: AbsInfo, name: &str) -> std::io::Result<VirtualDevice> {
    let abs_x = UinputAbsSetup::new(AbsoluteAxisCode::ABS_X, abs_info);
    let abs_y = UinputAbsSetup::new(AbsoluteAxisCode::ABS_Y, abs_info);

    let mut keys = evdev::AttributeSet::new();
    for button in KEYS {
        keys.insert(button)
    }

    let joystick = VirtualDevice::builder()?
        .name(name)
        .with_absolute_axis(&abs_x)?
        .with_absolute_axis(&abs_y)?
        .with_keys(&keys)?
        .build()?;

    Ok(joystick)
}

fn mouse_move_evs(x:i32, y:i32) -> [InputEvent; 4] {
    let event_x = InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_X.0, x);
    let event_y = InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_Y.0, y);

    let event_sync = InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0);
    
    let events: [InputEvent; 4] = [event_x, event_sync, event_y, event_sync];
    return events;
}


fn load_config() -> Config {
    if Config::exists() {
      match Config::load() {
        Ok(conf) => {
          info!("Using configuration file {}", Config::path());
          conf
        }
        Err(_) => {
          warn!("Problem loading the configuration, using default");
          Config::default()
        }
      }
    } else {
      info!("No configuration found, using default");
      Config::default()
    }
}
