use super::*;
use crate::app_config::AppType;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn provider(id: &str) -> Provider {
    Provider::with_id(id.into(), id.into(), json!({"env": {"key": "test"}}), None)
}

#[test]
fn unchanged_provider_save_does_not_emit_a_database_change() {
    let db = Database::memory().unwrap();
    let mut provider = provider("same");
    db.save_provider(&AppType::Claude, &provider).unwrap();
    db.add_custom_endpoint(&AppType::Claude, "same", "https://endpoint")
        .unwrap();
    {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "UPDATE providers SET is_current = 1, in_failover_queue = 1",
            [],
        )
        .unwrap();
    }
    let updates = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&updates);
    db.conn.lock().unwrap().update_hook(Some(
        move |action: rusqlite::hooks::Action, _: &str, table: &str, _: i64| {
            if action == rusqlite::hooks::Action::SQLITE_UPDATE && table == "providers" {
                count.fetch_add(1, Ordering::Relaxed);
            }
        },
    ));

    for _ in 0..5 {
        db.save_provider(&AppType::Claude, &provider).unwrap();
    }
    assert_eq!(updates.load(Ordering::Relaxed), 0);

    // NULL <-> value transitions and normal edits must still be persisted.
    provider.notes = Some("updated".into());
    db.save_provider(&AppType::Claude, &provider).unwrap();
    assert_eq!(
        db.get_provider_by_id("same", &AppType::Claude)
            .unwrap()
            .unwrap()
            .notes,
        provider.notes
    );
    provider.notes = None;
    provider.name = "renamed".into();
    provider.settings_config = json!({"env": {"key": "changed"}});
    db.save_provider(&AppType::Claude, &provider).unwrap();
    db.save_provider(&AppType::Claude, &provider).unwrap();
    assert_eq!(updates.load(Ordering::Relaxed), 2);
    let loaded = db
        .get_all_providers(&AppType::Claude)
        .unwrap()
        .swap_remove("same")
        .unwrap();
    assert_eq!(loaded.name, "renamed");
    assert_eq!(loaded.notes, None);
    assert_eq!(loaded.settings_config, provider.settings_config);
    assert!(loaded.in_failover_queue);
    assert_eq!(loaded.meta.unwrap().custom_endpoints.len(), 1);
    // Check the stored flag without consulting the device-local current pointer.
    let is_current: bool = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT is_current FROM providers WHERE id = 'same' AND app_type = 'claude'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(is_current);
}

#[test]
fn provider_save_does_not_restore_removed_endpoints_or_drop_new_ones() {
    let db = Database::memory().unwrap();
    db.save_provider(&AppType::Claude, &provider("stale"))
        .unwrap();
    db.add_custom_endpoint(&AppType::Claude, "stale", "https://old")
        .unwrap();
    let stale = db.get_all_providers(&AppType::Claude).unwrap()["stale"].clone();
    db.remove_custom_endpoint(&AppType::Claude, "stale", "https://old")
        .unwrap();
    db.add_custom_endpoint(&AppType::Claude, "stale", "https://new")
        .unwrap();
    db.save_provider(&AppType::Claude, &stale).unwrap();
    let saved = db.get_all_providers(&AppType::Claude).unwrap();
    let endpoints = &saved["stale"].meta.as_ref().unwrap().custom_endpoints;
    assert_eq!(endpoints.len(), 1);
    assert!(endpoints.contains_key("https://new"));
}

#[test]
fn unchanged_metadata_maps_do_not_emit_database_changes_after_reload() {
    let db = Database::memory().unwrap();
    let mut provider = provider("metadata");
    provider.meta = Some(
        serde_json::from_value(json!({
            "localProxyRequestOverrides": {
                "headers": {"X-First": "one", "X-Second": "two", "X-Third": "three"},
                "body": {"reasoning": {"effort": "high"}}
            }
        }))
        .unwrap(),
    );
    db.save_provider(&AppType::Claude, &provider).unwrap();
    {
        let conn = db.conn.lock().unwrap();
        let stored: String = conn
            .query_row(
                "SELECT meta FROM providers WHERE id = 'metadata'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut reordered: serde_json::Value = serde_json::from_str(&stored).unwrap();
        let headers = reordered["localProxyRequestOverrides"]["headers"]
            .as_object_mut()
            .unwrap();
        let reversed = headers
            .iter()
            .rev()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        *headers = reversed;
        let reordered = reordered.to_string();
        assert_ne!(stored, reordered);
        // HashMap serialization order is unstable across reads/devices. Make
        // the first equivalent-but-differently-ordered save deterministic.
        conn.execute(
            "UPDATE providers SET meta = ?1 WHERE id = 'metadata'",
            [reordered],
        )
        .unwrap();
    }
    let updates = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&updates);
    db.conn.lock().unwrap().update_hook(Some(
        move |action: rusqlite::hooks::Action, _: &str, table: &str, _: i64| {
            if action == rusqlite::hooks::Action::SQLITE_UPDATE && table == "providers" {
                count.fetch_add(1, Ordering::Relaxed);
            }
        },
    ));
    db.save_provider(&AppType::Claude, &provider).unwrap();
    for _ in 0..5 {
        provider = db
            .get_provider_by_id("metadata", &AppType::Claude)
            .unwrap()
            .unwrap();
        db.save_provider(&AppType::Claude, &provider).unwrap();
    }
    assert_eq!(updates.load(Ordering::Relaxed), 0);

    provider
        .meta
        .as_mut()
        .unwrap()
        .local_proxy_request_overrides
        .as_mut()
        .unwrap()
        .headers
        .insert("X-First".into(), "changed".into());
    db.save_provider(&AppType::Claude, &provider).unwrap();
    assert_eq!(updates.load(Ordering::Relaxed), 1);
    let loaded = db
        .get_provider_by_id("metadata", &AppType::Claude)
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded
            .meta
            .unwrap()
            .local_proxy_request_overrides
            .unwrap()
            .headers["X-First"],
        "changed"
    );
}

