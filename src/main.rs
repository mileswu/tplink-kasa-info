use clap::{App, Arg};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap();
    PathBuf::from(format!("{}/.tplink.toml", home))
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Settings {
    username: String,
    password: String,
}

async fn get_token(settings: &Settings) -> String {
    println!("Fetching token");
    let request = json!({
        "method": "login",
        "params": {
            "appType": "Kasa_Android",
            "cloudUserName": settings.username,
            "cloudPassword": settings.password,
            "terminalUUID": ""
        }
    });
    let client = reqwest::Client::new();
    let response_text = client
        .post("https://wap.tplinkcloud.com")
        .header("Content-Type", "application/json")
        .body(request.to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_str(&response_text).unwrap();
    let error_code = response["error_code"].as_i64().unwrap();
    if error_code != 0 {
        panic!("Got error when logging in (response = {})", response_text);
    }
    let result = response["result"].as_object().unwrap();
    let token = result["token"].as_str().unwrap();
    return String::from(token);
}

async fn print_device_list(token: &str) {
    let request = json!({
        "method": "getDeviceList"
    });
    let client = reqwest::Client::new();
    let response_text = client
        .post(&format!(
            "{}{}",
            "https://wap.tplinkcloud.com/?token=", token
        ))
        .header("Content-Type", "application/json")
        .body(request.to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_str(&response_text).unwrap();
    let error_code = response["error_code"].as_i64().unwrap();
    if error_code != 0 {
        panic!(
            "Got error when getting device list (response = {})",
            response_text
        );
    }
    let result = response["result"].as_object().unwrap();
    let device_list = result["deviceList"].as_array().unwrap();
    for i in device_list.iter() {
        let alias = i["alias"].as_str().unwrap();
        let device_id = i["deviceId"].as_str().unwrap();
        println!("{} = {}", alias, device_id);
    }
}

fn prompt(text: &str) -> String {
    print!("{}: ", text);
    io::stdout().flush().unwrap();
    let mut value = String::new();
    io::stdin().read_line(&mut value).unwrap();
    return value.trim().to_string();
}

fn setup(username: Option<&str>, password: Option<&str>, overwrite: bool) {
    let config_path = config_path();
    if overwrite == false && config_path.exists() {
        panic!(
            "A config already exists at {}. Please remove it if first before running setup again",
            config_path.display()
        );
    }
    let username = username
        .map(|i| i.to_string())
        .unwrap_or_else(|| prompt("Enter your tp-link kasa username"));
    let password = password
        .map(|i| i.to_string())
        .unwrap_or_else(|| prompt("Enter your tp-link kasa password"));
    let settings = Settings { username, password };
    let toml = toml::to_string(&settings).unwrap();
    fs::write(&config_path, &toml).unwrap();
}

fn get_settings(matches: &clap::ArgMatches, args: Vec<&str>) -> Settings {
    let mut config = config::Config::new();
    let config_path = config_path();
    if config_path.exists() {
        config.merge(config::File::from(config_path)).unwrap();
    }
    for arg in &args {
        match matches.value_of(arg) {
            Some(v) => {
                config.set(arg, v).unwrap();
            }
            None => (),
        };
    }
    let settings: Settings = config.try_into().unwrap();
    return settings;
}

#[tokio::main]
async fn main() {
    let common_args = [
        Arg::with_name("username")
            .short("u")
            .value_name("USERNAME")
            .help("Tp-link kasa username")
            .takes_value(true),
        Arg::with_name("password")
            .short("p")
            .value_name("PASSWORD")
            .help("Tp-link kasa password")
            .takes_value(true),
    ];
    let common_arg_names = vec!["username", "password"];
    let matches = App::new("Query TP-Link Kasa")
        .subcommand(
            App::new("list")
                .about("List TP-Link devices registered to your account")
                .args(&common_args),
        )
        .subcommand(
            App::new("setup")
                .about("Stores username and password in a settings file")
                .args(&common_args)
                .arg(
                    Arg::with_name("overwrite")
                        .short("o")
                        .help("Overwrite settings file if it exists (default: false)"),
                ),
        )
        .setting(clap::AppSettings::ArgRequiredElseHelp)
        .get_matches();
    match matches.subcommand() {
        ("list", Some(submatches)) => {
            let settings = get_settings(&submatches, common_arg_names);
            let token = get_token(&settings).await;
            print_device_list(token.as_str()).await;
        }
        ("setup", Some(submatches)) => {
            let username = submatches.value_of("username");
            let password = submatches.value_of("password");
            let overwrite = submatches.is_present("overwrite");
            setup(username, password, overwrite)
        }
        _ => panic!("Unreachable branch due to clap::AppSettings::ArgRequiredElseHelp"),
    }
    return;
}
