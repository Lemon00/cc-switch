//! Application-level MCP write plan. Adapters retain ownership of native
//! validation, write locks and compensating rollback. The DB commit is the
//! final continuation; a failure unwinds completed file writes in reverse order.
use super::McpService;
use crate::{
    app_config::{AppType, McpServer},
    error::AppError,
    mcp,
    store::AppState,
};

pub(super) fn commits_with_db(app: &AppType) -> bool {
    NATIVE_ORDER.contains(app)
}

// All operations use the same native lock acquisition order.
const NATIVE_ORDER: [AppType; 2] = [AppType::Agy, AppType::Dsh];

#[derive(Clone, Copy)]
enum Change {
    SetEnabled(bool),
    Remove,
}

pub(super) struct McpWritePlan<'a> {
    server: &'a McpServer,
    steps: Vec<(AppType, Change)>,
}

impl<'a> McpWritePlan<'a> {
    pub fn upsert(server: &'a McpServer, previous: Option<&'a McpServer>) -> Self {
        let mut plan = Self {
            server,
            steps: Vec::new(),
        };
        for app in NATIVE_ORDER
            .into_iter()
            // Commit the remaining supported applications after the native adapters.
            .chain(AppType::persisted().filter(|app| !commits_with_db(app)))
        {
            let enabled = server.apps.is_enabled_for(&app);
            let previously_enabled = previous.is_some_and(|saved| saved.apps.is_enabled_for(&app));
            if enabled || previously_enabled {
                plan.steps.push((app, Change::SetEnabled(enabled)));
            }
        }
        plan
    }

    pub fn delete(server: &'a McpServer) -> Self {
        let steps = NATIVE_ORDER
            .into_iter()
            .chain(
                server
                    .apps
                    .enabled_apps()
                    .into_iter()
                    .filter(|app| !commits_with_db(app)),
            )
            .filter(|app| server.apps.is_enabled_for(app))
            .map(|app| (app, Change::Remove))
            .collect();
        Self { server, steps }
    }

    pub fn toggle(server: &'a McpServer, app: AppType, enabled: bool) -> Self {
        Self {
            server,
            steps: vec![(app, Change::SetEnabled(enabled))],
        }
    }

    pub fn execute(
        &self,
        state: &AppState,
        mut commit: impl FnMut() -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        self.execute_steps(state, &self.steps, &mut commit)
    }

    fn execute_steps(
        &self,
        state: &AppState,
        steps: &[(AppType, Change)],
        commit: &mut dyn FnMut() -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let Some(((app, change), remaining)) = steps.split_first() else {
            return commit();
        };
        let enabled = matches!(change, Change::SetEnabled(true));
        let spec = enabled.then_some(&self.server.server);
        let id = &self.server.id;
        let next = &mut || self.execute_steps(state, remaining, commit);
        match app {
            AppType::Dsh => mcp::dsh::sync_and_commit(id, spec, next),

            AppType::Agy => mcp::agy::sync_and_commit(
                id,
                match change {
                    Change::Remove => mcp::agy::AgyChange::Remove,
                    Change::SetEnabled(_) => mcp::agy::AgyChange::from_enabled(spec),
                },
                next,
            ),

            _ => McpService::sync_legacy_and_commit(self.server, app, enabled, next),
        }
    }
}
