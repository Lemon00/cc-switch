//! 统一供应商 (Universal Provider) DAO
//!
//! 提供统一供应商的 CRUD 操作。

use crate::database::{lock_conn, to_json_string, Database};
use crate::error::AppError;
use crate::provider::UniversalProvider;
use std::collections::HashMap;

/// 统一供应商的 Settings Key
const UNIVERSAL_PROVIDERS_KEY: &str = "universal_providers";

impl Database {
    /// 获取所有统一供应商
    pub fn get_all_universal_providers(
        &self,
    ) -> Result<HashMap<String, UniversalProvider>, AppError> {
        let conn = lock_conn!(self.conn);

        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let result: Option<String> =
            match stmt.query_row([UNIVERSAL_PROVIDERS_KEY], |row| row.get(0)) {
                Ok(value) => Some(value),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => {
                    return Err(AppError::Database(format!("读取统一供应商数据失败: {e}")));
                }
            };

        match result {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| AppError::Database(format!("解析统一供应商数据失败: {e}"))),
            None => Ok(HashMap::new()),
        }
    }

    /// 获取单个统一供应商
    pub fn get_universal_provider(&self, id: &str) -> Result<Option<UniversalProvider>, AppError> {
        let providers = self.get_all_universal_providers()?;
        Ok(providers.get(id).cloned())
    }

    /// 保存统一供应商（添加或更新）
    pub fn save_universal_provider(&self, provider: &UniversalProvider) -> Result<(), AppError> {
        self.mutate_universal_providers(|providers| {
            providers.insert(provider.id.clone(), provider.clone());
            Ok(())
        })
    }

    /// 删除统一供应商
    pub fn delete_universal_provider(&self, id: &str) -> Result<bool, AppError> {
        let mut existed = false;
        self.mutate_universal_providers(|providers| {
            existed = providers.remove(id).is_some();
            Ok(())
        })?;
        Ok(existed)
    }

    /// 在同一事务内读改写 settings.universal_providers，避免并发 Lost Update。
    fn mutate_universal_providers(
        &self,
        mutate: impl FnOnce(&mut HashMap<String, UniversalProvider>) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let mut guard = lock_conn!(self.conn);
        let conn = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let raw: Option<String> = match stmt.query_row([UNIVERSAL_PROVIDERS_KEY], |row| row.get(0))
        {
            Ok(value) => Some(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => {
                return Err(AppError::Database(format!("读取统一供应商数据失败: {e}")));
            }
        };
        drop(stmt);
        let mut providers: HashMap<String, UniversalProvider> = match raw {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| AppError::Database(format!("解析统一供应商数据失败: {e}")))?,
            None => HashMap::new(),
        };
        mutate(&mut providers)?;
        let json = to_json_string(&providers)?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
            [UNIVERSAL_PROVIDERS_KEY, &json],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        conn.commit().map_err(|e| AppError::Database(e.to_string()))
    }
}
