//! 账户管理：离线账户 / authlib-injector 外置登录 / 正版接口模板
//!
//! 账户数据存放在启动器自身 config.json（与游戏目录解耦），
//! access token 只落盘，不通过任何 command 返回给前端。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::models::{AccountInfo, CreateOfflineAccountRequest, ExternalLoginRequest};
use crate::state::{self, AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountRecord {
    id: String,
    kind: String,
    username: String,
    uuid: String,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    api_base: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
}

fn read_records() -> Vec<AccountRecord> {
    let (values, _) = state::load_accounts_config();
    values
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

fn write_records(records: &[AccountRecord], active: Option<&str>) -> Result<(), String> {
    let values = records
        .iter()
        .map(|record| serde_json::to_value(record).map_err(|e| e.to_string()))
        .collect::<Result<Vec<Value>, String>>()?;
    state::save_accounts_config(&values, active)
}

fn resolved_active(records: &[AccountRecord], configured: Option<String>) -> Option<String> {
    if let Some(id) = configured {
        if records.iter().any(|record| record.id == id) {
            return Some(id);
        }
    }
    records.first().map(|record| record.id.clone())
}

fn to_info(record: &AccountRecord, active: &Option<String>) -> AccountInfo {
    AccountInfo {
        id: record.id.clone(),
        kind: record.kind.clone(),
        username: record.username.clone(),
        uuid: record.uuid.clone(),
        avatar_url: record.avatar_url.clone(),
        api_base: record.api_base.clone(),
        is_active: active.as_deref() == Some(record.id.as_str()),
    }
}

fn list_infos(records: &[AccountRecord], active: &Option<String>) -> Vec<AccountInfo> {
    records.iter().map(|record| to_info(record, active)).collect()
}

/// 切换账户时同步游戏 settings.json 的 username，启动器与游戏保持一致
fn sync_game_username(app: &AppHandle, records: &[AccountRecord], active: &Option<String>) {
    let Some(id) = active else {
        return;
    };
    let Some(account) = records.iter().find(|record| record.id == *id) else {
        return;
    };
    let path = app.state::<AppState>().game_root.join("settings.json");
    let mut settings: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .filter(|value: &Value| value.is_object())
        .unwrap_or_else(|| json!({}));
    settings["username"] = json!(account.username);
    if let Ok(text) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(path, text);
    }
}

fn external_avatar_url(api_base: &str, uuid: &str) -> String {
    let base = api_base.trim().trim_end_matches('/');
    let root = base
        .strip_suffix("/api/yggdrasil")
        .unwrap_or(base)
        .trim_end_matches('/');
    format!("{}/avatar/{}?size=96", root, uuid)
}

/// 列出全部账户
#[tauri::command]
pub async fn list_accounts() -> Result<Vec<AccountInfo>, String> {
    let (_, configured) = state::load_accounts_config();
    let records = read_records();
    let active = resolved_active(&records, configured);
    Ok(list_infos(&records, &active))
}

/// 当前账户（无账户时返回 None）
#[tauri::command]
pub async fn get_active_account() -> Result<Option<AccountInfo>, String> {
    let (_, configured) = state::load_accounts_config();
    let records = read_records();
    let active = resolved_active(&records, configured);
    Ok(active
        .as_ref()
        .and_then(|id| records.iter().find(|record| record.id == *id))
        .map(|record| to_info(record, &active)))
}

/// 新建 / 编辑离线账户（传 id 表示编辑）
#[tauri::command]
pub async fn upsert_offline_account(
    app: AppHandle,
    req: CreateOfflineAccountRequest,
) -> Result<AccountInfo, String> {
    let username = req.username.trim();
    if username.is_empty() {
        return Err("用户名不能为空".to_string());
    }
    let (_, configured) = state::load_accounts_config();
    let mut records = read_records();
    let mut active = resolved_active(&records, configured);

    let id = req
        .id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            format!("off-{}-{}", stamp, records.len())
        });

    let uuid = format!("offline-{}", username.to_lowercase().replace(' ', "-"));
    let avatar_url = req.avatar_url.map(|url| url.trim().to_string()).filter(|s| !s.is_empty());

    if let Some(existing) = records.iter_mut().find(|record| record.id == id) {
        existing.username = username.to_string();
        existing.uuid = uuid;
        if let Some(url) = avatar_url {
            existing.avatar_url = Some(url);
        }
    } else {
        records.push(AccountRecord {
            id: id.clone(),
            kind: "offline".to_string(),
            username: username.to_string(),
            uuid,
            avatar_url,
            api_base: None,
            access_token: None,
        });
        if active.is_none() {
            active = Some(id.clone());
        }
    }

    write_records(&records, active.as_deref())?;
    sync_game_username(&app, &records, &active);
    Ok(records
        .iter()
        .find(|record| record.id == id)
        .map(|record| to_info(record, &active))
        .ok_or_else(|| "账户保存失败".to_string())?)
}

