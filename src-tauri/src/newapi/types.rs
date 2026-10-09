//! new-api 接口 DTO。
//! 命名规则：需要同时进出（Serialize+Deserialize）的结构体，序列化统一 camelCase（rename_all），
//! 反序列化用 alias 接收站点的 snake_case 原始键——rename_all 会同时作用于反序列化，
//! 漏写 alias 会让多单词字段静默归零（LogItem 曾因此整列空白）。

use serde::{Deserialize, Serialize};

/// /api/* 统一信封：认 success 字段，不认 HTTP 状态码（§8 判定铁律）。
/// 注意：部分写接口（如 POST /api/token/）成功时只有 success/message，没有 data 键。
#[derive(Debug, Clone, Deserialize)]
pub struct Envelope<T> {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub message: String,
    /// 新版 new-api 安全会话体系返回的机器可读错误码（如 AUTH_SESSION_LIMIT）。
    #[serde(default)]
    pub code: Option<String>,
    // 注意：data 不能加 #[serde(default)]，否则 serde 会给 T 追加 Default 约束；
    // Option 缺键时本就自动归 None。
    pub data: Option<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginData {
    /// Gen B 为 access_token；Gen A（旧版）为 accessToken，alias 兼容。
    #[serde(default, alias = "accessToken")]
    pub access_token: String,
    #[serde(default)]
    pub user: Option<UserBrief>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserBrief {
    #[serde(default)]
    pub id: i64,
    /// 登录响应（buildSelfUserData）已带 username/email，免再打一次 self_info。
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub email: String,
}

/// GET /api/user/self 的 data（snake_case）。
#[derive(Debug, Clone, Deserialize)]
pub struct SelfData {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub quota: i64,
    #[serde(default)]
    pub used_quota: i64,
    #[serde(default)]
    pub request_count: i64,
    #[serde(default)]
    pub group: String,
}

/// GET /api/token/search 的 data：分页信封 { items, total, ... }。
#[derive(Debug, Clone, Deserialize)]
pub struct TokenPage {
    #[serde(default)]
    pub items: Vec<TokenListItem>,
    #[serde(default)]
    pub total: i64,
}

/// 令牌列表项（列表接口返回的是脱敏条目，无明文 key）。
#[derive(Debug, Clone, Deserialize)]
pub struct TokenListItem {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub created_time: i64,
}

/// 通用分页信封 data：{ items: [...] }。items 缺键归 None（不能加 serde(default)，
/// 否则 serde 会给 T 追加 Default 约束）。
#[derive(Debug, Clone, Deserialize)]
pub struct PageData<T> {
    pub items: Option<Vec<T>>,
}

/// GET /api/user/topup/self 的条目（充值记录）。字段按站点为准，全部容错缺省。
/// 入站 snake_case（alias），出站 camelCase。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopUpRecord {
    #[serde(default)]
    pub money: String,
    #[serde(default)]
    pub amount: i64,
    #[serde(default, alias = "create_time")]
    pub create_time: i64,
    #[serde(default, alias = "trade_no")]
    pub trade_no: String,
    #[serde(default)]
    pub status: String,
}

/// GET /api/log/self 的条目（type=2 计费日志）。
/// 入站 snake_case（alias，站点原始键），出站 camelCase（前端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogItem {
    #[serde(default)]
    pub id: i64,
    #[serde(default, alias = "created_at")]
    pub created_at: i64,
    #[serde(default, alias = "token_name")]
    pub token_name: String,
    #[serde(default, alias = "model_name")]
    pub model_name: String,
    #[serde(default)]
    pub quota: i64,
    #[serde(default, alias = "prompt_tokens")]
    pub prompt_tokens: i64,
    #[serde(default, alias = "completion_tokens")]
    pub completion_tokens: i64,
    #[serde(default, alias = "use_time")]
    pub use_time: i64,
    /// 日志正文（topup 类日志的金额在 content 里，如「通过兑换码充值 ＄0.001470 额度」）
    #[serde(default, alias = "content")]
    pub content: String,
    #[serde(default, alias = "is_stream")]
    pub is_stream: bool,
}

/// GET /api/log/self 的分页信封。
#[derive(Debug, Clone, Deserialize)]
pub struct LogPage {
    #[serde(default)]
    pub items: Option<Vec<LogItem>>,
    #[serde(default)]
    pub total: i64,
}

/// GET /api/status 提炼出的实例能力（命令层再转 camelCase）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    pub version: String,
    pub server_address: String,
    pub email_verification: bool,
    pub self_use_mode: bool,
    pub register_enabled: bool,
    pub turnstile_check: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归锁：站点 snake_case 必须全部命中，不得因 rename_all 静默归零。
    #[test]
    fn log_item_accepts_snake_case_and_emits_camel_case() {
        let raw = r#"{"id":7,"created_at":1791384732,"token_name":"t","model_name":"glm-5.3","quota":1225,"prompt_tokens":13,"completion_tokens":10,"use_time":2,"is_stream":true}"#;
        let item: LogItem = serde_json::from_str(raw).unwrap();
        assert_eq!(item.created_at, 1791384732);
        assert_eq!(item.model_name, "glm-5.3");
        assert_eq!(item.prompt_tokens, 13);
        assert_eq!(item.completion_tokens, 10);
        assert_eq!(item.use_time, 2);
        assert!(item.is_stream);
        let out = serde_json::to_value(&item).unwrap();
        assert_eq!(out["createdAt"], 1791384732);
        assert_eq!(out["modelName"], "glm-5.3");
        assert_eq!(out["useTime"], 2);
    }

    /// TopUpRecord 同规则：入站 snake_case、出站 camelCase。
    #[test]
    fn topup_record_accepts_snake_case_and_emits_camel_case() {
        let raw = r#"{"money":"50.00","amount":5000000,"create_time":1791384732,"trade_no":"abc123","status":"success"}"#;
        let r: TopUpRecord = serde_json::from_str(raw).unwrap();
        assert_eq!(r.create_time, 1791384732);
        assert_eq!(r.trade_no, "abc123");
        let out = serde_json::to_value(&r).unwrap();
        assert_eq!(out["createTime"], 1791384732);
        assert_eq!(out["tradeNo"], "abc123");
    }
}
