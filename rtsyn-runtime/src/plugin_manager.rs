use csv_recorder_plugin::CsvRecorderedPlugin;
use libloading::Library;
use live_plotter_plugin::LivePlotterPlugin;
use performance_monitor_plugin::PerformanceMonitorPlugin;
use rtsyn_plugin::ui::DisplaySchema;
use rtsyn_plugin::{
    Plugin, PluginApi, PluginString, RTSYN_PLUGIN_ABI_VERSION,
    RTSYN_PLUGIN_ABI_VERSION_SYMBOL, RTSYN_PLUGIN_API_SYMBOL,
};
use serde_json::Value;

pub enum RuntimePlugin {
    CsvRecorder(CsvRecorderedPlugin),
    LivePlotter(LivePlotterPlugin),
    PerformanceMonitor(PerformanceMonitorPlugin),
    #[cfg(feature = "comedi")]
    ComediDaq(comedi_daq_plugin::ComediDaqPlugin),
    Dynamic(DynamicPluginInstance),
}

pub struct DynamicPluginInstance {
    pub _lib: Library,
    pub api: *const PluginApi,
    pub handle: *mut std::ffi::c_void,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub input_bytes: Vec<Vec<u8>>,
    pub output_bytes: Vec<Vec<u8>>,
    pub internal_variables: Vec<String>,
    pub internal_variable_bytes: Vec<Vec<u8>>,
    pub input_indices: Option<Vec<i32>>,
    pub output_indices: Option<Vec<i32>>,
    pub last_base_config: Option<Value>,
    pub last_period_seconds: Option<f64>,
    pub last_max_integration_steps: Option<usize>,
    pub last_inputs: Vec<u64>,
}

impl DynamicPluginInstance {
    pub unsafe fn load(path: &str, id: u64) -> Option<Self> {
        let lib = Library::new(path).ok()?;
        let version_symbol: libloading::Symbol<unsafe extern "C" fn() -> u32> = match lib
            .get(RTSYN_PLUGIN_ABI_VERSION_SYMBOL.as_bytes())
        {
            Ok(symbol) => symbol,
            Err(_) => {
                eprintln!(
                        "[RTSyn][ERROR]: Plugin '{}' is incompatible (missing ABI version symbol). Rebuild plugin.",
                        path
                    );
                return None;
            }
        };
        let abi_version = version_symbol();
        if abi_version != RTSYN_PLUGIN_ABI_VERSION {
            eprintln!(
                "[RTSyn][ERROR]: Plugin '{}' ABI version mismatch (plugin={}, runtime={}). Rebuild plugin.",
                path, abi_version, RTSYN_PLUGIN_ABI_VERSION
            );
            return None;
        }
        let symbol: libloading::Symbol<unsafe extern "C" fn() -> *const PluginApi> =
            lib.get(RTSYN_PLUGIN_API_SYMBOL.as_bytes()).ok()?;
        let api_ptr = symbol();
        if api_ptr.is_null() {
            return None;
        }
        let api = api_ptr;
        let handle = ((*api).create)(id);
        if handle.is_null() {
            return None;
        }
        let inputs = Self::read_ports(unsafe { &*api }, handle, unsafe { (*api).inputs_json });
        let outputs = Self::read_ports(unsafe { &*api }, handle, unsafe { (*api).outputs_json });
        let input_bytes: Vec<Vec<u8>> = inputs.iter().map(|v| v.as_bytes().to_vec()).collect();
        let output_bytes: Vec<Vec<u8>> = outputs.iter().map(|v| v.as_bytes().to_vec()).collect();
        let input_indices =
            if ((*api).resolve_input_index).is_some() && ((*api).set_input_by_index).is_some() {
                let resolver = (*api).resolve_input_index?;
                Some(
                    input_bytes
                        .iter()
                        .map(|key| resolver(handle, key.as_ptr(), key.len()))
                        .collect(),
                )
            } else {
                None
            };
        let output_indices =
            if ((*api).resolve_output_index).is_some() && ((*api).get_output_by_index).is_some() {
                let resolver = (*api).resolve_output_index?;
                Some(
                    output_bytes
                        .iter()
                        .map(|key| resolver(handle, key.as_ptr(), key.len()))
                        .collect(),
                )
            } else {
                None
            };
        let display_schema = unsafe { (*api).display_schema_json }.and_then(|schema_fn| {
            let raw = schema_fn(handle);
            if raw.ptr.is_null() || raw.len == 0 {
                return None;
            }
            let json = unsafe { raw.into_string() };
            serde_json::from_str::<DisplaySchema>(&json).ok()
        });
        let internal_variables = display_schema
            .as_ref()
            .map(|schema| schema.variables.clone())
            .unwrap_or_default();
        let internal_variable_bytes = internal_variables
            .iter()
            .map(|v| v.as_bytes().to_vec())
            .collect();
        let last_inputs = vec![f64::NAN.to_bits(); inputs.len()];
        Some(Self {
            _lib: lib,
            api,
            handle,
            inputs,
            outputs,
            input_bytes,
            output_bytes,
            internal_variables,
            internal_variable_bytes,
            input_indices,
            output_indices,
            last_base_config: None,
            last_period_seconds: None,
            last_max_integration_steps: None,
            last_inputs,
        })
    }