/// 删除账户；删除当前账户时自动切到第一个剩余账户
#[tauri::command]
pub async fn delete_account(app: AppHandle, id: String) -> Result<Vec<AccountInfo>, String> {
    let (_, configured) = state::load_accounts_config();
    let mut records = read_records();
    let mut active = resolved_active(&records, configured);
    records.retain(|record| record.id != id);
    if active.as_deref() == Some(id.as_str()) {
        active = records.first().map(|record| record.id.clone());
    }
    write_records(&records, active.as_deref())?;
    sync_game_username(&app, &records, &active);
    Ok(list_infos(&records, &active))
}

/// 切换当前账户
#[tauri::command]
pub async fn set_active_account(
    app: AppHandle,
    id: String,
) -> Result<Vec<AccountInfo>, String> {
    let records = read_records();
    if !records.iter().any(|record| record.id == id) {
        return Err("账户不存在".to_string());
    }
    let active = Some(id);
    write_records(&records, active.as_deref())?;
    sync_game_username(&app, &records, &active);
    Ok(list_infos(&records, &active))
}

/// authlib-injector 外置登录（直接实现）
#[tauri::command]
pub async fn authenticate_external(
    app: AppHandle,
    req: ExternalLoginRequest,
) -> Result<AccountInfo, String> {
    let base = req.api_base.trim().trim_end_matches('/');
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err("认证服务器地址必须以 http(s):// 开头".to_string());
    }
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err("请输入外置登录账号与密码".to_string());
    }

    let app_state = app.state::<AppState>();
    let client = &app_state.http_client;
    let response = client
        .post(format!("{}/authserver/authenticate", base))
        .json(&json!({
            "agent": { "name": "Minecraft", "version": 1 },
            "username": req.username.trim(),
            "password": req.password,
            "requestUser": true
        }))
        .send()
        .await
        .map_err(|e| format!("连接认证服务器失败：{}", e))?;

    let status = response.status();
    let data: Value = response
        .json()
        .await
        .map_err(|e| format!("认证服务器返回解析失败：{}", e))?;
    if !status.is_success() {
        let message = data
            .get("errorMessage")
            .or_else(|| data.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("账号或密码错误");
        return Err(format!("外置登录失败：{}", message));
    }

    let token = data
        .get("accessToken")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "认证服务器未返回 accessToken".to_string())?;
    let profile = data
        .get("selectedProfile")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "认证服务器未返回角色信息".to_string())?;
    let uuid = profile
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "角色缺少 UUID".to_string())?;
    let username = profile
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(req.username.trim());

    let mut records = read_records();
    let id = format!("ext-{}", uuid);
    let record = AccountRecord {
        id: id.clone(),
        kind: "external".to_string(),
        username: username.to_string(),
        uuid: uuid.to_string(),
        avatar_url: Some(external_avatar_url(base, uuid)),
        api_base: Some(base.to_string()),
        access_token: Some(token.to_string()),
    };
    if let Some(existing) = records.iter_mut().find(|record| record.id == id) {
        *existing = record;
    } else {
        records.push(record);
    }
    let active = Some(id.clone());
    write_records(&records, active.as_deref())?;
    sync_game_username(&app, &records, &active);

    Ok(records
        .iter()
        .find(|record| record.id == id)
        .map(|record| to_info(record, &active))
        .ok_or_else(|| "账户保存失败".to_string())?)
}
