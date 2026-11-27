//! 聚合与故障转移：同名模型发布为 `group/<model>`，独有模型为 `<account>/<model>`。
//! 公共模型只在提供该模型的成员之间轮询、故障转移；不改默认账号或客户端配置。
//! 名单和永久账号 key 存在 `live-state.json`。旧 `ccs-*` id 仍固定到原账号。

mod pools;
pub use pools::{account_namespace, group_codex_catalog, model_pools};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::live::engine::DeviceStore;
use crate::live::project::claude::{env_string, has_one_m_marker, ONE_M_MARKER_FOR_CLIENT};
use crate::model_names::strip_one_m_suffix_for_upstream;
use crate::provider::{ClaudeStackModel, Provider};
use crate::services::provider::codex_client_catalog::StaleClients;

use super::state::{self, StackState};

/// Claude Code 的 Stack 模型 id：`ccs-claude-<key>--<model>`。id 里要有 `claude` 才进
/// `/model` 选择器，不以 `claude-` 开头 MAX 窗口才生效。
const CLAUDE_PREFIX: &str = "ccs-claude-";
const CLAUDE_SEPARATOR: &str = "--";
/// Codex 的 Stack 模型 id：`ccs-<key>/<model>`。
const CODEX_PREFIX: &str = "ccs-";
const CODEX_SEPARATOR: char = '/';
/// key 的最大长度：只是为了模型 id 不至于太长。
const KEY_MAX_LEN: usize = 24;

/// Claude Code 在没有 `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 时按这个窗口算。
pub const CLAUDE_DEFAULT_WINDOW: u64 = 200_000;

/// 支持 Stack 模型的应用。
pub fn supports_stack(app: &AppType) -> bool {
    app.supports_stack()
}

/// 给 `provider` 一个 key：登记过就用原来的（移除后重新加入，旧会话里的 id 继续有效），
/// 没有就按图标、名称生成一个新的写进登记簿。
///
/// 新 key 和登记簿里所有的 key 去重，不只是当前成员：已移除、已删除的供应商的 key 也
/// 占着位置，旧 id 才不会被发给新来的这家。
pub fn allocate_key(stack: &mut StackState, provider: &Provider) -> String {
    if let Some(key) = stack.key_of(&provider.id) {
        return key.to_string();
    }
    // 自定义图标是一长串 base64，拿来生成 key 没有意义，只用内置图标名
    let icon = provider
        .icon
        .as_deref()
        .filter(|icon| !crate::services::provider::icon::is_custom_icon(icon));
    let base = [icon, Some(provider.name.as_str())]
        .into_iter()
        .flatten()
        .map(slug)
        .find(|candidate| !candidate.is_empty())
        .unwrap_or_else(|| {
            let id: String = slug(&provider.id)
                .chars()
                .filter(|c| *c != '-')
                .take(6)
                .collect();
            format!("p{id}")
        });
    let mut key = base.clone();
    let mut suffix = 2;
    while stack.keys.contains_key(&key) {
        key = format!("{base}-{suffix}");
        suffix += 1;
    }
    stack.keys.insert(key.clone(), provider.id.clone());
    key
}

/// 小写 ASCII，只保留 `[a-z0-9-]`，其余字符换成 `-`，连续的 `-` 合并，首尾的 `-` 去掉。
/// 结果里不会有 `--`（Claude id 的分隔符）和 `/`（Codex id 的分隔符）。
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= KEY_MAX_LEN {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Stack 模型 id。Claude 的上游是 1M 窗口时末尾带 `[1m]`，Claude Code 才按 1M 计算；Claude
/// Desktop 的 1M 写在模型条目里（`supports1m` + `prefer1m`），id 不带标记。
pub fn encode(app: &AppType, key: &str, model: &str, one_m: bool) -> String {
    match app {
        AppType::Codex => format!("{CODEX_PREFIX}{key}{CODEX_SEPARATOR}{model}"),

        _ => {
            let marker = if one_m { ONE_M_MARKER_FOR_CLIENT } else { "" };
            format!("{CLAUDE_PREFIX}{key}{CLAUDE_SEPARATOR}{model}{marker}")
        }
    }
}

/// 解码的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded<'a> {
    /// 不带保留前缀：普通模型名，照旧走代理路由。
    Plain,
    /// 带保留前缀，但切不出 key 和模型。
    Malformed,
    /// 带保留前缀。`model` 可以含 `/` 和 `--`（key 里没有这两种分隔符，在第一个处切开）。
    Stack {
        key: &'a str,
        model: &'a str,
        /// Claude id 末尾带着 1M 标记。
        one_m: bool,
    },
}

/// 按客户端的格式解码模型 id。Claude：`ccs-claude-` 开头；Codex：`ccs-` 开头且含 `/`。
pub fn decode<'a>(app: &AppType, id: &'a str) -> Decoded<'a> {
    let (rest, separator, one_m) = match app {
        AppType::Claude => {
            let Some(rest) = id.strip_prefix(CLAUDE_PREFIX) else {
                return Decoded::Plain;
            };
            let stripped = strip_one_m_suffix_for_upstream(rest);
            (stripped, CLAUDE_SEPARATOR, stripped.len() != rest.len())
        }
        AppType::Codex => {
            let Some(rest) = id.strip_prefix(CODEX_PREFIX) else {
                return Decoded::Plain;
            };
            if !rest.contains(CODEX_SEPARATOR) {
                return Decoded::Plain;
            }
            (rest, "/", false)
        }

        _ => return Decoded::Plain,
    };
    match rest.split_once(separator) {
        Some((key, model)) if !key.is_empty() && !model.is_empty() => {
            Decoded::Stack { key, model, one_m }
        }
        _ => Decoded::Malformed,
    }
}

/// Claude Stack 供应商发布给客户端的一个模型（Codex 的由目录条目描述，见
/// `codex_config::plan_codex_stack_catalog`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackModel {
    /// 发布给客户端的 id（带保留前缀）。
    pub id: String,
    /// 发往上游的模型名：行里配置的原值（可能带 `[1M]`，转发时和路由请求一样处理）。
    pub upstream: String,
    /// 模型自己的显示名（行里配的，没有是模型名），不带供应商名。
    pub name: String,
    /// 选择器里显示的名字：`<显示名>（<供应商名>）`。
    pub display_name: String,
    /// 选择器里的说明。
    pub description: String,
    /// 上游是 1M 窗口（id 带 `[1m]`）。
    pub one_m: bool,
    /// 非 1M 模型的窗口：行里的 `CLAUDE_CODE_MAX_CONTEXT_TOKENS`，没有是 200K。
    pub window: u64,
}

