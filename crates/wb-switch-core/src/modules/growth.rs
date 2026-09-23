//! 成长中心：任务列表 / 接受 / 事件上报 / 领奖 / 抽奖 / 盲盒。
//!
//! 对照 WorkBuddy-Daily `workbuddy_daily.py` 中的 `/v2/activity/growth/*` 与
//! `/v2/report` 接口。复用 switch 既有基建：
//! - [`account::build_auth_headers`] 构造鉴权头（与桌面端一致）；
//! - [`config::http_request`] 统一 HTTP；
//! - 401/403 时用 [`refresh::refresh_account_token`] 刷新一次并重试（与
//!   `checkin` / `travel` 同一套语义）。
//!
//! 成长中心仅国内版开放（与 `travel` / `checkin` 一致），国际版账号在任何请求前
//! 直接短路，绝不发请求。
//!
//! 第一期范围：任务列表、批量接受、单/批量领奖、通用事件上报、成长档案
//! （profile/energy/streak）、大转盘抽奖、盲盒开启，以及单账号一键闭环。
//! 真实 AI 对话类任务（chat 5 次、夜猫子，走 SSE）与开学季/小程序任务留待二期。

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::{json, Value};
use sha1::{Digest, Sha1};

use crate::modules::account::{
    account_display_name, build_auth_headers, load_accounts, variant_of,
};
use crate::modules::config::{http_request, now_ms, RunFlagGuard, WORKBUDDY_API_ENDPOINT};
use crate::modules::refresh::refresh_account_token;
use crate::modules::variant::WbVariant;

/// 成长中心请求并发锁：防止与手动/上一轮重复运行。
static GROWTH_RUNNING: AtomicBool = AtomicBool::new(false);

/// 事件上报信封里的桌面端指纹常量（对齐官方桌面端，与 Python 版同值）。
const IDE_NAME: &str = "WorkBuddy";
const IDE_VERSION: &str = "5.5.6";
const COMMIT: &str = "5f9692923c93033111c51ad7b003eb80204a9b75";
const RELEASE_DATE: i64 = 1_789_036_585_355;
const UA_SHORT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 WorkBuddy/5.5.4";

// ---------------------------------------------------------------------------
// 设备指纹
// ---------------------------------------------------------------------------

/// 由 uid 稳定派生 36 位 hex 设备标识（与 Python 版 `md5(salt:uid)[:36]` 等价语义：
/// 同账号每次相同）。用 core 已有的 sha1 实现，避免新增 md5 依赖。
fn derive_id(uid: &str, salt: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("{salt}:{uid}").as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(40);
    for b in digest.iter() {
        hex.push_str(&format!("{b:02x}"));
    }
    hex.truncate(36);
    hex
}

// ---------------------------------------------------------------------------
// HTTP 请求
// ---------------------------------------------------------------------------

/// 成长中心请求头：在鉴权头基础上加 web 域标识（与 travel 一致）。
fn build_growth_headers(account: &Value) -> HashMap<String, String> {
    let mut headers = build_auth_headers(account);
    headers.insert("x-client-platform".to_string(), "web".to_string());
    headers.insert("origin".to_string(), WORKBUDDY_API_ENDPOINT.to_string());
    headers.insert(
        "referer".to_string(),
        format!("{WORKBUDDY_API_ENDPOINT}/profile/growth-center"),
    );
    headers
}

