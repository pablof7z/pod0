use std::path::Path;

use crate::ffi::actions::tasks_module::AgentTasksAction;
use crate::ffi::projections::AgentTaskSummary;
use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    pub(crate) fn handle_task_action(&self, action: AgentTasksAction) -> serde_json::Value {
        let app = self.app;
        let dispatch = move |namespace: &str, body: &str| -> bool {
            if app.is_null() {
                return false;
            }
            let (Ok(ns_c), Ok(body_c)) = (
                std::ffi::CString::new(namespace),
                std::ffi::CString::new(body),
            ) else {
                return false;
            };
            let raw = nmp_ffi::nmp_app_dispatch_action(app, ns_c.as_ptr(), body_c.as_ptr());
            if raw.is_null() {
                return false;
            }
            // SAFETY: `raw` is a heap-owned NUL-terminated C string minted by
            // `nmp_app_dispatch_action`; read it, then return ownership with
            // `nmp_free_string`.
            let envelope = unsafe { std::ffi::CStr::from_ptr(raw) }
                .to_string_lossy()
                .into_owned();
            nmp_ffi::nmp_free_string(raw);
            envelope.contains("\"correlation_id\"")
        };

        let data_dir = self
            .store
            .lock()
            .ok()
            .and_then(|store| store.data_dir().map(Path::to_path_buf));
        let persist = |tasks: &[AgentTaskSummary]| {
            let Some(dir) = data_dir.as_deref() else {
                return;
            };
            let _ = crate::store::agent_tasks::save_agent_tasks(dir, tasks);
        };

        crate::tasks_handler::handle_tasks_action_with_persist(
            action,
            &self.agent_tasks,
            &self.rev,
            Some(&dispatch),
            Some(&persist),
        )
    }
}
