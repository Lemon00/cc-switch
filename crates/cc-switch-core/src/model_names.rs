//! Shared model naming rules. No proxy, mode or filesystem dependencies.

pub const PROXY_OPUS_ALIAS: &str = "claude-opus-5";
pub const PROXY_SONNET_ALIAS: &str = "claude-sonnet-5";
pub const PROXY_FABLE_ALIAS: &str = "claude-fable-5";
pub const ONE_M_MARKER_FOR_CLIENT: &str = "[1m]";

/// Claude Code 通过 `[1M]` 后缀声明 100 万上下文能力；上游 API
/// 通常不接受这个本地能力标记，转发前需要剥离。
pub fn strip_one_m_suffix_for_upstream(model: &str) -> &str {
    let trimmed = model.trim_end();
    let marker = b"[1m]";
    let bytes = trimmed.as_bytes();
    if bytes.len() >= marker.len()
        && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
    {
        return trimmed[..trimmed.len() - marker.len()].trim_end();
    }
    model
}

pub fn has_one_m_marker(model: &str) -> bool {
    model.trim_end().to_ascii_lowercase().ends_with("[1m]")
}

const POINT_RELEASE_CLIENT_IDS: &[(&str, &str, &str, &str)] = &[
    (PROXY_OPUS_ALIAS, "opus-5", "5", "claude-opus-5-5"),
    (PROXY_SONNET_ALIAS, "sonnet-5", "5", "claude-sonnet-5-5"),
    (PROXY_FABLE_ALIAS, "fable-5", "2", "claude-fable-5-2"),
    (PROXY_FABLE_ALIAS, "fable-5", "1", "claude-fable-5-1"),
];

pub fn point_release_client_id(family_alias: &str, upstream: &str) -> Option<&'static str> {
    let id = strip_one_m_suffix_for_upstream(upstream)
        .trim()
        .to_ascii_lowercase();
    POINT_RELEASE_CLIENT_IDS
        .iter()
        .filter(|(alias, _, _, _)| *alias == family_alias)
        .find(|(_, stem, minor, _)| {
            ['.', '-'].iter().any(|sep| {
                let needle = format!("{stem}{sep}{minor}");
                id.find(&needle).is_some_and(|at| {
                    let rest = &id[at + needle.len()..];
                    !rest.starts_with(|c: char| c.is_ascii_digit() || c == '.')
                })
            })
        })
        .map(|(_, _, _, client)| *client)
}

/// 子代理模型原样写给客户端；上游是小版本时同样换成目录 id，否则 Claude Code 不认识它。
pub fn client_model_for_subagent(upstream: &str) -> String {
    let Some(alias) = [PROXY_OPUS_ALIAS, PROXY_SONNET_ALIAS, PROXY_FABLE_ALIAS]
        .into_iter()
        .find_map(|family| point_release_client_id(family, upstream))
    else {
        return upstream.to_string();
    };
    let mut client_model = alias.to_string();
    if has_one_m_marker(upstream) {
        client_model.push_str(ONE_M_MARKER_FOR_CLIENT);
    }
    client_model
}

/// Codex Stack 模型的说明：`<上游模型名> · <窗口>`。显示名可以是用户自己起的，id 又带着
/// 前缀，上游真正的模型名只有这里看得到；供应商名已经在显示名里，不再重复。窗口未知
/// （`0`）时只写模型名。
pub fn model_description(model: &str, window: u64) -> String {
    match window_label(window) {
        Some(window) => format!("{model} · {window}"),
        None => model.to_string(),
    }
}

/// Claude Code、Claude Desktop 里模型的说明：`<供应商名> · <上游模型名> · <窗口>`。路由那家的
/// 档位、聚合模式的模型、「打开终端」的菜单都用它。供应商名为空时同 [`model_description`]。
pub fn provider_model_description(provider_name: &str, model: &str, window: u64) -> String {
    let description = model_description(model, window);
    match provider_name.trim() {
        "" => description,
        name => format!("{name} · {description}"),
    }
}

/// 窗口的简写：整百万写 `1M`，其余按千写 `256K`，不足一千照原样。
fn window_label(window: u64) -> Option<String> {
    match window {
        0 => None,
        w if w % 1_000_000 == 0 => Some(format!("{}M", w / 1_000_000)),
        w if w >= 1_000 => Some(format!("{}K", (w + 500) / 1_000)),
        w => Some(w.to_string()),
    }
}