/// 判定是否因 token 失效被拒（用于触发刷新重试）。与 checkin 版同义。
fn is_unauthorized(resp: &Value) -> bool {
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code == 401 || code == 403 {
        return true;
    }
    let msg = resp
        .get("message")
        .or_else(|| resp.get("msg"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    ["unauthorized", "401", "登录", "失效", "过期", "token"]
        .iter()
        .any(|k| msg.contains(k))
}

/// 发成长中心请求；遇到未授权且存在 refresh token 时刷新一次并重试。
async fn growth_request(path: &str, method: &str, body: Option<Value>, account: &Value) -> Value {
    let url = format!("{WORKBUDDY_API_ENDPOINT}{path}");
    let headers = build_growth_headers(account);
    let mut resp = http_request(&url, method, body.clone(), Some(&headers)).await;
    if is_unauthorized(&resp)
        && !account
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
    {
        let refreshed = refresh_account_token(account.clone()).await;
        let headers = build_growth_headers(&refreshed);
        resp = http_request(&url, method, body, Some(&headers)).await;
    }
    resp
}

// ---------------------------------------------------------------------------
// 任务名中文映射
// ---------------------------------------------------------------------------

/// 任务 code → 中文名（覆盖成长中心常见任务；未知 code 原样返回）。
pub fn task_cn(code: &str) -> String {
    let name = match code {
        "design_creative" => "设计创意模式",
        "explore_inspiration" => "探索优秀灵感",
        "desktop_chat" => "桌面端对话",
        "try_skill" => "尝鲜热门技能",
        "use_library" => "体验资料库",
        "tencent_cloud_expert" => "腾讯轻量云专家",
        "peacekeeper_theme" => "和平精英主题",
        "Buddy_App" => "发现应用",
        "Buddy_App_QQ" => "企鹅教师助手",
        "glm_chat" => "GLM-5.2模型对话",
        "chat_5_times" => "和AI聊天5次",
        "night_owl" => "夜猫子活动",
        "summon_team" => "召唤3次专家团",
        "summon_expert" => "召唤5次专家",
        "template_5" => "使用5个模板",
        "set_automation" => "设置自动化任务",
        "adopt_buddy" => "领取Buddy",
        _ => return code.to_string(),
    };
    name.to_string()
}

// ---------------------------------------------------------------------------
// 任务：列表 / 接受 / 领奖
// ---------------------------------------------------------------------------

/// GET /v2/activity/growth/tasks —— 结构化任务列表。
pub async fn list_tasks(account: &Value) -> Value {
    let resp = growth_request("/v2/activity/growth/tasks", "GET", None, account).await;
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = resp
            .get("message")
            .or_else(|| resp.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return json!({ "ok": false, "error": if msg.is_empty() { format!("code={code}") } else { msg } });
    }
    let tasks = resp
        .get("data")
        .and_then(|d| d.get("tasks"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    let items: Vec<Value> = tasks
        .iter()
        .filter_map(|t| t.as_object())
        .map(|t| {
            let code = t.get("task_code").and_then(|v| v.as_str()).unwrap_or("");
            let progress = t.get("progress").and_then(|p| p.as_object());
            let current = progress.and_then(|p| p.get("current")).cloned().unwrap_or(Value::Null);
            let target = progress.and_then(|p| p.get("target")).cloned().unwrap_or(Value::Null);
            json!({
                "taskCode": code,
                "name": task_cn(code),
                "acceptStatus": t.get("accept_status").cloned().unwrap_or(Value::Null),
                "status": t.get("status").cloned().unwrap_or(Value::Null),
                "current": current,
                "target": target,
                "raw": t.clone(),
            })
        })
        .collect();

    json!({ "ok": true, "tasks": items })
}

/// POST /v2/activity/growth/tasks/accept —— 批量接受未接受任务。
///
/// 响应逐任务返回 results[]；顶层 code=0 仅代表请求送达，不代表每项都登记成功。
pub async fn accept_tasks(account: &Value, codes: &[String]) -> Value {
    if codes.is_empty() {
        return json!({ "ok": true, "accepted": 0, "results": [] });
    }
    let resp = growth_request(
        "/v2/activity/growth/tasks/accept",
        "POST",
        Some(json!({ "task_codes": codes })),
        account,
    )
    .await;
    let results = resp
        .get("data")
        .and_then(|d| d.get("results"))
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    let accepted = results
        .iter()
        .filter(|r| r.get("status").and_then(|s| s.as_str()) == Some("accepted"))
        .count();
    json!({ "ok": true, "accepted": accepted, "results": results })
}

/// POST /activity/growth/tasks/{code}/claim —— 领取单个任务奖励。
///
/// 注意：官方领奖路径不带 `/v2` 前缀（实测）。
pub async fn claim_task(account: &Value, code: &str) -> Value {
    let path = format!("/activity/growth/tasks/{code}/claim");
    let resp = growth_request(&path, "POST", Some(json!({})), account).await;
    let data = resp.get("data").cloned().unwrap_or_else(|| json!({}));
    let already = data.get("already_claimed").and_then(|v| v.as_bool()).unwrap_or(false);
    json!({ "ok": true, "code": code, "already_claimed": already, "data": data })
}

/// 扫描任务列表，把所有「已完成但未领奖」的任务依次领取。
pub async fn claim_all(account: &Value) -> Value {
    let list = list_tasks(account).await;
    let tasks = list
        .get("tasks")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    let mut claimed = Vec::new();
    let mut skipped = 0;
    for t in &tasks {
        let acc = t.get("acceptStatus").and_then(|s| s.as_str()).unwrap_or("");
        let cur = t.get("current").and_then(|v| v.as_i64()).unwrap_or(0);
        let tgt = t.get("target").and_then(|v| v.as_i64()).unwrap_or(0);
        if acc == "claimed" {
            skipped += 1;
            continue;
        }
        // 完成可领：进度已满 且 未领奖
        if tgt == 0 || cur < tgt {
            continue;
        }
        let Some(code) = t.get("taskCode").and_then(|c| c.as_str()) else {
            continue;
        };
        let res = claim_task(account, code).await;
        if res.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            claimed.push(json!({ "code": code, "name": task_cn(code), "data": res.get("data") }));
        }
    }
    json!({ "ok": true, "claimed": claimed, "already": skipped })
}

// ---------------------------------------------------------------------------
// 事件上报
// ---------------------------------------------------------------------------

/// 批量上报事件到 /v2/report（裸数组，每事件自带完整信封；对齐 Python 版 report()）。
pub async fn report_events(account: &Value, events: Vec<Value>) -> Value {
    let Some(uid) = account.get("uid").and_then(|v| v.as_str()) else {
        return json!({ "ok": false, "error": "账号缺少 uid" });
    };
    let nick = account_display_name(account);
    let now = now_ms();
    let machine = derive_id(uid, "machine");
    let session = derive_id(uid, "session");
    let mut out = Vec::with_capacity(events.len());
    for ev in events {
        let mut env = json!({
            "timestamp": now,
            "reportDelay": 0,
            "userId": uid,
            "userNickname": nick,
            "ideName": IDE_NAME,
            "ideType": IDE_NAME,
            "ideVersion": IDE_VERSION,
            "machineId": machine,
            "sessionId": session,
            "mode": "CLOUD",
            "userAgent": UA_SHORT,
            "os": "Win32",
            "arch": "x64",
            "osVersion": "10.0.26220",
            "timezone": "Asia/Shanghai",
            "product": "SaaS",
            "releaseDate": RELEASE_DATE,
            "commit": COMMIT,
            "extName": "workbuddy-desktop",
            "extVersion": IDE_VERSION,
            "cpuCores": 20,
            "memorySize": 24,
        });
        if let Some(obj) = ev.as_object() {
            if let Some(em) = env.as_object_mut() {
                for (k, v) in obj { em.insert(k.clone(), v.clone()); }
            }
        }
        out.push(env);
    }
    let resp = growth_request("/v2/report", "POST", Some(json!(out)), account).await;
    json!({ "ok": true, "count": out.len(), "raw": resp })
}


/// 取单个任务的当前进度（用于刷进度前判断是否已完成）。
async fn task_progress(account: &Value, code: &str) -> (String, i64, i64) {
    let list = list_tasks(account).await;
    let items = list.get("tasks").and_then(|t| t.as_array()).cloned().unwrap_or_default();
    for t in &items {
        if t.get("taskCode").and_then(|c| c.as_str()) == Some(code) {
            let status = t.get("acceptStatus").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let cur = t.get("current").and_then(|v| v.as_i64()).unwrap_or(0);
            let tgt = t.get("target").and_then(|v| v.as_i64()).unwrap_or(0);
            return (status, cur, tgt);
        }
    }
    (String::new(), 0, 0)
}

/// 刷核心纯事件任务的进度（不需要 SSE 对话）：模板5、设计创意、自动化、优秀灵感。
pub async fn progress_core_tasks(account: &Value) -> Vec<String> {
    let mut done = Vec::new();

    let (st, cur, tgt) = task_progress(account, "template_5").await;
    if st != "completed" && st != "claimed" && cur < tgt {
        let scenes = [
            ("01-ProductDesign", "产品设计"),
            ("02-Marketing", "营销文案"),
            ("03-DataAnalysis", "数据分析"),
            ("04-CodeReview", "代码审查"),
            ("05-Report", "报告撰写"),
        ];
        for (tid, name) in scenes.iter() {
            let evs = vec![
                json!({"eventCode":"agent_task_created","source":"CLOUD","name":"","mode":"craft","requestModelId":"default","action":tid,"has_template":true,"template_id":tid,"template_name":name}),
                json!({"eventCode":"agent_task_created_with_template","templateId":tid,"templateName":name,"isCustomModel":true,"id":tid,"name":name}),
            ];
            report_events(account, evs).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        done.push("template_5".to_string());
    }

    let (st, _, _) = task_progress(account, "create_canvas").await;
    if st != "completed" && st != "claimed" {
        report_events(account, vec![
            json!({"eventCode":"agent_task_created","source":"CLOUD","name":"","mode":"craft","requestModelId":"default","task_mode":"design"}),
            json!({"eventCode":"wbx_design_canvas_task_create"}),
        ]).await;
        done.push("create_canvas".to_string());
    }

    let (st, _, _) = task_progress(account, "automation_1").await;
    if st != "completed" && st != "claimed" {
        report_events(account, vec![
            json!({"eventCode":"agent_task_created","source":"CLOUD","name":"","mode":"craft","requestModelId":"default","task_mode":"automation","isAutomationBackground":true}),
            json!({"eventCode":"automated_task_create_suc","action":"create"}),
            json!({"eventCode":"automated_task_execute","action":"execute"}),
        ]).await;
        done.push("automation_1".to_string());
    }

    let (st, _, _) = task_progress(account, "playbook_prompt").await;
    if st != "completed" && st != "claimed" {
        let rid = uuid::Uuid::new_v4().to_string();
        report_events(account, vec![
            json!({"eventCode":"playbook_prompt_send","ext1":rid,"requestId":rid,"id":"01-ProductDesign","name":"产品设计","type":"other","promptLength":30,"isOfficial":1,"source":"growth-center"}),
        ]).await;
        done.push("playbook_prompt".to_string());
    }


    // ---- 体验资料库（Library_read） ----
    let (st, _, _) = task_progress(account, "Library_read").await;
    if st != "completed" && st != "claimed" {
        let uid = account.get("uid").and_then(|v| v.as_str()).unwrap_or("");
        let nick = account_display_name(account);
        let web_machine = derive_id(uid, "webmachine");
        let now = now_ms();
        report_events(account, vec![
            json!({"eventCode":"web_element_click","timestamp":now,"reportDelay":0,
                "pageURL":"https://www.workbuddy.cn/library/doc/intro",
                "elementId":"library_doc_intro_click","elementName":"WorkBuddy资料库介绍",
                "os":"Win32","arch":"x64","osVersion":"10.0",
                "userAgent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                "machineId":web_machine,"userId":uid,"userNickname":nick}),
        ]).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        done.push("Library_read".to_string());
    }

    // ---- 发现应用 / 企鹅教师助手 ----
    for (code, buddy_id, buddy_name) in [
        ("Buddy_App", "buddy-app-default", "发现应用"),
        ("Buddy_App_QQ", "cb_y5Dy46tPQGGWtueMxXbe", "企鹅教师助手"),
    ] {
        let (st, _, _) = task_progress(account, code).await;
        if st == "completed" || st == "claimed" { continue; }
        report_events(account, vec![
            json!({"eventCode":"buddyapp_discover_click","buddyId":buddy_id,"buddyName":buddy_name}),
            json!({"eventCode":"buddyapp_show","elementId":buddy_id,"elementName":buddy_name,"position":2,"buddyId":buddy_id,"buddyName":buddy_name}),
            json!({"eventCode":"buddyapp_enter_click","elementId":buddy_id,"elementName":buddy_name,"position":2,"isFirstPage":"1","buddyId":buddy_id,"buddyName":buddy_name}),
            json!({"eventCode":"buddyapp_auth_confirm_click","elementId":buddy_id,"elementName":buddy_name,"buddyId":buddy_id,"buddyName":buddy_name}),
            json!({"eventCode":"buddyapp_bindaccount_skip_click","elementId":buddy_id,"elementName":buddy_name,"buddyId":buddy_id,"buddyName":buddy_name}),
        ]).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        done.push(code.to_string());
    }

    // ---- 召唤5次专家（expert_5） ----
    let (st, cur, tgt) = task_progress(account, "expert_5").await;
    if st != "completed" && st != "claimed" && cur < tgt {
        let needed = (tgt - cur).max(0);
        for i in 0..needed {
            let eid = format!("expert-web-{:04}", i);
            let ename = format!("在线助手{}", i);
            report_events(account, vec![
                json!({"eventCode":"expert_summoned","id":eid,"name":ename,"type":"agent","expertTitle":"智能助手","expertType":"agent"}),
                json!({"eventCode":"expert_actual_use","id":eid,"name":ename,"type":"agent","expertTitle":"智能助手","expertType":"agent","source":"builtin","version":"","cost":0,"characterCount":12}),
            ]).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        done.push("expert_5".to_string());
    }

    // ---- 召唤3次专家团（Expert_team_use_3） ----
    let (st, cur, tgt) = task_progress(account, "Expert_team_use_3").await;
    if st != "completed" && st != "claimed" && cur < tgt {
        let needed = (tgt - cur).max(0);
        for i in 0..needed {
            let tid = format!("team-web-{:04}", i);
            let tname = format!("专家团{}", i);
            report_events(account, vec![
                json!({"eventCode":"expert_actual_use","id":tid,"name":tname,"expertTitle":"行业专家","type":"team","expertType":"team","source":"builtin","version":"","cost":8,"characterCount":20}),
            ]).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        done.push("Expert_team_use_3".to_string());
    }

    // ---- 通用：遍历所有未完成任务 ----
    let all = list_tasks(account).await;
    let items = all.get("tasks").and_then(|t| t.as_array()).cloned().unwrap_or_default();
    for t in &items {
        let code = t.get("taskCode").and_then(|c| c.as_str()).unwrap_or("").to_string();
        let name = t.get("name").and_then(|c| c.as_str()).unwrap_or("").to_string();
        let st2 = t.get("acceptStatus").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let cur = t.get("current").and_then(|v| v.as_i64()).unwrap_or(0);
        let tgt = t.get("target").and_then(|v| v.as_i64()).unwrap_or(1);
        if st2 == "completed" || st2 == "claimed" { continue; }
        if cur >= tgt && tgt > 0 { continue; }
        let needed = (tgt - cur).max(1);
        let conv = format!("conv-{}", uuid::Uuid::new_v4());
        let req = format!("req-{}", uuid::Uuid::new_v4());
        let msg = format!("msg-{}", uuid::Uuid::new_v4());
        let evs: Vec<Value> = match code.as_str() {
            "RichMeow_Chat" => vec![
                json!({"eventCode":"agent_task_created","source":"LOCAL","name":"working","task_target":"local","mode":"craft","requestModelId":"fast-model","requestModelName":"fast-model","has_repo":false,"repo_type":"none","workspace_type":"empty","has_connector":false,"connector_types":[],"has_mention":false,"mention_types":[],"has_template":false,"action":"","template_name":"","has_expert":false,"expert_id":"","expert_name":"","expert_industry_id":"","has_skill":false,"skill_names":[],"conversationId":conv,"messageId":msg,"buddyId":"","buddyName":""}),
                json!({"eventCode":"chat_message_send","messageId":msg.clone()+"-assistant","historyCount":0,"isContextTruncated":false,"currentStepCount":1,"traceId":req,"rootRequestId":req,"parentConversationId":conv,"agentName":"cli","agentType":"main"}),
                json!({"eventCode":"chat_request_send","inputLength":24,"isPlan":false,"isAutoExecuteTerminal":false,"isAutoModify":false,"codebaseEnable":false,"maxToken":0,"maxSteps":500,"temperature":0,"maxRetries":0,"mentionContexts":[],"knowledgeId":[],"knowledgeName":[],"codebaseId":"","mentionContextCount":0,"command":"","recommendId":"","skillId":"","skillCount":0,"totalCount":0,"traceId":req,"rootRequestId":req,"parentConversationId":conv,"agentName":"cli","agentType":"main"}),
                json!({"eventCode":"chat_message_response","messageId":msg.clone()+"-assistant","responseModelId":"fast-model","inputToken":120,"outputToken":80,"totalToken":200,"cachedTokens":0,"cachedWriteTokens":0,"cachedMissTokens":0,"isSuccessful":true,"messageErrorCode":"","finishReason":"stop","traceId":req,"conversationId":conv,"rootRequestId":req,"parentConversationId":conv,"agentName":"cli","agentType":"main"}),
                json!({"eventCode":"chat_message_status","messageId":msg.clone()+"-assistant","messageErrorCode":"0","traceId":req,"rootRequestId":req,"parentConversationId":conv,"agentName":"cli","agentType":"main"}),
                json!({"eventCode":"chat_request_response","mode":"craft","toolCallCount":0,"inputToken":120,"outputToken":80,"totalToken":200,"cachedTokens":0,"cachedWriteTokens":0,"cachedMissTokens":0,"isSuccessful":true,"messageErrorCode":"","finishReason":"stop","rootRequestId":req,"parentConversationId":conv,"agentName":"cli","agentType":"main"}),
            ],
            "Expert_lighthouse" => vec![
                json!({"eventCode":"expert_summoned","id":"expert-lh","name":"轻量云专家","type":"agent","expertTitle":"轻量云专家","expertType":"agent","source":"builtin"}),
                json!({"eventCode":"expert_actual_use","id":"expert-lh","name":"轻量云专家","expertTitle":"轻量云专家","type":"agent","expertType":"agent","source":"builtin","version":"","cost":0,"characterCount":12,"conversationId":conv,"requestId":req,"messageId":msg,"requestModelId":"deepseek-v4-flash","requestModelName":"DeepSeek V4 Flash"}),
            ],
            "Hp_Appearance" => vec![
                json!({"eventCode":"appearance_skin_apply","action":"apply","source":"settings_close","id":"theme-tkmw7j","vipLevel":"free","series":"craft","type":"personal"}),
            ],
            "skill_1" => vec![
                json!({"eventCode":"skill_info","skillId":"skill-algo","skillName":"algorithmic-trading","mode":"LOCAL","source":"builtin"}),
            ],
            "Buddy_App_QQ" => vec![
                json!({"eventCode":"buddyapp_discover_click","buddyId":"cb_y5Dy46tPQGGWtueMxXbe","buddyName":"企鹅教师助手","mode":"LOCAL"}),
                json!({"eventCode":"buddyapp_show","elementId":"cb_y5Dy46tPQGGWtueMxXbe","elementName":"企鹅教师助手","position":2,"buddyId":"cb_y5Dy46tPQGGWtueMxXbe","buddyName":"企鹅教师助手","mode":"LOCAL"}),
                json!({"eventCode":"buddyapp_enter_click","elementId":"cb_y5Dy46tPQGGWtueMxXbe","elementName":"企鹅教师助手","position":2,"isFirstPage":"1","buddyId":"cb_y5Dy46tPQGGWtueMxXbe","buddyName":"企鹅教师助手","mode":"LOCAL"}),
                json!({"eventCode":"buddyapp_auth_confirm_click","elementId":"cb_y5Dy46tPQGGWtueMxXbe","elementName":"企鹅教师助手","buddyId":"cb_y5Dy46tPQGGWtueMxXbe","buddyName":"企鹅教师助手","mode":"LOCAL"}),
                json!({"eventCode":"buddyapp_bindaccount_skip_click","elementId":"cb_y5Dy46tPQGGWtueMxXbe","elementName":"企鹅教师助手","buddyId":"cb_y5Dy46tPQGGWtueMxXbe","buddyName":"企鹅教师助手","mode":"LOCAL"}),
            ],
            "black_cat" => { done.push(format!("跳过:{}({}) 需夜间真实对话", name, code)); continue; }
            "Expert_Philanthropy" => { done.push(format!("跳过:{}({}) 需真实捐款", name, code)); continue; }
            "wb_wechat_oa_subscribe_task" => { done.push(format!("跳过:{}({}) 需真实关注", name, code)); continue; }
            "Model_chat_GLM5.2" => { done.push(format!("跳过:{}({}) 需真实对话", name, code)); continue; }
            "chat_5" => { done.push(format!("跳过:{}({}) 需真实对话", name, code)); continue; }
            _ => {
                done.push(format!("未识别:{}({})", name, code));
                continue;
            }
        };
        for ev_chunk in evs.chunks(5) {
            let r = report_events(account, ev_chunk.to_vec()).await;
            let raw = format!("{}", r.get("raw").unwrap_or(&json!(null)));
            done.push(format!("  [{}] resp: {}", code, raw));
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        done.push(code);
    }
    done
}

// ---------------------------------------------------------------------------
// 成长档案
// ---------------------------------------------------------------------------

/// 成长档案：等级/能量/连签/profile 汇总。
pub async fn growth_profile(account: &Value) -> Value {
    let profile = growth_request("/v2/activity/growth/profile", "GET", None, account)
        .await
        .get("data")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let energy = growth_request("/v2/activity/growth/energy", "GET", None, account)
        .await
        .get("data")
        .and_then(|d| d.get("balance").cloned())
        .unwrap_or(Value::Null);
    let streak = growth_request("/v2/activity/growth/streak", "GET", None, account)
        .await
        .get("data")
        .cloned()
        .unwrap_or_else(|| json!({}));
    json!({
        "ok": true,
        "level": profile.get("level").cloned().unwrap_or(Value::Null),
        "points": profile.get("points").cloned().unwrap_or(Value::Null),
        "energy": energy,
        "streakDays": streak.get("streak").and_then(|s| s.get("days")).cloned().unwrap_or(Value::Null),
        "profile": profile,
    })
}

// ---------------------------------------------------------------------------
// 抽奖 / 盲盒
// ---------------------------------------------------------------------------

/// 大转盘：查剩余次数并循环抽完。`max_times` 为单次调用最多抽奖次数上限。
pub async fn lottery_draw(account: &Value, max_times: u32) -> Value {
    let chances_resp = growth_request("/v2/activity/growth/lottery/chances", "GET", None, account).await;
    let chances = chances_resp
        .get("data")
        .and_then(|d| {
            d.get("balance")
                .or_else(|| d.get("chances"))
                .or_else(|| d.get("remaining"))
                .and_then(|v| v.as_i64())
        })
        .unwrap_or(0)
        .max(0) as u32;
    if chances == 0 {
        return json!({ "ok": true, "drawn": 0, "prizes": [] });
    }
    let times = chances.min(max_times);
    let mut prizes = Vec::new();
    for i in 0..times {
        if i > 0 {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let client_token = format!("draw-{}", uuid::Uuid::new_v4());
        let resp = growth_request(
            "/v2/activity/growth/lottery/draw",
            "POST",
            Some(json!({ "client_token": client_token })),
            account,
        )
        .await;
        if resp.get("code").and_then(|v| v.as_i64()) != Some(0) {
            break;
        }
        let prize = resp
            .get("data")
            .and_then(|d| d.get("prize_name").or_else(|| d.get("name")))
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        prizes.push(prize);
    }
    json!({ "ok": true, "drawn": prizes.len(), "prizes": prizes })
}

/// 盲盒：能量足够时开启，最多 `max_times` 次。
pub async fn blindbox_open(account: &Value, max_times: u32) -> Value {
    let mut got = Vec::new();
    for i in 0..max_times {
        if i > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let resp = growth_request(
            "/v2/activity/growth/buddy/open",
            "POST",
            Some(json!({ "count": 1 })),
            account,
        )
        .await;
        if resp.get("code").and_then(|v| v.as_i64()) != Some(0) {
            break;
        }
        if let Some(it) = resp
            .get("data")
            .and_then(|d| d.get("results"))
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
        {
            let ins = it.get("instance").unwrap_or(it);
            let tpl = it.get("template").cloned().unwrap_or_else(|| json!({}));
            let name = ins
                .get("name")
                .or_else(|| tpl.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let rarity = ins
                .get("rarity")
                .or_else(|| tpl.get("rarity"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            got.push(if rarity.is_empty() {
                name.to_string()
            } else {
                format!("{name}({rarity})")
            });
        }
    }
    json!({ "ok": true, "opened": got.len(), "items": got })
}

// ---------------------------------------------------------------------------
// 一键闭环
// ---------------------------------------------------------------------------

/// 单账号一键跑一遍：接受未接受任务 → 补领 → 抽奖 → 盲盒。
///
/// 真实 AI 对话类任务（chat 5 次、夜猫子）不在本期自动化范围内，保持手动。
pub async fn run_growth_once(account: &Value) -> Value {
    if variant_of(account) != WbVariant::Cn {
        return json!({ "ok": false, "skipped": true, "reason": "仅国内版支持成长中心" });
    }

    // 1) 接受所有未接受任务
    let list = list_tasks(account).await;
    let todo: Vec<String> = list
        .get("tasks")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|t| {
            (t.get("acceptStatus").and_then(|s| s.as_str()) == Some("not_accepted"))
                .then(|| t.get("taskCode").and_then(|c| c.as_str()).map(String::from))
                .flatten()
        })
        .collect();
    let accept = if todo.is_empty() {
        json!({ "accepted": 0, "results": [] })
    } else {
        accept_tasks(account, &todo).await
    };
    // 2) 上报事件刷核心任务进度（模板/设计/自动化/灵感）
    let progressed = progress_core_tasks(account).await;
    tokio::time::sleep(Duration::from_secs(2)).await;


    // 2) 补领所有已完成未领奖任务
    let claim = claim_all(account).await;

    // 3) 抽奖
    let lottery = lottery_draw(account, 50).await;

    // 4) 盲盒
    let blindbox = blindbox_open(account, 5).await;

    // 5) 成长档案
    let profile = growth_profile(account).await;

    json!({
        "ok": true,
        "account": account_display_name(account),
        "accepted": accept.get("accepted").cloned().unwrap_or(Value::Null),
        "progressed": json!(progressed),
        "claimed": claim.get("claimed").cloned().unwrap_or(Value::Null),
        "lottery": lottery,
        "blindbox": blindbox,
        "profile": profile,
    })
}

/// 对全部国内版账号执行一轮成长任务。
pub async fn run_growth_all() -> Value {
    let Some(_guard) = RunFlagGuard::try_acquire(&GROWTH_RUNNING) else {
        return json!({ "ok": false, "status": "skipped", "reason": "already_running" });
    };
    let accounts: Vec<Value> = load_accounts()
        .into_iter()
        .filter(|acc| variant_of(acc) == WbVariant::Cn)
        .collect();
    if accounts.is_empty() {
        return json!({ "ok": true, "accounts": [] });
    }
    let mut results = Vec::new();
    for acc in &accounts {
        let r = run_growth_once(acc).await;
        results.push(json!({
            "accountId": acc.get("id").cloned().unwrap_or(Value::Null),
            "account": account_display_name(acc),
            "result": r,
        }));
    }
    json!({ "ok": true, "accounts": results })
}

// ---------------------------------------------------------------------------
// 工具：供 api 层调用的单账号查询入口
// ---------------------------------------------------------------------------

/// 供路由层：按账号 id 跑一轮。
pub async fn run_growth_for_account(account_id: &str) -> Value {
    let Some(acc) = crate::modules::account::find_account(account_id) else {
        return json!({ "ok": false, "error": "账号不存在" });
    };
    run_growth_once(&acc).await
}

/// 供路由层：单账号查询任务列表。
pub async fn tasks_for_account(account_id: &str) -> Value {
    let Some(acc) = crate::modules::account::find_account(account_id) else {
        return json!({ "ok": false, "error": "账号不存在" });
    };
    if variant_of(&acc) != WbVariant::Cn {
        return json!({ "ok": false, "skipped": true, "reason": "仅国内版支持成长中心", "tasks": [] });
    }
    list_tasks(&acc).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn derive_id_is_stable_and_36_hex() {
        let a = derive_id("uid-123", "machine");
        let b = derive_id("uid-123", "machine");
        let c = derive_id("uid-123", "session");
        assert_eq!(a, b, "同账号同 salt 必须稳定");
        assert_ne!(a, c, "不同 salt 必须不同");
        assert_eq!(a.len(), 36, "派生 id 长度对齐 Python 版 [:36]");
        assert!(a.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn task_cn_known_and_fallback() {
        assert_eq!(task_cn("template_5"), "使用5个模板".to_string());
        assert_eq!(task_cn("design_creative"), "设计创意模式".to_string());
        assert_eq!(task_cn("some_unknown_code"), "some_unknown_code".to_string());
    }

    #[test]
    fn is_unauthorized_detects_code() {
        assert!(is_unauthorized(&json!({"code": 401})));
        assert!(is_unauthorized(&json!({"code": 403})));
        assert!(!is_unauthorized(&json!({"code": 0})));
    }
}
