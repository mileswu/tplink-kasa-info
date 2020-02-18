use clap::{App, Arg};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::future::Future;
use std::io::{self, Write};
use std::path::PathBuf;

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap();
    PathBuf::from(format!("{}/.tplink.toml", home))
}

const BASE_URL: &str = "https://wap.tplinkcloud.com/";

#[derive(Deserialize, Serialize, Debug)]
pub struct Settings {
    username: String,
    password: String,
    token: String,
}

enum LoginDetails {
    Settings(Settings),
    UsernameAndPassword(String, String),
}

fn write_settings(username: &str, password: &str, token: &str) {
    let settings = Settings {
        username: username.to_owned(),
        password: password.to_owned(),
        token: token.to_owned(),
    };
    let toml = toml::to_string(&settings).unwrap();
    let config_path = config_path();
    fs::write(&config_path, &toml).unwrap();
}

async fn get_new_token(login_details: &LoginDetails) -> String {
    println!("Fetching new token");
    let (username, password) = match login_details {
        LoginDetails::Settings(s) => (&s.username, &s.password),
        LoginDetails::UsernameAndPassword(u, p) => (u, p),
    };
    let request = json!({
        "method": "login",
        "params": {
            "appType": "Kasa_Android",
            "cloudUserName": username,
            "cloudPassword": password,
            "terminalUUID": ""
        }
    });
    let client = reqwest::Client::new();
    let response_text = client
        .post(BASE_URL)
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
    if let LoginDetails::Settings(_) = login_details {
        write_settings(username, password, token);
    };
    return String::from(token);
}

async fn setup(overwrite: bool) {
    let config_path = config_path();
    if overwrite == false && config_path.exists() {
        panic!(
            "A config already exists at {}. Please remove it if first before running setup again",
            config_path.display()
        );
    }
    fn prompt(text: &str) -> String {
        print!("{}: ", text);
        io::stdout().flush().unwrap();
        let mut value = String::new();
        io::stdin().read_line(&mut value).unwrap();
        return value.trim().to_string();
    }
    let username = prompt("Enter your tp-link kasa username");
    let password = prompt("Enter your tp-link kasa password");
    let token = get_new_token(&LoginDetails::UsernameAndPassword(
        username.clone(),
        password.clone(),
    ))
    .await;
    let settings = Settings {
        username,
        password,
        token,
    };
    let toml = toml::to_string(&settings).unwrap();
    fs::write(&config_path, &toml).unwrap();
}

async fn runner(
    request: serde_json::value::Value,
    arg_matches: &clap::ArgMatches<'_>,
) -> serde_json::value::Value {
    let login_details = match (
        arg_matches.value_of("username"),
        arg_matches.value_of("password"),
    ) {
        (Some(_), None) | (None, Some(_)) => {
            panic!("You must pass both a username and password, or neither");
        }
        (Some(u), Some(p)) => LoginDetails::UsernameAndPassword(String::from(u), String::from(p)),
        (None, None) => {
            let config_path = config_path();
            if config_path.exists() {
                let settings: Settings =
                    toml::from_slice(&fs::read(&config_path).unwrap()).unwrap();
                LoginDetails::Settings(settings)
            } else {
                panic!("Config does not exist at {}. Either run the setup command, or pass a username and password via command-line flags", config_path.to_str().unwrap());
            }
        }
    };
    enum ApiResult {
        Success(serde_json::value::Value),
        Error(String),
        TokenExpired,
    }
    async fn go(request: serde_json::value::Value, token: String) -> ApiResult {
        let client = reqwest::Client::new();
        let response_text = client
            .post(&format!("{}/?token={}", BASE_URL, token))
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
        if error_code == 0 {
            let result = response["result"].to_owned();
            return ApiResult::Success(result);
        } else if error_code == -20651 {
            return ApiResult::TokenExpired;
        } else {
            return ApiResult::Error(response_text);
        }
    };
    async fn fetch_token_and_go<T: Future<Output = ApiResult>>(
        request: serde_json::value::Value,
        login_details: &LoginDetails,
        go: fn(serde_json::value::Value, String) -> T,
    ) -> serde_json::value::Value {
        let token = get_new_token(login_details).await;
        match go(request, token).await {
            ApiResult::Success(r) => r,
            ApiResult::TokenExpired => panic!("Token is supposedly expired but we just got it"),
            ApiResult::Error(e) => panic!(e),
        }
    };
    match login_details {
        LoginDetails::Settings(ref s) => {
            let request_clone = request.clone();
            match go(request_clone, s.token.clone()).await {
                ApiResult::Success(r) => r,
                ApiResult::TokenExpired => fetch_token_and_go(request, &login_details, go).await,
                ApiResult::Error(e) => panic!(e),
            }
        }
        LoginDetails::UsernameAndPassword(_, _) => {
            fetch_token_and_go(request, &login_details, go).await
        }
    }
}

async fn print_device_list(arg_matches: &clap::ArgMatches<'_>) {
    let request = json!({ "method": "getDeviceList" });
    let result_value = runner(request, arg_matches).await;
    let result = result_value.as_object().unwrap();
    let device_list = result["deviceList"].as_array().unwrap();
    for i in device_list.iter() {
        let alias = i["alias"].as_str().unwrap();
        let device_id = i["deviceId"].as_str().unwrap();
        println!("{} = {}", alias, device_id);
    }
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
    let matches = App::new("Query TP-Link Kasa")
        .subcommand(
            App::new("list")
                .about("List TP-Link devices registered to your account")
                .args(&common_args),
        )
        .subcommand(
            App::new("setup")
                .about("Stores username and password in a settings file")
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
            print_device_list(submatches).await;
        }
        ("setup", Some(submatches)) => {
            let overwrite = submatches.is_present("overwrite");
            setup(overwrite).await;
        }
        _ => panic!("Unreachable branch due to clap::AppSettings::ArgRequiredElseHelp"),
    }
    return;
}
