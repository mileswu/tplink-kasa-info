use serde_json::json;

async fn get_token() -> String {
    let request = json!({
        "method": "login",
        "params": {
            "appType": "Kasa_Android",
            "cloudUserName": "",
            "cloudPassword": "",
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

#[tokio::main]
async fn main() {
    let token = get_token().await;
    print_device_list(token.as_str()).await;
    return;
}
