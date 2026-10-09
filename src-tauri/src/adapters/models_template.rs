//! 写入模板与 relatedModels 解析（AC-09，§11.2 / §22.3）。
//!
//! 为什么必须显式写 relatedModels：自定义模型不继承产品内置 defaultRelatedModels，
//! 不写会让 lite/reasoning 静默回退主模型 → 用户无声多花钱。

use serde_json::{json, Value};

use crate::config;

use super::types::{ModelSeed, WriteInput};

/// 规范化实例根地址：剥掉端点后缀，得到纯 base（https://api.greenpool.cn）。
pub fn base_url_of(url: &str) -> String {
    let mut base = url.trim().trim_end_matches('/').to_string();
    let lowered = base.to_ascii_lowercase();
    for suffix in ["/v1/chat/completions", "/chat/completions", "/v1"] {
        if lowered.ends_with(suffix) {
            base.truncate(base.len() - suffix.len());
            break;
        }
    }
    base.trim_end_matches('/').to_string()
}

/// 规范化实例根地址，产出完整补全地址：<base>/v1/chat/completions。
/// 幂等处理，绝不产出 …/v1/v1/…（§11.3）。
pub fn chat_completions_url(base_url: &str) -> String {
    format!("{}/v1/chat/completions", base_url_of(base_url))
}

#[derive(Debug, Clone, Default)]
pub struct RelatedModelsStatement {
    pub lite: Option<String>,
    pub reasoning: Option<String>,
}

/// 解析 relatedModels：显式指定（暂无）→ 关键词启发式 → 首/末项兜底（留 warning，§11.4）。
pub fn resolve_related(model_ids: &[String]) -> (RelatedModelsStatement, Vec<String>) {
    let mut warnings = Vec::new();

    let lite = model_ids
        .iter()
        .find(|m| {
            let lower = m.to_lowercase();
            config::RELATED_LITE_KEYWORDS.iter().any(|k| lower.contains(k))
        })
        .cloned();
    let reasoning = model_ids
        .iter()
        .find(|m| {
            let lower = m.to_lowercase();
            config::RELATED_REASONING_KEYWORDS.iter().any(|k| lower.contains(k))
        })
        .cloned();

    let lite = match lite {
        Some(x) => Some(x),
        None => {
            warnings.push("日常小任务模型走首项兜底".into());
            model_ids.first().cloned()
        }
    };
    let reasoning = match reasoning {
        Some(x) => Some(x),
        None => {
            warnings.push("深度思考模型走末项兜底".into());
            model_ids.last().cloned()
        }
    };

    (RelatedModelsStatement { lite, reasoning }, warnings)
}

/// 按固定结构构造一条模型条目（§11.2 逐字段裁决：name=GP / vendor=Custom / useCustomProtocol=false
/// / maxTokens=1_000_000 / supportsReasoning 显式 true / reasoning 默认开启可关）。
pub fn build_model_entry(
    input: &WriteInput,
    seed: &ModelSeed,
    related: &RelatedModelsStatement,
) -> Value {
    let id = seed.id.trim();
    // 逐模型思考档位（与 EP 系列对齐；未知模型走统一三档兜底）
    let (default_effort, supported_efforts) = config::reasoning_effort_for(id);
    let related_value = match (&related.lite, &related.reasoning) {
        (Some(lite), Some(reasoning)) => json!({ "lite": lite, "reasoning": reasoning }),
        (Some(lite), None) => json!({ "lite": lite }),
        (None, Some(reasoning)) => json!({ "reasoning": reasoning }),
        (None, None) => json!({}),
    };

    json!({
        "id": id,
        "name": config::MANAGED_MODEL_NAME,
        "vendor": config::MANAGED_MODEL_VENDOR,
        "apiKey": input.api_key,
        "url": chat_completions_url(&input.base_url),
        "maxInputTokens": config::MANAGED_MODEL_MAX_INPUT_TOKENS,
        "maxOutputTokens": config::MANAGED_MODEL_MAX_OUTPUT_TOKENS,
        "supportsToolCall": seed.supports_tool_call,
        "supportsImages": seed.supports_images,
        "supportsReasoning": seed.supports_reasoning,
        "useCustomProtocol": config::MANAGED_MODEL_USE_CUSTOM_PROTOCOL,
        "reasoning": {
            "canDisableThinking": config::REASONING_CAN_DISABLE_THINKING,
            "defaultEffort": default_effort,
            "supportedEfforts": supported_efforts,
        },
        "relatedModels": related_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalizes_v1_suffix() {
        assert_eq!(
            chat_completions_url("https://api.greenpool.cn"),
            "https://api.greenpool.cn/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://api.greenpool.cn/v1"),
            "https://api.greenpool.cn/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://api.greenpool.cn/v1/chat/completions"),
            "https://api.greenpool.cn/v1/chat/completions"
        );
    }

    #[test]
    fn related_falls_back_to_ends_with_warning() {
        let ids: Vec<String> = vec!["a".into(), "b".into()];
        let (related, warnings) = resolve_related(&ids);
        assert_eq!(related.lite.as_deref(), Some("a"));
        assert_eq!(related.reasoning.as_deref(), Some("b"));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn related_uses_keywords() {
        let ids: Vec<String> = vec!["deepseek-v4.1-flash".into(), "deepseek-v4-pro".into()];
        let (related, warnings) = resolve_related(&ids);
        assert_eq!(related.lite.as_deref(), Some("deepseek-v4.1-flash"));
        assert_eq!(related.reasoning.as_deref(), Some("deepseek-v4-pro"));
        assert!(warnings.is_empty());
    }

    /// 回归锁：GP 条目思考档位必须与 EP 系列逐模型对齐（2026-10-09 产品裁决）。
    #[test]
    fn model_entry_uses_per_model_effort_table() {
        let input = WriteInput {
            base_url: "https://api.greenpool.cn".into(),
            api_key: "k".into(),
            model_ids: vec![],
        };
        let related = RelatedModelsStatement::default();
        let seed = |id: &str| ModelSeed {
            id: id.into(),
            supports_tool_call: true,
            supports_images: true,
            supports_reasoning: true,
        };

        // fast-model：EP 实测 medium + [medium]
        let e = build_model_entry(&input, &seed("fast-model"), &related);
        assert_eq!(e["reasoning"]["defaultEffort"], "medium");
        assert_eq!(e["reasoning"]["supportedEfforts"], serde_json::json!(["medium"]));

        // deepseek-v4-pro：EP 实测 high + [high, xhigh]
        let e = build_model_entry(&input, &seed("deepseek-v4-pro"), &related);
        assert_eq!(e["reasoning"]["defaultEffort"], "high");
        assert_eq!(e["reasoning"]["supportedEfforts"], serde_json::json!(["high", "xhigh"]));

        // 未知模型：回退统一三档 + high
        let e = build_model_entry(&input, &seed("future-model-9"), &related);
        assert_eq!(e["reasoning"]["defaultEffort"], "high");
        assert_eq!(
            e["reasoning"]["supportedEfforts"],
            serde_json::json!(["low", "high", "xhigh"])
        );
    }
}