/// Claude 行发布的模型：配了 Stack 模型列表（`meta.stackModels`）就是列表，清空了就什么都不
/// 发布；没配时是模型映射，即 `ANTHROPIC_MODEL` 和各档 `ANTHROPIC_DEFAULT_*_MODEL`，显示名取
/// 对应档位的 `*_MODEL_NAME`。按去掉 1M 标记后的名字去重（任何一处带标记就按 1M）。
pub fn claude_models(key: &str, provider: &Provider) -> Vec<StackModel> {
    let listed = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.stack_models.as_deref());
    let found = match listed {
        Some(list) => listed_models(list),
        None => claude_env(provider).map(mapped_models).unwrap_or_default(),
    };
    stack_models(key, provider, found)
}

/// 行里要发布的一个模型（还没加前缀）。
struct Found {
    /// 去掉 1M 标记的模型名。
    model: String,
    /// 发往上游的原值（1M 模型带标记）。
    upstream: String,
    name: Option<String>,
    one_m: bool,
}

/// 按去掉 1M 标记后的名字去重地加入 `found`。同一个模型有一处带 1M 标记就按 1M，发往上游的
/// 也用带标记的那个写法；显示名取第一个有的。
fn push_found(found: &mut Vec<Found>, upstream: &str, name: Option<String>) {
    let model = strip_one_m_suffix_for_upstream(upstream).trim().to_string();
    if model.is_empty() {
        return;
    }
    let one_m = has_one_m_marker(upstream);
    match found.iter_mut().find(|entry| entry.model == model) {
        Some(entry) => {
            if one_m && !entry.one_m {
                entry.upstream = upstream.to_string();
                entry.one_m = true;
            }
            if entry.name.is_none() {
                entry.name = name;
            }
        }
        None => found.push(Found {
            model,
            upstream: upstream.to_string(),
            name,
            one_m,
        }),
    }
}

fn claude_env(provider: &Provider) -> Option<&Map<String, Value>> {
    provider
        .settings_config
        .get("env")
        .and_then(Value::as_object)
}

/// Stack 模型列表（`meta.stackModels`）里的模型。
fn listed_models(list: &[ClaudeStackModel]) -> Vec<Found> {
    let mut found = Vec::new();
    for entry in list {
        let raw = entry.model.trim();
        let model = strip_one_m_suffix_for_upstream(raw).trim();
        let upstream = if entry.one_m || has_one_m_marker(raw) {
            format!("{model}{ONE_M_MARKER_FOR_CLIENT}")
        } else {
            model.to_string()
        };
        let name = entry
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        push_found(&mut found, &upstream, name);
    }
    found
}

/// 模型映射里的模型：`ANTHROPIC_MODEL` 和各档 `ANTHROPIC_DEFAULT_*_MODEL`。
fn mapped_models(env: &Map<String, Value>) -> Vec<Found> {
    const ROLES: [(&str, Option<&str>); 5] = [
        ("ANTHROPIC_MODEL", None),
        (
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            Some("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"),
        ),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            Some("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"),
        ),
        (
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            Some("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"),
        ),
        (
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            Some("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME"),
        ),
    ];
    let mut found = Vec::new();
    for (model_key, name_key) in ROLES {
        let Some(upstream) = env_string(env, model_key) else {
            continue;
        };
        let name = name_key.and_then(|name_key| env_string(env, name_key).map(str::to_string));
        push_found(&mut found, upstream, name);
    }
    found
}

/// 给找到的模型加上前缀、显示名和窗口。非 1M 模型的窗口是行里的
/// `CLAUDE_CODE_MAX_CONTEXT_TOKENS`（Claude Code 只有一个全局窗口，没法按模型设），没有是 200K。
fn stack_models(key: &str, provider: &Provider, found: Vec<Found>) -> Vec<StackModel> {
    let window = claude_env(provider)
        .and_then(|env| env.get("CLAUDE_CODE_MAX_CONTEXT_TOKENS"))
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        })
        .filter(|window| *window > 0)
        .unwrap_or(CLAUDE_DEFAULT_WINDOW);
    found
        .into_iter()
        .map(|found| {
            let name = found.name.unwrap_or_else(|| found.model.clone());
            let shown_window = if found.one_m { 1_000_000 } else { window };
            StackModel {
                id: encode(&AppType::Claude, key, &found.model, found.one_m),
                display_name: display_name(&name, &provider.name),
                name,
                description: provider_model_description(&provider.name, &found.model, shown_window),
                upstream: found.upstream,
                one_m: found.one_m,
                window,
            }
        })
        .collect()
}

/// Codex 行发布的模型 id：行里的模型目录，没有配置目录时只有行的 `model`。显示名和窗口
/// 由目录条目决定（`codex_config::plan_codex_stack_catalog`）。
pub fn codex_model_ids(key: &str, provider: &Provider) -> Vec<String> {
    let config = provider
        .settings_config
        .get("config")
        .and_then(Value::as_str)
        .unwrap_or("");
    crate::codex_config::codex_published_models(&provider.settings_config, config)
        .into_iter()
        .map(|model| encode(&AppType::Codex, key, &model, false))
        .collect()
}

/// Claude Desktop Stack 供应商发布的一个模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopStackModel {
    /// 发布给 Desktop 的 id（`claude-fable-ccs-<数字>`）。
    pub id: String,
    /// 发往上游的模型名：行里映射表的目标模型。
    pub upstream: String,
    /// 选择器里显示的名字（`labelOverride`）：`<供应商名> / <模型显示名>`。
    pub label: String,
    /// 供应商名（模型说明里用）。
    pub provider_name: String,
    /// 映射表里这个模型标了 1M。
    pub supports_1m: bool,
}

/// 选择器里 Stack 模型的显示名：`<模型显示名>（<供应商名>）`。
pub fn display_name(model: &str, provider_name: &str) -> String {
    format!("{model}（{provider_name}）")
}

