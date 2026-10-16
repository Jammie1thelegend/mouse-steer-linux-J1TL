use std::fs;
use serde::Deserialize;

use log::{error};

use crate::MouseSteerError;



#[derive(Deserialize)]
#[derive(Debug)]
pub struct Config {
    pub resolution: i32,
    pub steer_sensitivity: f32,
    pub brake_sensitivity: f32,
    pub steer_deadzone: f32,
    pub brake_deadzone: f32,
}

impl Config {
    pub fn load_default() -> Self {
    Self {
        resolution: 1024,
        steer_sensitivity: 1.0,
        brake_sensitivity: 3.0,
        steer_deadzone: 1.0,
        brake_deadzone: 0.0,
        }
    }
    pub fn load() -> Result<Self, MouseSteerError> {
        let path = dirs::config_dir()
            .unwrap_or_default()
            .join("mouse-steer-linux-J1TL")
            .join("config.toml")
            .to_string_lossy()
            .into_owned();

        let default_config =   
"resolution = 1024            # Base width of steering and braking range in pixels (Advanced users only)
steer_sensitivity = 1.0       # Steering sensitivity (Bigger = more sensitivity)
brake_sensitivity = 3.0       # Brake sensitivity    (Bigger = more sensitivity)
steer_deadzone = 1.0         # Steering deadzone
brake_deadzone = 0.0         # Braking deadzone";

        match fs::File::open(&path) {
            Ok(file) => {file}, 
            Err(_) => {fs::write(&path, default_config).unwrap(); fs::File::open(&path).unwrap()}
        };

        let config_raw = match fs::read_to_string(&path) {
            Ok(raw) => {raw}, 
            Err(_) => {fs::write(&path, default_config).unwrap(); fs::read_to_string(&path).unwrap()}
        };

    // add error handling
        let config: Self = match toml::from_str(&config_raw) {
            Ok(parsed) => {parsed},
            Err(_) => {Config::load_default()}
        };

        if config.steer_deadzone > 100.0 || config.steer_deadzone < 0.0 {
            error!("steering deadzone ({}) too big/ too small!", config.steer_deadzone);
            return Err(MouseSteerError::ConfigValueOutOfRange)
        }
        if config.brake_deadzone > 100.0 || config.brake_deadzone < 0.0 {
            error!("brake deadzone ({}) too big/ too small!", config.brake_deadzone);
            return Err(MouseSteerError::ConfigValueOutOfRange)
        }


        return Ok(config)
    }

    pub fn raw_resolution(&self) -> i32 {
        i16::MAX as i32 * 2
    }
    pub fn raw_range_max(&self) -> i32 {
        self.raw_resolution()/2
    }
    pub fn raw_range_min(&self) -> i32 {
        -self.raw_resolution()/2
    }

    pub fn config_half_steer_range(&self) -> i32 {
        ((self.resolution as f32/self.steer_sensitivity)/2.0) as i32
    }
    pub fn config_half_brake_range(&self) -> i32 {
        ((self.resolution as f32/self.brake_sensitivity)/40.0) as i32
    }
    pub fn steer_multiplier(&self) -> i32 {
        self.raw_range_max() / self.config_half_steer_range()
    }
    pub fn brake_multiplier(&self) -> i32 {
        self.raw_range_max() / self.config_half_brake_range()
    }
/*     pub fn calculated_half_steer_range(&self) -> i32 {
        self.steer_multiplier() * (self.config_half_steer_range() / self.steer_multiplier())
    }
    pub fn calculated_half_brake_range(&self) -> i32 {
        self.brake_multiplier() * (self.config_half_brake_range() / self.brake_multiplier())
    } */
    pub fn steer_deadzone_calculated(&self) -> i32 {
        (self.steer_deadzone/ 100.0 * (self.raw_range_max() as f32)) as i32
    }
    pub fn brake_deadzone_calculated(&self) -> i32 {
        (self.brake_deadzone/ 100.0 * (self.raw_range_max() as f32)) as i32
    }

}