use evdev::{
    AbsInfo, AbsoluteAxisCode, EventSummary, Device, EventType, InputEvent, KeyCode, RelativeAxisCode, UinputAbsSetup, uinput::VirtualDevice
};

use thiserror::Error;
use log::{info, warn, error, LevelFilter};
use env_logger::Builder;

use std::fs;

use std::sync::mpsc;
use std::thread;

use std::time::Duration;

use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};

mod configuration;
use configuration::Config;

//mod window;
//use window::create_window;

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

    #[error("Config value out of range")]
    ConfigValueOutOfRange,
}



fn main() -> Result<(), MouseSteerError> {

    // initialize logger
    Builder::new()
        .filter_level(LevelFilter::Trace)  // This shows everything
        .init();

    let conf = match Config::load() {Ok(config) => {config}, Err(error) => {return Err(error)}};
    info!("sensitivity: {}", conf.steer_sensitivity);
    

    // select mouse device
    let mouse_and_path = select_input_device(
        EventType::RELATIVE, 
        vec![RelativeAxisCode::REL_X, RelativeAxisCode::REL_WHEEL], 
        vec![KeyCode::BTN_LEFT, KeyCode::BTN_RIGHT], 
        "mouse").unwrap();
    
    let mut mouse = mouse_and_path.0;

    //select keyboard device
    let mut keyboard = select_input_device(
        EventType::KEY, 
        vec![RelativeAxisCode::REL_MISC], 
        vec![KeyCode::KEY_F5, KeyCode::KEY_F7, KeyCode::KEY_F8], 
        "keyboard").unwrap().0;
    

    // set up virtual joystick
    let abs_info_x = AbsInfo::new(
        0, 
        conf.raw_range_min(), 
        conf.raw_range_max(), 
        10, 
        0, 
        conf.raw_resolution());
    
    let abs_info_y = AbsInfo::new(
        0, 
        conf.raw_range_min(), 
        conf.raw_range_max(), 
        10, 
        0, 
        conf.raw_resolution());
    
    let mut joystick = create_joystick(abs_info_x, abs_info_y, VJOYSTICK_NAME).unwrap();
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

                    let _ = thr_mouse.grab();
                    let _ = thr_mouse.ungrab(); 
                    //let _ = thr_mouse.send_events(&mouse_move_evs(i32::MIN, i32::MIN));
                    //let _ = thr_mouse.send_events(&mouse_move_evs(i32::MIN, i32::MIN));
                    let _ = thr_mouse.grab();
                    let _ = thr_mouse.ungrab(); 
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.send_events(&mouse_move_evs(1920/2, 1080/2));
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.send_events(&mouse_move_evs(0, 0));
                    let _ = thr_mouse.grab();
                    let _ = thr_mouse.ungrab();

                    thread::sleep(Duration::from_millis(100));

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
    //thread::spawn(move || { create_window()});

    // fetch events and send them through to virtual joystick
    let steer_deadzone: i32 = conf.steer_deadzone_calculated();
    let brake_deadzone: i32 = conf.brake_deadzone_calculated();

    let steer_min: i32 = conf.raw_range_min();
    let steer_max: i32 = conf.raw_range_max();
    let brake_min: i32 = conf.raw_range_min();
    //let brake_max: i32 = conf.calculated_half_brake_range();

    let steer_multiplier: i32 = conf.steer_multiplier();
    let brake_multiplier: i32 = conf.brake_multiplier();

    let mut joystick_x_pos: i32 = 0;
    let mut joystick_y_pos: i32 = 0;

    let mut mouse_steer_active: bool = false;
    let mut braking_active: bool = false;

    loop {
        if mouse_steer_active == true {let _ = mouse.grab();}
        if mouse_steer_active == false {let _ = mouse.ungrab();}

        for ev in mouse.fetch_events().unwrap(){
                match ev.destructure() {
                    EventSummary::RelativeAxis(_, RelativeAxisCode::REL_X, _) => {
                        mouse_steer_active = match rx.try_recv() {Ok(bool) => bool, Err(_e) => mouse_steer_active};
                        if mouse_steer_active == false {joystick_x_pos = 0; continue}

                        joystick_x_pos += ev.value() * steer_multiplier;


                        if joystick_x_pos < steer_min{
                            joystick_x_pos = steer_min
                        }
                        if joystick_x_pos > steer_max{
                            joystick_x_pos = steer_max
                        }

                        if joystick_x_pos < steer_deadzone && joystick_x_pos > -steer_deadzone {continue}


                        let ev = InputEvent::new(
                            EventType::ABSOLUTE.0,
                            AbsoluteAxisCode::ABS_X.0,
                            joystick_x_pos,
                        );
                        let _ = joystick.emit(&[ev]);
                    },
                    EventSummary::RelativeAxis(_, RelativeAxisCode::REL_WHEEL, _) => {
                        mouse_steer_active = match rx.try_recv() {Ok(bool) => bool, Err(_e) => mouse_steer_active};
                        if mouse_steer_active == false {joystick_y_pos = 0; continue}
                        if braking_active == false {joystick_y_pos = 0; continue}
                        
                        joystick_y_pos += ev.value() * brake_multiplier;

                        if joystick_y_pos < brake_min {
                            joystick_y_pos = brake_min
                        }
                        if joystick_y_pos > 0 {
                            joystick_y_pos = 0
                        }

                        if joystick_y_pos < brake_deadzone && joystick_y_pos > -brake_deadzone {continue}


                        let emitted_ev = InputEvent::new(
                            EventType::ABSOLUTE.0,
                            AbsoluteAxisCode::ABS_RY.0,
                            joystick_y_pos,
                        );
                        let _ = joystick.emit(&[emitted_ev]);
                    },
                    EventSummary::Key(_, KeyCode::BTN_RIGHT, _) => {
                        if mouse_steer_active == false {continue}
                        
                        if ev.value() == 0 {

                            braking_active = false;

                            let emitted_ev = InputEvent::new(
                                EventType::ABSOLUTE.0,
                                AbsoluteAxisCode::ABS_RY.0,
                                0,
                            );
                            let _ = joystick.emit(&[emitted_ev]);
                        }

                        else if ev.value() > 0 {
                            if braking_active == false {
                                
                                braking_active = true;

                                joystick_y_pos = (brake_min*80)/100;

                                let emitted_ev = InputEvent::new(
                                    EventType::ABSOLUTE.0,
                                    AbsoluteAxisCode::ABS_RY.0,
                                    joystick_y_pos,
                                );
                            let _ = joystick.emit(&[emitted_ev]);
                            }
                        }
                    },
                    EventSummary::Key(_, KeyCode::BTN_LEFT, _) => {
                        if mouse_steer_active == false {continue}

                        let mut emitted_ev = InputEvent::new(
                            EventType::ABSOLUTE.0,
                            AbsoluteAxisCode::ABS_Z.0,
                            100,
                        );

                        if ev.value() == 0 {
                            emitted_ev = InputEvent::new(EventType::ABSOLUTE.0,
                            AbsoluteAxisCode::ABS_Z.0,
                            0
                            );
                        }

                        let _ = joystick.emit(&[emitted_ev]);
                    }

                    _ => {}
                }
            }
         }
    }