    unsafe fn read_ports(
        _api: &PluginApi,
        handle: *mut std::ffi::c_void,
        fetch: extern "C" fn(*mut std::ffi::c_void) -> PluginString,
    ) -> Vec<String> {
        let raw = fetch(handle);
        if raw.ptr.is_null() || raw.len == 0 {
            return Vec::new();
        }
        let json = unsafe { raw.into_string() };
        serde_json::from_str::<Vec<String>>(&json).unwrap_or_default()
    }
}

pub fn runtime_plugin_loads_started(instance: &RuntimePlugin) -> bool {
    match instance {
        RuntimePlugin::CsvRecorder(p) => p.behavior().loads_started,
        RuntimePlugin::LivePlotter(p) => p.behavior().loads_started,
        RuntimePlugin::PerformanceMonitor(p) => p.behavior().loads_started,
        #[cfg(feature = "comedi")]
        RuntimePlugin::ComediDaq(p) => p.behavior().loads_started,
        RuntimePlugin::Dynamic(dynamic) => {
            let api = unsafe { &*dynamic.api };
            let Some(behavior_json_fn) = api.behavior_json else {
                return false;
            };
            let json_str = behavior_json_fn(dynamic.handle);
            if json_str.ptr.is_null() || json_str.len == 0 {
                return false;
            }
            let json = unsafe { json_str.into_string() };
            serde_json::from_str::<rtsyn_plugin::ui::PluginBehavior>(&json)
                .map(|b| b.loads_started)
                .unwrap_or(false)
        }
    }
}

pub fn set_dynamic_config_if_needed(
    plugin_instance: &mut DynamicPluginInstance,
    plugin_config: &Value,
    period_seconds: f64,
    max_integration_steps: usize,
) {
    let needs_update = plugin_instance
        .last_base_config
        .as_ref()
        .map(|last| last != plugin_config)
        .unwrap_or(true)
        || plugin_instance
            .last_period_seconds
            .map(|last| (last - period_seconds).abs() > f64::EPSILON)
            .unwrap_or(true)
        || plugin_instance
            .last_max_integration_steps
            .map(|last| last != max_integration_steps)
            .unwrap_or(true);
    if !needs_update {
        return;
    }

    let Some(map) = plugin_config.as_object() else {
        return;
    };
    let mut out = map.clone();
    out.insert("period_seconds".to_string(), Value::from(period_seconds));
    out.insert(
        "max_integration_steps".to_string(),
        Value::from(max_integration_steps as f64),
    );
    let json = Value::Object(out).to_string();
    let api = unsafe { &*plugin_instance.api };
    (api.set_config_json)(
        plugin_instance.handle,
        json.as_bytes().as_ptr(),
        json.as_bytes().len(),
    );
    plugin_instance.last_base_config = Some(plugin_config.clone());
    plugin_instance.last_period_seconds = Some(period_seconds);
    plugin_instance.last_max_integration_steps = Some(max_integration_steps);
}

pub fn set_dynamic_config_patch(plugin_instance: &mut DynamicPluginInstance, key: &str, value: Value) {
    let api = unsafe { &*plugin_instance.api };
    let mut patch = serde_json::Map::new();
    patch.insert(key.to_string(), value.clone());
    let json = Value::Object(patch).to_string();
    (api.set_config_json)(
        plugin_instance.handle,
        json.as_bytes().as_ptr(),
        json.as_bytes().len(),
    );

    if let Some(Value::Object(ref mut cfg)) = plugin_instance.last_base_config {
        cfg.insert(key.to_string(), value);
    }
}