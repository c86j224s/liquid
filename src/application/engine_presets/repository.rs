use crate::contracts::EnginePreset;
use liquid_storage_sqlite::{
    NewEnginePresetRecord, SqliteEnginePresetRepository as StorageSqliteEnginePresetRepository,
};
use sqlx::sqlite::SqlitePool;

pub(crate) struct SqliteEnginePresetRepository<'a> {
    inner: StorageSqliteEnginePresetRepository<'a>,
}

impl<'a> SqliteEnginePresetRepository<'a> {
    pub(crate) fn new(db: &'a SqlitePool) -> Self {
        Self {
            inner: StorageSqliteEnginePresetRepository::new(db),
        }
    }

    pub(crate) async fn list_custom_presets(&self) -> Result<Vec<EnginePreset>, sqlx::Error> {
        self.inner.list_custom_presets::<EnginePreset>().await
    }

    pub(crate) async fn load_custom_preset_by_id(
        &self,
        id: i64,
    ) -> Result<Option<EnginePreset>, sqlx::Error> {
        self.inner
            .load_custom_preset_by_id::<EnginePreset>(id)
            .await
    }

    pub(crate) async fn load_enabled_custom_preset_by_id(
        &self,
        id: i64,
    ) -> Result<Option<EnginePreset>, sqlx::Error> {
        self.inner
            .load_enabled_custom_preset_by_id::<EnginePreset>(id)
            .await
    }

    pub(crate) async fn insert_custom_preset(
        &self,
        record: &NewEnginePresetRecord,
    ) -> Result<i64, sqlx::Error> {
        self.inner.insert_custom_preset(record).await
    }

    pub(crate) async fn update_custom_preset_name_and_model(
        &self,
        id: i64,
        name: &str,
        model: Option<&str>,
    ) -> Result<u64, sqlx::Error> {
        self.inner
            .update_custom_preset_name_and_model(id, name, model)
            .await
    }

    pub(crate) async fn delete_custom_preset(&self, id: i64) -> Result<u64, sqlx::Error> {
        self.inner.delete_custom_preset(id).await
    }

    pub(crate) async fn record_test_result(
        &self,
        id: i64,
        status: &str,
        message: &str,
    ) -> Result<u64, sqlx::Error> {
        self.inner.record_test_result(id, status, message).await
    }
}