fn create_joystick(abs_info_x: AbsInfo, abs_info_y: AbsInfo, name: &str) -> std::io::Result<VirtualDevice> {
    let abs_steer = UinputAbsSetup::new(AbsoluteAxisCode::ABS_X, abs_info_x);
    let abs_brake = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RY, abs_info_y);

    let abs_accelerator = UinputAbsSetup::new(AbsoluteAxisCode::ABS_Z, AbsInfo::new(0, 0, 100, 0, 0, 100));

    let dummy_1 = UinputAbsSetup::new(AbsoluteAxisCode::ABS_Y, abs_info_x);
    let dummy_2 = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RX, abs_info_y);
    let dummy_3 = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RZ, AbsInfo::new(0, 0, 1000, 0, 0, 1000));

    let mut keys = evdev::AttributeSet::new();
    for button in KEYS {
        keys.insert(button)
    }

    let joystick = VirtualDevice::builder()?
        .name(name)
        .with_absolute_axis(&abs_steer)?
        .with_absolute_axis(&abs_brake)?
        .with_absolute_axis(&abs_accelerator)?
        .with_absolute_axis(&dummy_1)?
        .with_absolute_axis(&dummy_2)?
        .with_absolute_axis(&dummy_3)?
        .with_keys(&keys)?
        .build()?;

    Ok(joystick)
}

fn mouse_move_evs(x:i32, y:i32) -> [InputEvent; 2] {
    let event_x = InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_X.0, x);
    let event_y = InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_Y.0, y);

    let events: [InputEvent; 2] = [event_x, event_y];
    return events;
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

                if d.supported_relative_axes().map_or(false, |keys| {
                    let mut correct_rels = 0;

                    for fr in &filter_rels {if keys.contains(*fr) {correct_rels += 1}} // borrows then dereferences, just pleasing the compiler
                    
                    if correct_rels == filter_rels.len() {true}
                    else {false}
                    }
                ) == false {if filter_rels[0] == RelativeAxisCode::REL_MISC {} else {continue;}}
                
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
    if filter_rels[0] == RelativeAxisCode::REL_MISC {
        println!("Awaiting input to choose device, please do the following inputs on your {}: press {:?}. (Does not need to happen at the same time)", devname, filter_keys);
    }
    else {
        println!("Awaiting input to choose device, please do the following inputs on your {}: move {:?}, and press {:?}. (Does not need to happen at the same time)", devname, filter_rels, filter_keys);
    }

    let (dftx, dfrx) = mpsc::channel(); // device found channel
    let dev_found = Arc::new(AtomicBool::new(false));
    for p in paths{


        let dftx_clone = dftx.clone();
        let dev_found_clone = dev_found.clone();
        let mut dev = Device::open(&p).unwrap();
        let mut req_rels: Vec<RelativeAxisCode> = filter_rels.clone();
        let mut req_keys: Vec<KeyCode> = filter_keys.clone();
        thread::spawn(move || {
            let req_num = req_keys.len() as i32 + req_rels.len() as i32;
            let mut req_count = 0;

            if req_rels[0] == RelativeAxisCode::REL_MISC {req_count += 1}

            let path_perm = p.clone();

            'thread_loop: loop {
                for ev in dev.fetch_events().unwrap() {
                    if dev_found_clone.load(Ordering::Acquire) == true {break 'thread_loop}


                    let path_temp = path_perm.clone();
                    let ev_code = ev.code();

                    if ev.event_type() == EventType::RELATIVE {
                        
                        let mut rem_req_rels: Vec<RelativeAxisCode> = vec![];

                        for rr in &req_rels {
                            if ev_code == rr.0 {req_count += 1; println!("{:?} moved", rr);  continue};
                            rem_req_rels.push(*rr); // * dereferences
                        }

                        req_rels = rem_req_rels;
                        
                    }
                    if ev.event_type() == EventType::KEY {
                        let mut rem_req_keys: Vec<KeyCode> = vec![];

                        for rk in &req_keys {
                            if ev_code == rk.0 {req_count += 1; println!("{:?} pressed", rk);  continue};
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



