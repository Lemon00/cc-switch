//! Compact models.dev catalog and pricing synchronization. Remote metadata that is
//! not used by the application is skipped during deserialization and never sent over IPC.
use super::model_pricing::{self, ModelPricingInfo, ModelsDevSyncConfig};
use crate::{config, database::Database};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::{BTreeMap, BTreeSet}, sync::Arc, time::Duration};

pub type Catalog = BTreeMap<String, Provider>;

#[derive(Default, Deserialize, Serialize)]
pub struct Provider {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api: Option<String>,
    #[serde(default)]
    models: BTreeMap<String, Model>,
}

#[derive(Default, Deserialize, Serialize)]
pub struct Model {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    release_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cost: Option<Cost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modalities: Option<Modalities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<Limit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_options: Option<Vec<ReasoningOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical_model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct Cost {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_read: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_write: Option<Value>,
}

#[derive(Default, Deserialize, Serialize)]
struct Modalities {
    #[serde(default)] input: Vec<String>,
    #[serde(default)] output: Vec<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct Limit {
    #[serde(default, skip_serializing_if = "Option::is_none")] context: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] input: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] output: Option<u64>,
}

#[derive(Default, Deserialize, Serialize)]
struct ReasoningOption {
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")] kind: Option<String>,
    #[serde(default)] values: Vec<String>,
}

static FETCH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static SYNC_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn catalog() -> Result<Catalog, String> {
    let _guard = FETCH_LOCK.lock().await;
    let cached = tokio::task::spawn_blocking(|| {
        let path = config::get_app_config_dir().join("models-dev-catalog-v1.json");
        let fresh = path.metadata().and_then(|m| m.modified()).ok()
            .and_then(|t| t.elapsed().ok()).is_some_and(|age| age < Duration::from_secs(3600));
        if fresh {
            if let Ok(file) = std::fs::File::open(&path) {
                if let Ok(data) = serde_json::from_reader(std::io::BufReader::new(file)) {
                    return Some(data);
                }
            }
        }
        None
    }).await.map_err(|e| e.to_string())?;
    if let Some(data) = cached { return Ok(data); }
            let bytes = model_pricing::fetch_models_dev_bytes().await?;
            tokio::task::spawn_blocking(move || {
                let data: Catalog = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if data.is_empty() { return Err("models.dev returned an empty catalog".into()); }
                let path = config::get_app_config_dir().join("models-dev-catalog-v1.json");
                let compact = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
                config::atomic_write(&path, &compact).map_err(|e| e.to_string())?;
                Ok(data)
            }).await.map_err(|e| e.to_string())?
}

struct Entry {
    key: String, provider: String, id: String, name: String, release: String, prices: [f64; 4],
}

fn normalize_id(id: &str) -> String {
    let id = id.rsplit('/').next().unwrap_or(id).split(':').next().unwrap_or("")
        .trim().replace('@', "-").to_lowercase();
    id.strip_suffix("[1m]").unwrap_or(&id).trim().to_owned()
}

fn entries(catalog: &Catalog) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (provider, data) in catalog {
        for (id, model) in &data.models {
            let name = model.name.as_deref().filter(|v| !v.is_empty()).unwrap_or(id);
            let searchable = format!("{id} {name}").to_lowercase();
            if model.status.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("deprecated"))
                || ["audio", "deprecated", "embedding", "image", "moderation", "realtime", "transcribe", "tts", "video"]
                    .iter().any(|marker| searchable.contains(marker)) { continue; }
            if let Some(m) = &model.modalities {
                if !m.output.is_empty() && (!m.output.iter().any(|s| s.eq_ignore_ascii_case("text"))
                    || m.output.iter().any(|s| ["audio", "image", "video"].iter().any(|v| s.eq_ignore_ascii_case(v)))) { continue; }
            }
            let Some(cost) = &model.cost else { continue; };
            let values = [&cost.input, &cost.output, &cost.cache_read, &cost.cache_write];
            if values.iter().any(|v| v.as_ref().is_some_and(|v| !v.is_null() && !v.as_f64().is_some_and(|n| n.is_finite() && (0.0..1e12).contains(&n)))) { continue; }
            if cost.input.as_ref().and_then(Value::as_f64).is_none() && cost.output.as_ref().and_then(Value::as_f64).is_none() { continue; }
            if normalize_id(id).is_empty() { continue; }
            entries.push(Entry { key: format!("{provider}/{id}"), provider: provider.clone(), id: id.clone(), name: name.into(),
                release: model.release_date.clone().unwrap_or_default(), prices: values.map(|v| v.as_ref().and_then(Value::as_f64).unwrap_or(0.0)) });
        }
    }
    entries.sort_by(|a, b| b.release.cmp(&a.release).then(a.name.cmp(&b.name)).then(a.key.cmp(&b.key)));
    entries
}

fn common_family(entry: &Entry) -> Option<&'static str> {
    let id = entry.id.to_lowercase();
    match entry.provider.as_str() {
        "anthropic" if id.starts_with("claude-") => Some("claude"),
        "openai" if ["gpt-", "o1-", "o3-", "o4-"].iter().any(|p| id.starts_with(p)) => Some("gpt"),
        "google" if id.starts_with("gemini-") => Some("gemini"),
        "xai" if id.starts_with("grok-") => Some("grok"),
        "deepseek" if id.starts_with("deepseek-") => Some("deepseek"),
        "alibaba" if id.starts_with("qwen") => Some("qwen"),
        "xiaomi" if id.starts_with("mimo-") => Some("mimo"),
        "longcat" if id.starts_with("longcat-") => Some("longcat"),
        "moonshotai" if id.starts_with("kimi-") => Some("kimi"),
        "minimax-cn" if id.starts_with("minimax-m") => Some("minimax"),
        "zai" if id.starts_with("glm-") => Some("glm"),
        _ => None,
    }
}

