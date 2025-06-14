use std::{
    env::{self, VarError},
    fs::File,
    io::{ErrorKind, Read},
};

use regex::Regex;

const CONFIG_PATH: &str = "wlbreaktime/config";

const DEFAULT_BREAK_DURATION_SECONDS: u64 = 80;
const DEFAULT_BREAK_INTERVAL_SECONDS: u64 = 1800;
const DEFAULT_SHOW_POPUP: bool = true;
const DEFAULT_PLAY_SOUND: bool = true;
const DEFAULT_SHOW_NOTIFICATION: bool = true;

#[derive(Debug)]
pub struct Config {
    pub break_interval: u64,
    pub break_duration: u64,
    pub show_popup: bool,
    pub play_sound: bool,
    pub show_notification: bool,
}

fn read_configuration(
    config: &mut Config,
    mut loaded_config: File,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut content = String::new();
    loaded_config.read_to_string(&mut content)?;
    println!("Read this content from config: '{content}'");

    let re = Regex::new(r"break_interval=(\d+)(s|m)?").unwrap();
    if let Some(c) = re.captures(&content) {
        let mut num = c.get(1).unwrap().as_str().parse::<u64>()?;
        if c.get(2).is_some_and(|m| m.as_str() == "m") {
            num = num * 60;
        }
        config.break_interval = num;
    }

    let re = Regex::new(r"break_duration=(\d+)(s|m)?").unwrap();
    if let Some(c) = re.captures(&content) {
        let mut num = c.get(1).unwrap().as_str().parse::<u64>()?;
        if c.get(2).is_some_and(|m| m.as_str() == "m") {
            num = num * 60;
        }
        config.break_duration = num;
    }

    let re = Regex::new(r"show_popup=(true|false)").unwrap();
    if let Some(c) = re.captures(&content) {
        let value = c.get(1).unwrap().as_str() == "true";
        config.show_popup = value;
    };

    let re = Regex::new(r"play_sound=(true|false)").unwrap();
    if let Some(c) = re.captures(&content) {
        let value = c.get(1).unwrap().as_str() == "true";
        config.play_sound = value;
    };

    let re = Regex::new(r"show_notification=(true|false)").unwrap();
    if let Some(c) = re.captures(&content) {
        let value = c.get(1).unwrap().as_str() == "true";
        config.show_notification = value;
    };

    Ok(())
}

pub fn load_configuration() -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config {
        break_interval: DEFAULT_BREAK_INTERVAL_SECONDS,
        break_duration: DEFAULT_BREAK_DURATION_SECONDS,
        show_popup: DEFAULT_SHOW_POPUP,
        play_sound: DEFAULT_PLAY_SOUND,
        show_notification: DEFAULT_SHOW_NOTIFICATION,
    };

    match File::open("/etc/".to_string() + CONFIG_PATH) {
        Ok(c) => read_configuration(&mut config, c)?,
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        // do nothing, just means that there is nothing configured on system level
        Err(_) => panic!("Other error!"),
    };

    let config_home = match env::var("XDG_CONFIG_HOME") {
        Ok(path) => path,
        Err(err) if err == VarError::NotPresent => {
            let home = env::var("HOME")?;
            home + "/.config"
        }
        Err(err) => {
            panic!("Error '{err}' occured while trying to read XDG_CONFIG_HOME!");
        }
    };

    match File::open(config_home + "/" + CONFIG_PATH) {
        Ok(c) => read_configuration(&mut config, c)?,
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        // do nothing, just means that there is nothing configured on user level
        Err(_) => panic!("Other error!"),
    };

    Ok(config)
}
