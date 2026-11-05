use evdev::{
    AbsInfo, AbsoluteAxisCode, EventSummary, Device, EventType, InputEvent, KeyCode, RelativeAxisCode, UinputAbsSetup, uinput::VirtualDevice
};

use thiserror::Error;
use log::{info, warn, error, LevelFilter};
use env_logger::Builder;

use std::sync::mpsc;
use std::thread;

use std::time::Duration;

mod configuration;
use configuration::Config;

mod evdev_device_picker;

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
    let mouse_and_path = evdev_device_picker::select_input_device(
        EventType::RELATIVE, 
        vec![RelativeAxisCode::REL_X, RelativeAxisCode::REL_WHEEL], 
        vec![KeyCode::BTN_LEFT, KeyCode::BTN_RIGHT], 
        "mouse").unwrap();
    
    let mut mouse = mouse_and_path.0;

    //select keyboard device
    let mut keyboard = evdev_device_picker::select_input_device(
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