fn selected_pricing(catalog: &Catalog, config: &ModelsDevSyncConfig) -> (usize, Vec<ModelPricingInfo>) {
    let explicit: BTreeSet<_> = config.selected_model_keys.iter().collect();
    let excluded: BTreeSet<_> = config.excluded_common_model_keys.iter().collect();
    let mut families = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut selected = 0;
    let mut pricing = Vec::new();
    for entry in entries(catalog) {
        let common = config.include_common_models && common_family(&entry).is_some_and(|family| {
            let count = families.entry(family).or_insert(0);
            *count += 1;
            *count <= 6
        });
        if !explicit.contains(&entry.key) && (!common || excluded.contains(&entry.key)) { continue; }
        selected += 1;
        let id = normalize_id(&entry.id);
        if !seen.insert(id.clone()) { continue; }
        let [input, output, read, write] = entry.prices.map(|v| {
            let s = format!("{v:.6}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s.is_empty() { "0".into() } else { s.to_string() }
        });
        pricing.push(ModelPricingInfo { model_id: id, display_name: entry.name, input_cost_per_million: input,
            output_cost_per_million: output, cache_read_cost_per_million: read, cache_creation_cost_per_million: write });
    }
    (selected, pricing)
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    skipped: bool, selected: usize, imported: usize, changed: usize, synced_at: Option<i64>,
}

pub async fn sync_pricing(db: Arc<Database>, force: bool) -> Result<SyncResult, String> {
    let _guard = SYNC_LOCK.lock().await;
    let db_read = db.clone();
    let state = tokio::task::spawn_blocking(move || model_pricing::get_models_dev_sync_state(&db_read))
        .await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp_millis();
    if !force && (!state.config.auto_sync_enabled || state.config.last_sync_at.is_some_and(|t| t <= now && now - t < 6 * 3600 * 1000)) {
        return Ok(SyncResult { skipped: true, synced_at: state.config.last_sync_at, ..Default::default() });
    }
    let result: Result<SyncResult, String> = async {
        let catalog = catalog().await?;
        let db_write = db.clone();
        tokio::task::spawn_blocking(move || {
            let latest = model_pricing::get_models_dev_sync_state(&db_write).map_err(|e| e.to_string())?;
            if !force && !latest.config.auto_sync_enabled {
                return Ok(SyncResult { skipped: true, synced_at: latest.config.last_sync_at, ..Default::default() });
            }
            let (selected, pricing) = selected_pricing(&catalog, &latest.config);
            let imported = pricing.len();
            let changed = if pricing.is_empty() { 0 } else { model_pricing::update_model_pricing_batch(&db_write, pricing).map_err(|e| e.to_string())? };
            let synced_at = Some(chrono::Utc::now().timestamp_millis());
            model_pricing::record_models_dev_sync_result(&db_write, synced_at, None).map_err(|e| e.to_string())?;
            Ok(SyncResult { skipped: false, selected, imported, changed, synced_at })
        }).await.map_err(|e| e.to_string())?
    }.await;
    if let Err(error) = &result {
        let error: String = error.clone();
        let _ = tokio::task::spawn_blocking(move || model_pricing::record_models_dev_sync_result(&db, None, Some(error))).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_drops_metadata_and_preserves_model_parameters() {
        let data: Catalog = serde_json::from_str(r#"{"openai":{"name":"OpenAI","npm":"unused","models":{"gpt-5":{"name":"GPT 5","description":"large unused metadata","limit":{"context":128000},"cost":{"input":1,"output":2}}}}}"#).unwrap();
        let serialized = serde_json::to_string(&data).unwrap();
        assert!(!serialized.contains("unused"));
        assert!(!serialized.contains("description"));
        assert!(serialized.contains("128000"));
        let (_, prices) = selected_pricing(&data, &ModelsDevSyncConfig::default());
        assert_eq!(prices[0].input_cost_per_million, "1");
    }
    #[test]
    fn selection_excludes_invalid_prices_and_honors_explicit_choices() {
        let data: Catalog = serde_json::from_str(r#"{"openai":{"models":{"gpt-5":{"cost":{"input":1}},"gpt-bad":{"cost":{"input":-1}},"gpt-string":{"cost":{"input":"2"}},"gpt-audio":{"cost":{"input":2}}}},"relay":{"models":{"vendor/custom:variant[1m]":{"cost":{"input":0,"output":0.000001}}}}}"#).unwrap();
        let mut config = ModelsDevSyncConfig::default();
        config.excluded_common_model_keys.push("openai/gpt-5".into());
        config.selected_model_keys.push("relay/vendor/custom:variant[1m]".into());
        let (count, pricing) = selected_pricing(&data, &config);
        assert_eq!(count, 1);
        assert_eq!(pricing[0].model_id, "custom");
        assert_eq!(pricing[0].output_cost_per_million, "0.000001");
    }
}