#[test]
#[ignore = "manual performance benchmark; reports median latency without timing assertions"]
fn benchmark_provider_list() {
    for count in [10, 100, 1000] {
        let db = Database::memory().unwrap();
        for i in 0..count {
            let id = format!("provider-{i:04}");
            db.save_provider(&AppType::Claude, &provider(&id)).unwrap();
            for endpoint in 0..3 {
                db.add_custom_endpoint(
                    &AppType::Claude,
                    &id,
                    &format!("https://endpoint-{endpoint}"),
                )
                .unwrap();
            }
        }
        let mut samples = Vec::new();
        for _ in 0..25 {
            let start = std::time::Instant::now();
            let result = db.get_all_providers(&AppType::Claude).unwrap();
            assert_eq!(result.len(), count);
            std::hint::black_box(result);
            samples.push(start.elapsed());
        }
        samples.sort_unstable();
        eprintln!(
            "provider_list count={count} endpoints={} median_us={}",
            count * 3,
            samples[samples.len() / 2].as_micros()
        );
    }
}

/// 新版本写入、本版本认不出的 meta（比如新增的枚举值）：读出来要报错点名，不能换成默认值，
/// 库里的原文一个字节都不能动；列表读取把读不出来的行单独列出，其余照常。
#[test]
fn unreadable_stored_json_is_reported_and_left_untouched() {
    let db = Database::memory().unwrap();
    for id in ["good", "broken"] {
        db.save_provider(&AppType::Claude, &provider(id)).unwrap();
    }
    let raw = |id: &str| -> (String, String) {
        db.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT settings_config, meta FROM providers WHERE app_type = 'claude' AND id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    };
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE providers SET settings_config = '{\"env\":' WHERE app_type = 'claude' AND id = 'broken'",
            [],
        )
        .unwrap();
    let stored = raw("broken");

    // 严格读取停在读不出的行并点名它，不写回任何默认值。
    let error = db
        .get_all_providers(&AppType::Claude)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("broken") && error.contains("settings_config"),
        "{error}"
    );
    let error = db
        .get_provider_by_id("broken", &AppType::Claude)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("broken") && error.contains("settings_config"),
        "{error}"
    );

    // 列表读取：能读的照常返回，读不出的单独列出。
    let rows = db.get_provider_rows(&AppType::Claude).unwrap();
    assert_eq!(rows.readable.keys().collect::<Vec<_>>(), ["good"]);
    assert_eq!(rows.unreadable.len(), 1);
    assert_eq!(rows.unreadable[0].id, "broken");
    assert!(rows.get("broken").is_err());
    assert!(rows.get("good").unwrap().is_some());
    assert!(db
        .get_unreadable_provider(&AppType::Claude, "broken")
        .unwrap()
        .is_some());
    assert!(db
        .get_unreadable_provider(&AppType::Claude, "good")
        .unwrap()
        .is_none());

    // 只问存不存在的地方不解析 JSON。
    assert!(db.provider_exists("broken", &AppType::Claude).unwrap());
    assert!(db
        .get_provider_by_id("good", &AppType::Claude)
        .unwrap()
        .is_some());
    assert_eq!(raw("broken"), stored);
}

/// 故障转移路由只读、不写：队列里读不出的成员被跳过，其余照常。
#[test]
fn failover_queue_skips_an_unreadable_member() {
    let db = Database::memory().unwrap();
    for id in ["a", "b"] {
        db.save_provider(&AppType::Claude, &provider(id)).unwrap();
        db.add_to_failover_queue(&AppType::Claude, id).unwrap();
    }
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE providers SET meta = '{' WHERE app_type = 'claude' AND id = 'a'",
            [],
        )
        .unwrap();

    let ids: Vec<String> = db
        .get_failover_providers(&AppType::Claude)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, ["b"]);
}

