//! models.json 容错读取与追加式合并（§11.5 / D-08 / AC-08）。

use serde_json::{json, Map, Value};

use crate::config;

#[derive(Debug, Clone)]
pub struct NormalizedConfig {
    pub models: Vec<Value>,
    pub available_models: Vec<String>,
}

/// 容错读取：数组根 → 归并为 {models, availableModels:[]}；非法/空 → 空结构；不丢数据（调用方负责先备份）。
pub fn normalize(raw: &str) -> NormalizedConfig {
    match serde_json::from_str::<Value>(raw) {
        Err(_) => NormalizedConfig { models: Vec::new(), available_models: Vec::new() },
        Ok(Value::Array(arr)) => NormalizedConfig { models: arr, available_models: Vec::new() },
        Ok(Value::Object(obj)) => {
            let models = obj
                .get("models")
                .and_then(|m| m.as_array())
                .cloned()
                .unwrap_or_default();
            let available_models = obj
                .get("availableModels")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            NormalizedConfig { models, available_models }
        }
        Ok(_) => NormalizedConfig { models: Vec::new(), available_models: Vec::new() },
    }
}

/// 是否为本工具管理的条目：name == "GP" 或历史 newapi- 前缀（§11.3 归属标识）。
pub fn is_managed_entry(v: &Value) -> bool {
    v.get("name")
        .and_then(|n| n.as_str())
        .map(|n| n == config::MANAGED_MODEL_NAME || n.starts_with(config::MANAGED_MODEL_ID_PREFIX))
        .unwrap_or(false)
}

/// 追加式合并（D-08）：剔除旧托管条目，保留用户既有条目，追加新托管条目。
/// availableModels 尊重现状，写入方绝不主动收敛（§11.3 血的教训）。
pub fn merge(current: &NormalizedConfig, entries: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = current
        .models
        .iter()
        .filter(|e| !is_managed_entry(e))
        .cloned()
        .collect();
    out.extend(entries);
    out
}

/// 顶层 key 级重建（修 H3）：以原文件对象为底，只覆盖 `models`；
/// 其他顶层字段（含 WorkBuddy 未来新增字段）原样保留；`availableModels` 原样保留，
/// 原本缺失时才补空数组（§11.2 结构要求）。数组根/畸形文件归一为 {models, availableModels:[]}。
pub fn rebuild_outer(raw: &str, merged_models: Vec<Value>) -> Value {
    let mut outer = serde_json::from_str::<Value>(raw).unwrap_or_else(|_| json!({}));
    if !outer.is_object() {
        outer = json!({});
    }
    let map: &mut Map<String, Value> = outer.as_object_mut().expect("guaranteed object");
    map.insert("models".into(), Value::Array(merged_models));
    map.entry("availableModels".to_string()).or_insert_with(|| Value::Array(vec![]));
    outer
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_array_root() {
        let n = normalize("[{\"id\":\"a\"}]");
        assert_eq!(n.models.len(), 1);
        assert!(n.available_models.is_empty());
    }

    #[test]
    fn normalize_invalid_returns_empty() {
        let n = normalize("not json");
        assert!(n.models.is_empty());
    }

    #[test]
    fn merge_replaces_managed_and_keeps_user_entries() {
        let current = normalize(
            r#"{"models":[{"id":"mine","name":"Mine"},{"id":"old","name":"GP"}],"availableModels":[]}"#,
        );
        let entries = vec![json!({"id": "new", "name": "GP"})];
        let merged = merge(&current, entries);
        let ids: Vec<String> = merged
            .iter()
            .map(|e| e.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string())
            .collect();
        assert_eq!(ids, vec!["mine".to_string(), "new".to_string()]);
    }

    #[test]
    fn rebuild_preserves_unknown_top_level_and_available_models() {
        let raw = r#"{"models":[],"availableModels":["a"],"futureField":{"x":1}}"#;
        let outer = rebuild_outer(raw, vec![json!({"id": "m", "name": "GP"})]);
        // 用户自定义的下拉范围必须原样保留（绝不主动收敛）
        assert_eq!(outer["availableModels"], json!(["a"]));
        // 未知顶层字段不丢
        assert_eq!(outer["futureField"], json!({"x": 1}));
        // models 被替换为合并结果
        assert_eq!(outer["models"].as_array().map(|a| a.len()), Some(1));
    }

    #[test]
    fn rebuild_fills_missing_available_models_and_normalizes_array_root() {
        let outer = rebuild_outer(r#"[{"id":"a"}]"#, vec![json!({"name": "GP"})]);
        assert_eq!(outer["availableModels"], json!([]));
        assert_eq!(outer["models"].as_array().map(|a| a.len()), Some(1));
    }
}
