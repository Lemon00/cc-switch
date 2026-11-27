//! Provider catalog reads and explicit native-file reconciliation.
use super::*;

impl ProviderService {
    /// Read the persisted snapshot without touching native files or writing rows.
    pub fn list(
        state: &AppState,
        app_type: AppType,
    ) -> Result<IndexMap<String, Provider>, AppError> {
        Ok(state.db.get_provider_rows(&app_type)?.readable)
    }

    /// Reconcile native files and return the refreshed catalog. This is a write use case.
    pub fn refresh(
        state: &AppState,
        app_type: AppType,
    ) -> Result<IndexMap<String, Provider>, AppError> {
        if !app_type.has_native_provider_catalog() {
            return Self::list(state, app_type);
        }
        let _native_guard = lock_native_provider_operations(state, &app_type);
        if app_type == AppType::Dsh {
            // A profile can appear while CC Switch is running. Keep the provider list usable
            // when a native lock or an invalid profile prevents the initial sync; retry later.
            if let Err(error) = initialize_dsh_profiles(state) {
                log::warn!("Initializing DeepSeek Harness profiles failed: {error}");
            }
        }

        if matches!(app_type, AppType::Dsh) {
            let native = crate::dsh_config::get_providers()?;
            let mut saved = match state.db.get_all_providers(&app_type) {
                Ok(saved) => saved,
                // 有读不出来的行时不合并：按「库里没有」补存原生条目会盖掉那一行。
                Err(error) => {
                    log::warn!("Not merging native {app_type:?} providers: {error}");
                    return Ok(state.db.get_provider_rows(&app_type)?.readable);
                }
            };
            for (id, config) in &native {
                if !saved.contains_key(id) {
                    let name = config.get("name").and_then(Value::as_str).unwrap_or(id);
                    saved.insert(
                        id.clone(),
                        Provider::with_id(id.clone(), name.into(), config.clone(), None),
                    );
                }
            }
            for (id, provider) in &mut saved {
                if let Some(config) = native.get(id) {
                    if let Some(name) = config.get("name").and_then(Value::as_str) {
                        provider.name = name.into();
                    }
                    provider.settings_config = config.clone();
                }
                Self::set_provider_live_config_managed(provider, native.contains_key(id));

                state.db.save_provider(&app_type, provider)?;
            }
            return Ok(saved);
        }
        Ok(state.db.get_provider_rows(&app_type)?.readable)
    }
}
