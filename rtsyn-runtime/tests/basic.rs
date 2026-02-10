use rtsyn_plugin::{Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port};
use rtsyn_runtime::{PluginSchedule, Runtime};
use serde_json::Value;
use std::sync::{Arc, Mutex};

struct LogPlugin {
    id: PluginId,
    meta: PluginMeta,
    log: Arc<Mutex<Vec<PluginId>>>,
    ticks: Arc<Mutex<Vec<u64>>>,
}

impl LogPlugin {
    fn new(id: u64, log: Arc<Mutex<Vec<PluginId>>>, ticks: Arc<Mutex<Vec<u64>>>) -> Self {
        Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "log".to_string(),
                fixed_vars: vec![("fixed".to_string(), Value::Null)],
                default_vars: vec![],
            },
            log,
            ticks,
        }
    }
}

impl Plugin for LogPlugin {
    fn id(&self) -> PluginId {
        self.id
    }

    fn meta(&self) -> &PluginMeta {
        &self.meta
    }

    fn inputs(&self) -> &[Port] {
        &[]
    }

    fn outputs(&self) -> &[Port] {
        &[]
    }

    fn process(&mut self, ctx: &mut PluginContext) -> Result<(), PluginError> {
        self.log.lock().unwrap().push(self.id);
        self.ticks.lock().unwrap().push(ctx.tick);
        Ok(())
    }
}

#[test]
fn tick_orders_by_priority() {
    let mut runtime = Runtime::new(workspace::WorkspaceDefinition {
        name: "test".to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: Vec::new(),
        connections: Vec::new(),
        settings: workspace::WorkspaceSettings::default(),
    });
    let log = Arc::new(Mutex::new(Vec::new()));
    let ticks = Arc::new(Mutex::new(Vec::new()));

    let p1 = LogPlugin::new(1, log.clone(), ticks.clone());
    let p2 = LogPlugin::new(2, log.clone(), ticks.clone());

    runtime.add_plugin(
        Box::new(p1),
        PluginSchedule {
            priority: 1,
            estimated_cost: 0,
        },
    );
    runtime.add_plugin(
        Box::new(p2),
        PluginSchedule {
            priority: 5,
            estimated_cost: 0,
        },
    );

    runtime.tick().unwrap();
    let order = log.lock().unwrap().clone();
    assert_eq!(order, vec![PluginId(2), PluginId(1)]);
}

#[test]
fn tick_increments_context() {
    let mut runtime = Runtime::new(workspace::WorkspaceDefinition {
        name: "test".to_string(),
        description: String::new(),
        target_hz: 1000,
        plugins: Vec::new(),
        connections: Vec::new(),
        settings: workspace::WorkspaceSettings::default(),
    });
    let log = Arc::new(Mutex::new(Vec::new()));
    let ticks = Arc::new(Mutex::new(Vec::new()));

    let p1 = LogPlugin::new(1, log, ticks.clone());
    runtime.add_plugin(
        Box::new(p1),
        PluginSchedule {
            priority: 1,
            estimated_cost: 0,
        },
    );

    runtime.tick().unwrap();
    runtime.tick().unwrap();

    let recorded = ticks.lock().unwrap().clone();
    assert_eq!(recorded, vec![0, 1]);
}