pub use crate::model_names::{model_description, provider_model_description};

/// 账号模型的内部标识，沿用旧编码供解析和组装目录。Claude Code 和 Codex
/// 的默认第三方账号也参与；公开名称由模型池统一生成。
fn model_ids_of(app: &AppType, key: &str, provider: &Provider) -> Vec<String> {
    match app {
        AppType::Claude => claude_models(key, provider)
            .into_iter()
            .map(|model| model.id)
            .collect(),
        AppType::Codex if !crate::services::provider::codex_direct::is_official(provider) => {
            codex_model_ids(key, provider)
        }

        _ => Vec::new(),
    }
}

/// 名单里的一家。
#[derive(Debug, Clone)]
pub struct Member {
    pub provider: Provider,
    pub key: String,
    /// 这家是路由那家（默认），见 [`is_published`]。
    pub route: bool,
    /// 发布给客户端的模型 id。
    pub model_ids: Vec<String>,
}

/// 名单里还在库里的成员，按加入顺序。库里已经没有的跳过（删除供应商会先把它移出名单，
/// 删行前失败才会留下）。`route` 是代理模式下的路由供应商（不在代理模式时为 `None`）。
pub fn members(
    db: &Database,
    app: &AppType,
    stack: &StackState,
    route: Option<&str>,
) -> Result<Vec<Member>, AppError> {
    let mut members = Vec::with_capacity(stack.members.len());
    for id in &stack.members {
        let Some(key) = stack.key_of(id) else {
            log::warn!("{} 的 Stack 模型成员 {id} 没有登记 key，跳过", app.as_str());
            continue;
        };
        let Some(provider) = db.get_provider_by_id(id, app)? else {
            continue;
        };
        let route = route == Some(id.as_str());
        let model_ids = model_ids_of(app, key, &provider);
        members.push(Member {
            key: key.to_string(),
            provider,
            route,
            model_ids,
        });
    }
    Ok(members)
}

/// 有模型的默认账号也参与公共模型池；官方原生目录继续走专用路径。
pub fn is_published(member: &Member) -> bool {
    !member.route || !member.model_ids.is_empty()
}

/// 发布 Stack 模型的成员（按名单顺序，见 [`is_published`]）。Stack 模式关着（路由模式）时
/// 没有：名单留着，下次进入 Stack 模式时恢复。
pub fn published_members(
    db: &Database,
    app: &AppType,
    stack: &StackState,
    route: Option<&str>,
) -> Result<Vec<Member>, AppError> {
    if !stack.enabled || stack.members.is_empty() || !supports_stack(app) {
        return Ok(Vec::new());
    }
    let mut members = members(db, app, stack, route)?;
    members.retain(is_published);
    Ok(members)
}

/// Claude 的这些成员发布给客户端的模型，按名单顺序。
pub fn claude_published(members: &[Member]) -> Vec<StackModel> {
    model_pools(&AppType::Claude, members)
        .into_iter()
        .map(|pool| StackModel {
            id: pool.id.clone(),
            upstream: if pool.one_m {
                format!("{}{ONE_M_MARKER_FOR_CLIENT}", pool.model)
            } else {
                pool.model.clone()
            },
            name: pool.id.clone(),
            display_name: pool.id,
            description: model_description(&pool.model, pool.window),
            one_m: pool.one_m,
            window: pool.window,
        })
        .collect()
}

/// 默认那家（路由）列表里的第一个模型：Stack 模式下 Claude Code 的四档别名（启动默认、后台
/// 任务、子代理别名）都指向它。列表的顺序就是模型映射的顺序（`ANTHROPIC_MODEL` 在前），
/// 所以没配列表的行用的是它的主模型。路由那家不在发布的成员里（Stack 模式关着、它没有模型）
/// 时没有。
pub fn claude_route_default(members: &[Member]) -> Option<StackModel> {
    let route = members.iter().find(|member| member.route)?;
    let first = claude_models(&route.key, &route.provider)
        .into_iter()
        .next()?;
    claude_published(members).into_iter().find(|model| {
        strip_one_m_suffix_for_upstream(&model.upstream)
            == strip_one_m_suffix_for_upstream(&first.upstream)
    })
}

/// 这个应用在 Stack 模式（代理模式且 Stack 模式开着）。读不出状态按不在处理。
pub fn stack_mode_now(app: &AppType) -> bool {
    if !supports_stack(app) {
        return false;
    }
    state::stack_mode(&DeviceStore::for_device(), app.as_str()).unwrap_or_else(|error| {
        log::warn!(
            "读取 {} 的 Stack 模式失败，按不在处理: {error}",
            app.as_str()
        );
        false
    })
}

/// 在 Stack 名单里（不管什么模式）。
pub fn is_member(app: &AppType, provider_id: &str) -> Result<bool, AppError> {
    if !supports_stack(app) {
        return Ok(false);
    }
    Ok(state::stack(&DeviceStore::for_device(), app.as_str())?.is_member(provider_id))
}

/// Claude Code 现在发布的 Stack 模型：代理模式下按已落定的名单和路由算，不在代理模式时没有。
pub fn claude_published_now(db: &Database) -> Result<Vec<StackModel>, AppError> {
    let app = AppType::Claude;
    let store = DeviceStore::for_device();
    let mode = state::mode_state(&store, app.as_str())?;
    if !mode.is_proxy() {
        return Ok(Vec::new());
    }
    let stack = state::stack(&store, app.as_str())?;
    Ok(claude_published(&published_members(
        db,
        &app,
        &stack,
        mode.proxy_route.as_deref(),
    )?))
}

/// 选中 Stack 模型的请求要发往的那一家。
#[derive(Debug, Clone)]
pub struct StackTarget {
    pub provider: Provider,
    /// 发往上游的模型名。
    pub upstream_model: String,
    /// 客户端发来的带前缀 id（只用于日志展示）。
    pub original_model: String,
    /// 公共模型的候选账号（含 provider）；空表示固定到一个账号的请求。
    pub candidates: Vec<Provider>,
}