#[test]
fn failover_queue_hides_retired_services_but_keeps_unreadable_rows_removable() {
    let db = Database::memory().unwrap();
    for app in [AppType::Claude, AppType::Codex] {
        for id in ["supported", "broken", "retired"] {
            db.save_provider(&app, &provider(id)).unwrap();
            db.add_to_failover_queue(&app, id).unwrap();
        }
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "UPDATE providers SET meta = '{' WHERE id = 'broken' AND app_type = ?1",
                [&app],
            )
            .unwrap();
            conn.execute("UPDATE providers SET meta = '{\"providerType\":\"github_copilot\"}' WHERE id = 'retired' AND app_type = ?1", [&app]).unwrap();
        }
        let queue = db.get_failover_queue(&app).unwrap();
        assert_eq!(
            queue
                .iter()
                .map(|item| item.provider_id.as_str())
                .collect::<Vec<_>>(),
            ["broken", "supported"]
        );
        let routable = db.get_failover_providers(&app).unwrap();
        assert_eq!(
            routable
                .iter()
                .map(|provider| provider.id.as_str())
                .collect::<Vec<_>>(),
            ["supported"]
        );
        db.remove_from_failover_queue(&app, "broken").unwrap();
        assert_eq!(db.get_failover_queue(&app).unwrap().len(), 1);
    }
}

#[test]
fn rename_to_retired_service_keeps_the_original_row_and_endpoints() {
    let db = Database::memory().unwrap();
    db.save_provider(&AppType::OpenCode, &provider("original"))
        .unwrap();
    db.add_custom_endpoint(&AppType::OpenCode, "original", "https://relay.example")
        .unwrap();
    let mut retired = provider("renamed");
    retired.meta = Some(ProviderMeta {
        provider_type: Some("github_copilot".into()),
        ..Default::default()
    });
    assert!(db
        .replace_provider_id(&AppType::OpenCode, "original", &retired)
        .is_err());
    assert!(db.provider_exists("original", &AppType::OpenCode).unwrap());
    assert!(!db.provider_exists("renamed", &AppType::OpenCode).unwrap());
    assert_eq!(
        db.conn
            .lock()
            .unwrap()
            .query_row("SELECT provider_id FROM provider_endpoints", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "original"
    );
}

#[test]
fn current_omo_provider_hides_retired_service() {
    let db = Database::memory().unwrap();
    for category in ["omo", "omo-slim"] {
        let mut provider = provider(category);
        provider.category = Some(category.into());
        db.save_provider(&AppType::OpenCode, &provider).unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE providers SET is_current = 1 WHERE id = ?1",
                [category],
            )
            .unwrap();
        assert!(db
            .get_current_omo_provider(&AppType::OpenCode, category)
            .unwrap()
            .is_some());
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE providers SET meta = '{\"providerType\":\"xai_oauth\"}' WHERE id = ?1",
                [category],
            )
            .unwrap();
        assert!(db
            .get_current_omo_provider(&AppType::OpenCode, category)
            .unwrap()
            .is_none());
    }
}

#[test]
fn retired_services_are_not_loadable_or_saveable() {
    let db = Database::memory().unwrap();
    for service in ["github_copilot", "xai_oauth"] {
        let mut provider =
            Provider::with_id(service.into(), service.into(), serde_json::json!({}), None);
        provider.meta = Some(ProviderMeta {
            provider_type: Some(service.into()),
            ..Default::default()
        });
        assert!(db.save_provider(&AppType::Claude, &provider).is_err());
        db.conn.lock().unwrap().execute("INSERT INTO providers(id, app_type, name, settings_config, meta) VALUES (?1, 'claude', ?1, '{}', ?2)", rusqlite::params![service, serde_json::to_string(&provider.meta).unwrap()]).unwrap();
        assert!(db
            .get_provider_by_id(service, &AppType::Claude)
            .unwrap()
            .is_none());
        assert!(db
            .read_provider(&AppType::Claude, service)
            .unwrap()
            .is_none());
    }
    assert!(db.get_all_providers(&AppType::Claude).unwrap().is_empty());
}

#[test]
fn retired_endpoint_cards_are_rejected_without_matching_url_text_or_models() {
    let db = Database::memory().unwrap();
    for url in [
        "https://api.githubcopilot.com",
        "https://api.individual.githubcopilot.com",
        "https://api.x.ai/v1",
        "https://grok.com/rest",
    ] {
        let provider = Provider::with_id(
            "old".into(),
            "Old".into(),
            serde_json::json!({"env": {"ANTHROPIC_BASE_URL": url}}),
            None,
        );
        assert!(
            db.save_provider(&AppType::Claude, &provider).is_err(),
            "{url}"
        );
        let provider = Provider::with_id(
            "old".into(),
            "Old".into(),
            serde_json::json!({"config": format!("model_provider = 'custom'\n[model_providers.custom]\nbase_url = '{url}'")}),
            None,
        );
        assert!(
            db.save_provider(&AppType::Codex, &provider).is_err(),
            "{url}"
        );
    }
    for url in [
        "https://relay.example/api.x.ai",
        "https://api.x.ai.example/v1",
    ] {
        let provider = Provider::with_id(
            "custom".into(),
            "Custom".into(),
            serde_json::json!({"env": {"ANTHROPIC_BASE_URL": url, "ANTHROPIC_MODEL": "grok-4"}}),
            None,
        );
        assert!(
            db.save_provider(&AppType::Claude, &provider).is_ok(),
            "{url}"
        );
    }
}
