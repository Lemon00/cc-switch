//! Pricing queries. File synchronization and historical cost backfill are
//! application concerns; a snapshot query only reads the stored prices.
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::pricing::ModelPricingInfo;

impl Database {
    pub fn list_model_pricing(&self) -> Result<Vec<ModelPricingInfo>, AppError> {
        let conn = lock_conn!(self.conn);
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='model_pricing'",
            [],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )?;
        if !table_exists {
            log::error!("model_pricing 表不存在,可能需要重启应用以触发数据库迁移");
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
             FROM model_pricing
             ORDER BY display_name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ModelPricingInfo {
                model_id: row.get(0)?,
                display_name: row.get(1)?,
                input_cost_per_million: row.get(2)?,
                output_cost_per_million: row.get(3)?,
                cache_read_cost_per_million: row.get(4)?,
                cache_creation_cost_per_million: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