/// 带保留前缀的 id 为什么解不出来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackMiss {
    /// 这个应用不在 Stack 模式（路由模式下名单留着，但不发布、也不转发）。
    StackOff,
    /// key 没登记，或 id 切不出 key 和模型。
    Unknown,
    /// 这家已经从 Stack 名单移除。
    Removed,
    /// 这家已经从 CC Switch 删除。
    Deleted,
}

impl StackMiss {
    /// 返回给客户端的错误文案。
    pub fn message(&self, model: &str) -> String {
        match self {
            Self::StackOff => format!(
                "聚合的模型 {model} 只能在聚合模式下使用，当前没有开启聚合模式，请在模型列表里重新选择 (Aggregated model {model} only works in Stack mode, which is off; pick a model from the model list again)"
            ),
            Self::Unknown => format!(
                "聚合的模型 {model} 在 CC Switch 里不存在，请在模型列表里重新选择 (Aggregated model {model} is unknown to CC Switch; pick a model from the model list again)"
            ),
            Self::Removed => format!(
                "聚合的模型 {model} 已从 CC Switch 移除，请在模型列表里重新选择 (Aggregated model {model} was removed from CC Switch; pick a model from the model list again)"
            ),
            Self::Deleted => format!(
                "聚合的模型 {model} 对应的供应商已删除，请在模型列表里重新选择 (The provider of aggregated model {model} was deleted; pick a model from the model list again)"
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Resolved {
    /// 普通模型名，照旧走代理路由。
    Plain,
    Hit(Box<StackTarget>),
    /// 带保留前缀但解不出来：报错，不回落到默认路由。名单为空时也一样。
    Miss(StackMiss),
}

/// 解析请求里的模型 id。不带保留前缀时不读任何状态，路由请求的路径不变。带前缀的只在
/// Stack 模式下解析：路由模式下名单留着，客户端手里旧的 Stack id 也不能转给名单里的那家。
pub fn resolve(
    db: &Database,
    store: &DeviceStore,
    app: &AppType,
    model: &str,
) -> Result<Resolved, AppError> {
    let (key, model_part, one_m) = match decode(app, model) {
        Decoded::Plain => return resolve_public(db, store, app, model),
        Decoded::Malformed => return Ok(Resolved::Miss(StackMiss::Unknown)),
        Decoded::Stack { key, model, one_m } => (key, model, one_m),
    };
    let view = state::stack_view(store, app.as_str())?;
    if !view.enabled {
        return Ok(Resolved::Miss(StackMiss::StackOff));
    }
    let stack = view.stack;
    let Some(provider_id) = stack.keys.get(key) else {
        return Ok(Resolved::Miss(StackMiss::Unknown));
    };
    let provider = db.get_provider_by_id(provider_id, app)?;
    let Some(provider) = provider else {
        return Ok(Resolved::Miss(StackMiss::Deleted));
    };
    if !stack.is_member(provider_id) {
        return Ok(Resolved::Miss(StackMiss::Removed));
    }
    // Claude 发往上游的是行里配置的原值（可能带 1M 标记），和路由请求映射出来的一样；
    // 行里已经没有这个模型时照原样发（上游自己决定认不认）。Codex 的 id 就是行里的模型名。
    let upstream_model = match app {
        AppType::Claude => claude_models(key, &provider)
            .into_iter()
            .find(|published| {
                strip_one_m_suffix_for_upstream(&published.upstream).trim() == model_part
            })
            .map(|published| published.upstream),
        _ => None,
    }
    .unwrap_or_else(|| {
        if one_m {
            format!("{model_part}{ONE_M_MARKER_FOR_CLIENT}")
        } else {
            model_part.to_string()
        }
    });
    Ok(Resolved::Hit(Box::new(StackTarget {
        provider,
        upstream_model,
        original_model: model.to_string(),
        candidates: Vec::new(),
    })))
}

fn resolve_public(
    db: &Database,
    store: &DeviceStore,
    app: &AppType,
    model: &str,
) -> Result<Resolved, AppError> {
    if !matches!(app, AppType::Claude | AppType::Codex) {
        return Ok(Resolved::Plain);
    }
    let Some((namespace, part)) = model.split_once('/') else {
        return Ok(Resolved::Plain);
    };
    let view = state::stack_view(store, app.as_str())?;
    let group = namespace == "group";
    let account = view
        .stack
        .keys
        .iter()
        .find(|(key, _)| account_namespace(key) == namespace);
    if !group && account.is_none() {
        return Ok(Resolved::Plain);
    }
    if !view.enabled {
        return Ok(Resolved::Miss(StackMiss::StackOff));
    }
    if part.is_empty() {
        return Ok(Resolved::Miss(StackMiss::Unknown));
    }
    let requested = if *app == AppType::Claude {
        strip_one_m_suffix_for_upstream(part)
    } else {
        part
    };
    let members = members(db, app, &view.stack, view.mode_state.proxy_route.as_deref())?;
    let pool = model_pools(app, &members)
        .into_iter()
        .find(|pool| pool.model == requested);
    if group {
        // A cached group id remains usable after all but one member is removed.
        let Some(pool) = pool else {
            return Ok(Resolved::Miss(StackMiss::Unknown));
        };
        let candidates: Vec<_> = pool
            .members
            .iter()
            .map(|&index| members[index].provider.clone())
            .collect();
        return Ok(Resolved::Hit(Box::new(StackTarget {
            provider: candidates[0].clone(),
            upstream_model: if *app == AppType::Claude && pool.one_m && has_one_m_marker(part) {
                format!("{requested}{ONE_M_MARKER_FOR_CLIENT}")
            } else {
                requested.to_string()
            },
            original_model: model.to_string(),
            candidates,
        })));
    }
    let (_, provider_id) = account.expect("known account namespace");
    let Some(provider) = db.get_provider_by_id(provider_id, app)? else {
        return Ok(Resolved::Miss(StackMiss::Deleted));
    };
    if !view.stack.is_member(provider_id) {
        return Ok(Resolved::Miss(StackMiss::Removed));
    }
    if !pool.is_some_and(|pool| {
        pool.members
            .iter()
            .any(|&index| members[index].provider.id == *provider_id)
    }) {
        return Ok(Resolved::Miss(StackMiss::Unknown));
    }
    Ok(Resolved::Hit(Box::new(StackTarget {
        provider,
        upstream_model: part.to_string(),
        original_model: model.to_string(),
        candidates: Vec::new(),
    })))
}

/// 给前端：名单里的一家。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackMemberView {
    pub provider_id: String,
    /// 发布给客户端的模型 id。
    pub model_ids: Vec<String>,
    /// 这家是默认那家（代理路由）。Claude 的照常发布，第一个模型同时占着四档别名；Codex 的
    /// 默认账号；Claude Code 和 Codex 的默认账号也参与模型池。
    pub route: bool,
    /// 账号自身提供的模型数量；合并后的公共模型可能由多家共同提供。
    pub member_models: usize,
}

/// 给前端：Codex 聚合面板「子代理模型」下拉的一项（见 `codex_direct::stack_subagent_choices`）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackSubagentModel {
    /// 发布给客户端的模型 id（`group/<model>` 或 `<account>/<model>`）。
    pub id: String,
    /// Previous per-account ids accepted for saved subagent/reviewer settings.
    pub aliases: Vec<String>,
    pub provider_id: String,
    /// 和模型选择器一致，公共模型不附带来源账号。
    pub display_name: String,
    /// 这个模型支持的推理强度，按目录里的顺序。
    pub efforts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
}

/// 给前端：Stack 模式的状态、名单和提示。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackView {
    /// 在 Stack 模式（代理模式且 Stack 模式开着）。
    pub active: bool,
    pub members: Vec<StackMemberView>,
    /// Codex Stack 模型客户端看不到或看不全：`routeOwnsCatalog` 路由那家自己管理模型目录
    /// 文件，Stack 模型不发布；官方做路由时官方模型列表暂未取到：`officialModelsBundled`
    /// 暂用 Codex 自带的列表（可能缺账号专属的模型），`officialModelsUnavailable` Stack 模型
    /// 暂不可用；`officialModelsOutdated` 本机 Codex 太旧，拉到的官方列表里没有能选的模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<&'static str>,
    /// `notice` 是 `officialModelsBundled` / `officialModelsUnavailable` 时，没取到列表的原因
    /// 码：先是官方列表的（`noLogin`、`loginStale`、`loginIncomplete`、`noVersion`、
    /// `fetchFailed`、`invalidList`），不可用时再跟自带列表的（`noCli`、`cliFailed`、
    /// `nothingUsable`）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notice_reasons: Vec<&'static str>,
    /// Codex 客户端还在用旧的模型列表（启动时读的目录），Stack 模型看不到，要重启才行。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_clients: Option<StaleClients>,
    /// `stale_clients` 对应的目录和登录（见 `codex_client_catalog::revision`）：用户关掉提示后，
    /// 同一个供应商上目录又改了，这个值变了，前端重新显示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_revision: Option<String>,
    /// Codex：聚合面板「子代理模型」下拉的选项，发布 Stack 模型的成员按名单顺序。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subagent_models: Vec<StackSubagentModel>,
}

pub fn member_views(app: &AppType, members: &[Member]) -> Vec<StackMemberView> {
    let pools = model_pools(app, members);
    members
        .iter()
        .enumerate()
        .map(|(index, member)| StackMemberView {
            provider_id: member.provider.id.clone(),
            model_ids: if matches!(app, AppType::Claude | AppType::Codex) {
                pools
                    .iter()
                    .filter(|pool| pool.members.contains(&index))
                    .map(|pool| pool.id.clone())
                    .collect()
            } else {
                member.model_ids.clone()
            },
            route: member.route,
            member_models: if member.route {
                model_ids_of(app, &member.key, &member.provider).len()
            } else {
                member.model_ids.len()
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(id: &str, name: &str, icon: Option<&str>, env: Value) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            name.to_string(),
            json!({ "env": env }),
            None,
        );
        provider.icon = icon.map(str::to_string);
        provider
    }

    #[test]
    fn keys_come_from_the_icon_then_the_name() {
        let mut stack = StackState::default();
        let kimi = provider("a", "Kimi For Coding", Some("kimi"), json!({}));
        let named = provider("b", "My Relay 2.0!", None, json!({}));
        let chinese = provider("c1b2c3d4-e5f6", "智谱", None, json!({}));
        let blank_icon = provider("d", "DeepSeek", Some("  "), json!({}));
        let custom_icon = provider(
            "e",
            "Acme",
            Some("data:image/png;base64,iVBORw0KGgo="),
            json!({}),
        );

        assert_eq!(allocate_key(&mut stack, &kimi), "kimi");
        assert_eq!(allocate_key(&mut stack, &named), "my-relay-2-0");
        assert_eq!(allocate_key(&mut stack, &chinese), "pc1b2c3");
        assert_eq!(allocate_key(&mut stack, &blank_icon), "deepseek");
        assert_eq!(allocate_key(&mut stack, &custom_icon), "acme");
    }

    #[test]
    fn keys_never_contain_the_separators_and_stay_short() {
        for text in ["a--b", "a/b", "--edge--", "UPPER__lower", &"x".repeat(80)] {
            let key = slug(text);
            assert!(!key.contains("--") && !key.contains('/'), "{text} → {key}");
            assert!(
                !key.starts_with('-') && !key.ends_with('-'),
                "{text} → {key}"
            );
            assert!(key.len() <= KEY_MAX_LEN, "{text} → {key}");
        }
        assert_eq!(slug("UPPER__lower"), "upper-lower");
    }

    #[test]
    fn a_key_stays_with_its_provider_forever() {
        let mut stack = StackState::default();
        let a = provider("a", "Kimi", Some("kimi"), json!({}));
        let b = provider("b", "Kimi Coding Plan", Some("kimi"), json!({}));

        assert_eq!(allocate_key(&mut stack, &a), "kimi");
        stack.members.push("a".to_string());
        // A 移除（登记簿保留）后，同图标的 B 拿不到 A 的 key。
        stack.members.clear();
        assert_eq!(allocate_key(&mut stack, &b), "kimi-2");
        // A 重新加入，拿回原来的 key。
        assert_eq!(allocate_key(&mut stack, &a), "kimi");
        assert_eq!(stack.keys.len(), 2);
    }

    #[test]
    fn claude_ids_round_trip() {
        let claude = AppType::Claude;
        for (model, one_m) in [
            ("kimi-k3", false),
            ("glm-5.2", true),
            ("vendor/model", false),
            ("model--with--dashes", true),
        ] {
            let id = encode(&claude, "kimi", model, one_m);
            assert_eq!(
                decode(&claude, &id),
                Decoded::Stack {
                    key: "kimi",
                    model,
                    one_m
                },
                "{id}"
            );
        }
        // 标记大小写不敏感。
        assert_eq!(
            decode(&claude, "ccs-claude-k--m[1m]"),
            Decoded::Stack {
                key: "k",
                model: "m",
                one_m: true
            }
        );
    }

    #[test]
    fn codex_ids_round_trip() {
        let codex = AppType::Codex;
        let id = encode(&codex, "deepseek", "deepseek/deepseek-v4-pro", false);
        assert_eq!(id, "ccs-deepseek/deepseek/deepseek-v4-pro");
        assert_eq!(
            decode(&codex, &id),
            Decoded::Stack {
                key: "deepseek",
                model: "deepseek/deepseek-v4-pro",
                one_m: false
            }
        );
    }

    #[test]
    fn reserved_ids_that_do_not_split_are_malformed() {
        let (claude, codex) = (AppType::Claude, AppType::Codex);
        assert_eq!(decode(&claude, "ccs-claude-kimi"), Decoded::Malformed);
        assert_eq!(decode(&claude, "ccs-claude---m"), Decoded::Malformed);
        assert_eq!(decode(&claude, "ccs-claude-k--"), Decoded::Malformed);
        assert_eq!(decode(&codex, "ccs-/m"), Decoded::Malformed);
        assert_eq!(decode(&codex, "ccs-k/"), Decoded::Malformed);
    }

    #[test]
    fn claude_rows_publish_each_model_once() {
        let row = provider(
            "p",
            "Zhipu",
            None,
            json!({
                "ANTHROPIC_MODEL": "glm-5.2",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.2[1M]",
                "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME": "GLM 5.2",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5.2",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-4.7-air",
                "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "128000"
            }),
        );
        let models = claude_models("zhipu", &row);
        assert_eq!(
            models,
            vec![
                StackModel {
                    id: "ccs-claude-zhipu--glm-5.2[1m]".to_string(),
                    upstream: "glm-5.2[1M]".to_string(),
                    name: "GLM 5.2".to_string(),
                    display_name: "GLM 5.2（Zhipu）".to_string(),
                    description: "Zhipu · glm-5.2 · 1M".to_string(),
                    one_m: true,
                    window: 128_000,
                },
                StackModel {
                    id: "ccs-claude-zhipu--glm-4.7-air".to_string(),
                    upstream: "glm-4.7-air".to_string(),
                    name: "glm-4.7-air".to_string(),
                    display_name: "glm-4.7-air（Zhipu）".to_string(),
                    description: "Zhipu · glm-4.7-air · 128K".to_string(),
                    one_m: false,
                    window: 128_000,
                },
            ]
        );
        let bare = provider("q", "Bare", None, json!({ "ANTHROPIC_AUTH_TOKEN": "sk" }));
        assert!(claude_models("bare", &bare).is_empty());
        let default_window = provider("r", "R", None, json!({ "ANTHROPIC_MODEL": "m" }));
        assert_eq!(
            claude_models("r", &default_window)[0].window,
            CLAUDE_DEFAULT_WINDOW
        );
    }

    #[test]
    fn the_description_names_the_upstream_model_and_its_window() {
        assert_eq!(model_description("kimi-k3", 256_000), "kimi-k3 · 256K");
        assert_eq!(model_description("kimi-k3", 262_144), "kimi-k3 · 262K");
        assert_eq!(model_description("glm-5.2", 1_000_000), "glm-5.2 · 1M");
        assert_eq!(model_description("glm-5.2", 1_050_000), "glm-5.2 · 1050K");
        assert_eq!(model_description("m", 0), "m");
        // Claude Code、Claude Desktop 的说明带供应商名。
        assert_eq!(
            provider_model_description("Kimi", "kimi-k3", 256_000),
            "Kimi · kimi-k3 · 256K"
        );
        assert_eq!(provider_model_description(" ", "m", 0), "m");
    }

    fn with_stack_models(mut provider: Provider, models: Value) -> Provider {
        provider.meta = Some(crate::provider::ProviderMeta {
            stack_models: serde_json::from_value(models).unwrap(),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn an_emptied_list_publishes_nothing_and_an_unset_one_follows_the_mapping() {
        let mapped = provider("p", "Kimi", None, json!({ "ANTHROPIC_MODEL": "kimi-k3" }));
        let emptied = with_stack_models(mapped.clone(), json!([]));
        assert!(claude_models("kimi", &emptied).is_empty());
        assert_eq!(
            serde_json::to_value(emptied.meta.as_ref().unwrap()).unwrap()["stackModels"],
            json!([])
        );

        let unset = with_stack_models(mapped, Value::Null);
        assert_eq!(unset.meta.as_ref().unwrap().stack_models, None);
        let ids: Vec<String> = claude_models("kimi", &unset)
            .into_iter()
            .map(|model| model.id)
            .collect();
        assert_eq!(ids, vec!["ccs-claude-kimi--kimi-k3"]);
        let meta = serde_json::to_value(unset.meta.as_ref().unwrap()).unwrap();
        assert!(meta.get("stackModels").is_none());
    }

    #[test]
    fn a_stack_model_list_replaces_the_mapping() {
        let row = with_stack_models(
            provider(
                "p",
                "Kimi",
                None,
                json!({
                    "ANTHROPIC_MODEL": "kimi-k3",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "kimi-k3-turbo",
                    "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "256000"
                }),
            ),
            json!([
                { "model": " kimi-k3 ", "displayName": "Kimi K3" },
                { "model": "kimi-k3-thinking", "displayName": " ", "oneM": true },
                { "model": "  " },
                { "model": "kimi-k3[1m]", "displayName": "ignored" }
            ]),
        );
        let summary: Vec<(String, String, String, bool, u64)> = claude_models("kimi", &row)
            .into_iter()
            .map(|model| {
                (
                    model.id,
                    model.upstream,
                    model.display_name,
                    model.one_m,
                    model.window,
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "ccs-claude-kimi--kimi-k3[1m]".to_string(),
                    "kimi-k3[1m]".to_string(),
                    "Kimi K3（Kimi）".to_string(),
                    true,
                    256_000,
                ),
                (
                    "ccs-claude-kimi--kimi-k3-thinking[1m]".to_string(),
                    "kimi-k3-thinking[1m]".to_string(),
                    "kimi-k3-thinking（Kimi）".to_string(),
                    true,
                    256_000,
                ),
            ]
        );
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        store: DeviceStore,
        db: Database,
    }

    /// Claude Code、Codex 都在 Stack 模式。Claude 的 kimi、zhipu 在名单里；gone 登记过但已
    /// 移除；deleted 在名单里但行已经删了。Codex 名单为空。
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = DeviceStore::at(dir.path());
        let db = Database::memory().unwrap();
        for row in [
            provider(
                "kimi",
                "Kimi",
                Some("kimi"),
                json!({ "ANTHROPIC_MODEL": "kimi-k3" }),
            ),
            provider(
                "zhipu",
                "Zhipu",
                None,
                json!({ "ANTHROPIC_MODEL": "glm-5.2[1M]" }),
            ),
            provider("gone", "Gone", None, json!({ "ANTHROPIC_MODEL": "g-1" })),
        ] {
            db.save_provider(&AppType::Claude, &row).unwrap();
        }
        state::update(&store, |live| {
            let claude = live.apps.entry("claude".to_string()).or_default();
            claude.mode = Some(state::Mode::Proxy);
            let stack = &mut claude.stack;
            stack.enabled = true;
            stack.members = ["kimi", "zhipu", "deleted"].map(str::to_string).to_vec();
            for id in ["kimi", "zhipu", "gone", "deleted"] {
                stack.keys.insert(id.to_string(), id.to_string());
            }
            let codex = live.apps.entry("codex".to_string()).or_default();
            codex.mode = Some(state::Mode::Proxy);
            codex.stack.enabled = true;
        })
        .unwrap();
        Fixture {
            _dir: dir,
            store,
            db,
        }
    }

    fn resolve_in(fx: &Fixture, app: AppType, model: &str) -> Resolved {
        resolve(&fx.db, &fx.store, &app, model).unwrap()
    }

    #[test]
    fn public_groups_resolve_only_current_members_and_legacy_ids_stay_pinned() {
        let fx = fixture();
        let row = provider(
            "zhipu",
            "Zhipu",
            None,
            json!({ "ANTHROPIC_MODEL": "kimi-k3" }),
        );
        fx.db.save_provider(&AppType::Claude, &row).unwrap();
        let Resolved::Hit(target) = resolve_in(&fx, AppType::Claude, "group/kimi-k3") else {
            panic!("group missing")
        };
        assert_eq!(
            target
                .candidates
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["kimi", "zhipu"]
        );
        assert_eq!(target.upstream_model, "kimi-k3");
        assert_eq!(
            hit(resolve_in(&fx, AppType::Claude, "kimi/kimi-k3")).0,
            "kimi"
        );
        assert_eq!(
            hit(resolve_in(&fx, AppType::Claude, "ccs-claude-kimi--kimi-k3")).0,
            "kimi"
        );
        assert_eq!(
            miss(resolve_in(&fx, AppType::Claude, "group/unknown")),
            StackMiss::Unknown
        );
        assert_eq!(
            miss(resolve_in(&fx, AppType::Claude, "kimi/unknown")),
            StackMiss::Unknown
        );
        state::update(&fx.store, |live| {
            live.apps
                .get_mut("claude")
                .unwrap()
                .stack
                .members
                .retain(|id| id != "kimi");
        })
        .unwrap();
        let Resolved::Hit(target) = resolve_in(&fx, AppType::Claude, "group/kimi-k3") else {
            panic!("remaining account missing")
        };
        assert_eq!(target.candidates.len(), 1);
        assert_eq!(target.provider.id, "zhipu");
        assert_eq!(
            miss(resolve_in(&fx, AppType::Claude, "kimi/kimi-k3")),
            StackMiss::Removed
        );
        state::update(&fx.store, |live| {
            live.apps.get_mut("claude").unwrap().stack.enabled = false;
        })
        .unwrap();
        assert_eq!(
            miss(resolve_in(&fx, AppType::Claude, "group/kimi-k3")),
            StackMiss::StackOff
        );
    }

    fn hit(resolved: Resolved) -> (String, String, String) {
        match resolved {
            Resolved::Hit(target) => (
                target.provider.id,
                target.upstream_model,
                target.original_model,
            ),
            other => panic!("expected a hit, got {other:?}"),
        }
    }

    fn miss(resolved: Resolved) -> StackMiss {
        match resolved {
            Resolved::Miss(miss) => miss,
            other => panic!("expected a miss, got {other:?}"),
        }
    }

    #[test]
    fn a_claude_route_publishes_its_whole_list_and_leads_with_its_default() {
        let fx = fixture();
        let stack = state::stack(&fx.store, "claude").unwrap();
        let members = |route| published_members(&fx.db, &AppType::Claude, &stack, route).unwrap();
        let ids = |route| {
            claude_published(&members(route))
                .into_iter()
                .map(|model| model.id)
                .collect::<Vec<_>>()
        };
        // 路由那家照常发布，和不在代理模式时算出来的一样。
        let all = ids(None);
        assert!(all.contains(&"kimi/kimi-k3".to_string()));
        assert_eq!(ids(Some("kimi")), all);
        assert_eq!(claude_route_default(&members(None)), None);
        let default = claude_route_default(&members(Some("zhipu"))).unwrap();
        assert_eq!(
            (default.id.as_str(), default.name.as_str()),
            ("zhipu/glm-5.2[1m]", "zhipu/glm-5.2[1m]")
        );

        // 界面上标出路由那家，它的模型 id 照常列出。
        let views = member_views(
            &AppType::Claude,
            &super::members(&fx.db, &AppType::Claude, &stack, Some("kimi")).unwrap(),
        );
        let route_flags: Vec<(&str, bool, usize)> = views
            .iter()
            .map(|view| (view.provider_id.as_str(), view.route, view.model_ids.len()))
            .collect();
        assert_eq!(route_flags, vec![("kimi", true, 1), ("zhipu", false, 1)]);
    }

    #[test]
    fn the_default_is_the_first_model_of_the_routes_list() {
        let fx = fixture();
        let kimi = with_stack_models(
            provider(
                "kimi",
                "Kimi",
                Some("kimi"),
                json!({ "ANTHROPIC_MODEL": "kimi-k3" }),
            ),
            json!([
                { "model": "kimi-k3-mini", "displayName": "kimi-k3-mini" },
                { "model": "kimi-k3" }
            ]),
        );
        fx.db.save_provider(&AppType::Claude, &kimi).unwrap();
        let stack = state::stack(&fx.store, "claude").unwrap();
        let published = published_members(&fx.db, &AppType::Claude, &stack, Some("kimi")).unwrap();
        let default = claude_route_default(&published).unwrap();
        assert_eq!(
            (default.id.as_str(), default.name.as_str()),
            ("kimi/kimi-k3-mini", "kimi/kimi-k3-mini")
        );
        let ids: Vec<String> = claude_published(&published)
            .into_iter()
            .map(|model| model.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "kimi/kimi-k3-mini".to_string(),
                "kimi/kimi-k3".to_string(),
                "zhipu/glm-5.2[1m]".to_string(),
            ]
        );
    }

    #[test]
    fn stacked_ids_resolve_to_their_provider_and_upstream_model() {
        let fx = fixture();
        assert_eq!(
            hit(resolve_in(&fx, AppType::Claude, "ccs-claude-kimi--kimi-k3")),
            (
                "kimi".to_string(),
                "kimi-k3".to_string(),
                "ccs-claude-kimi--kimi-k3".to_string()
            )
        );
        // 行里带 1M 标记的模型：不管客户端发来的 id 带不带标记，上游都用行里的原值。
        for id in ["ccs-claude-zhipu--glm-5.2", "ccs-claude-zhipu--glm-5.2[1m]"] {
            assert_eq!(
                hit(resolve_in(&fx, AppType::Claude, id)).1,
                "glm-5.2[1M]",
                "{id}"
            );
        }
        // 行里已经没有的模型照原样发。
        assert_eq!(
            hit(resolve_in(&fx, AppType::Claude, "ccs-claude-kimi--kimi-k9")).1,
            "kimi-k9"
        );
    }

    #[test]
    fn stacked_ids_that_cannot_be_resolved_never_fall_back_to_the_route() {
        let fx = fixture();
        let claude = |id| miss(resolve_in(&fx, AppType::Claude, id));
        assert_eq!(claude("ccs-claude-gone--g-1"), StackMiss::Removed);
        assert_eq!(claude("ccs-claude-deleted--d-1"), StackMiss::Deleted);
        assert_eq!(claude("ccs-claude-nobody--m"), StackMiss::Unknown);
        assert_eq!(claude("ccs-claude-kimi"), StackMiss::Unknown);
        // 名单为空（这个应用从没加过 Stack 模型）也一样报错。
        assert_eq!(
            miss(resolve_in(&fx, AppType::Codex, "ccs-kimi/kimi-k3")),
            StackMiss::Unknown
        );
        let message = StackMiss::Removed.message("ccs-claude-gone--g-1");
        assert!(message.contains("ccs-claude-gone--g-1"), "{message}");
    }

    #[test]
    fn stacked_ids_are_rejected_outside_stack_mode() {
        let fx = fixture();
        let id = "ccs-claude-kimi--kimi-k3";
        assert!(matches!(
            resolve_in(&fx, AppType::Claude, id),
            Resolved::Hit(_)
        ));

        // 路由模式：名单和 key 都留着，客户端手里的旧 id 也不能转给名单里的那家。
        state::update(&fx.store, |live| {
            live.apps.get_mut("claude").unwrap().stack.enabled = false;
        })
        .unwrap();
        assert_eq!(
            miss(resolve_in(&fx, AppType::Claude, id)),
            StackMiss::StackOff
        );

        // 退回直连后附加位不动，同样不解析。
        state::update(&fx.store, |live| {
            let claude = live.apps.get_mut("claude").unwrap();
            claude.stack.enabled = true;
            claude.mode = Some(state::Mode::Direct);
        })
        .unwrap();
        assert_eq!(
            miss(resolve_in(&fx, AppType::Claude, id)),
            StackMiss::StackOff
        );
        let message = StackMiss::StackOff.message(id);
        assert!(message.contains(id), "{message}");
    }

    #[test]
    fn codex_ids_resolve_to_the_catalog_model_and_leave_the_routes_names_alone() {
        let fx = fixture();
        let mut deepseek = provider("ds", "DeepSeek", Some("deepseek"), json!({}));
        deepseek.settings_config = json!({
            "auth": {},
            "config": "model = \"deepseek-v4-flash\"\n",
            "modelCatalog": { "models": [{ "model": "deepseek-v4-pro" }] },
        });
        fx.db.save_provider(&AppType::Codex, &deepseek).unwrap();
        state::update(&fx.store, |live| {
            let stack = &mut live.apps.entry("codex".to_string()).or_default().stack;
            stack.members = vec!["ds".to_string()];
            stack.keys.insert("deepseek".to_string(), "ds".to_string());
        })
        .unwrap();

        assert_eq!(
            codex_model_ids("deepseek", &deepseek),
            vec!["ccs-deepseek/deepseek-v4-pro"]
        );
        assert_eq!(
            hit(resolve_in(
                &fx,
                AppType::Codex,
                "ccs-deepseek/deepseek-v4-pro"
            )),
            (
                "ds".to_string(),
                "deepseek-v4-pro".to_string(),
                "ccs-deepseek/deepseek-v4-pro".to_string()
            )
        );
        // 已登记的账号命名空间解析到该账号；其他供应商路径照旧走普通路由。
        assert_eq!(
            hit(resolve_in(&fx, AppType::Codex, "deepseek/deepseek-v4-pro")).0,
            "ds"
        );
        assert!(matches!(
            resolve_in(&fx, AppType::Codex, "vendor/deepseek-v4-pro"),
            Resolved::Plain
        ));
    }
}
