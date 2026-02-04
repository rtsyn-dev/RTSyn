use rtsyn_plugin::{
    DeviceDriver, Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port, PortId,
};
use serde_json::Value;
use std::collections::HashMap;

// Simple mock NI DAQ plugin for now
pub struct NiDaqPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    
    // DAQ configuration
    device_name: String,
    sample_rate: f64,
    samples_per_channel: u32,
    
    // Channel management
    analog_input_channels: Vec<String>,
    analog_output_channels: Vec<String>,
    
    // Input/output value caches
    input_values: HashMap<String, f64>,
    output_values: HashMap<String, f64>,
    
    // State
    is_open: bool,
}

unsafe impl Send for NiDaqPlugin {}

impl NiDaqPlugin {
    pub fn new(id: u64) -> Self {
        println!("NI DAQ: Creating new plugin instance with id {}", id);
        
        let mut plugin = Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "NI DAQ Device Driver".to_string(),
                fixed_vars: vec![],
                default_vars: vec![
                    ("device_name".to_string(), Value::from("Dev1")),
                    ("sample_rate".to_string(), Value::from(10000.0)),
                    ("samples_per_channel".to_string(), Value::from(1000)),
                    ("ai_channels".to_string(), Value::from("")),
                    ("ao_channels".to_string(), Value::from("")),
                    ("scan_devices".to_string(), Value::from(false)),
                ],
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            device_name: "Dev1".to_string(),
            sample_rate: 10000.0,
            samples_per_channel: 1000,
            analog_input_channels: Vec::new(),
            analog_output_channels: Vec::new(),
            input_values: HashMap::new(),
            output_values: HashMap::new(),
            is_open: false,
        };
        
        // Auto-discover channels on creation
        println!("NI DAQ: Auto-configuring plugin...");
        plugin.auto_configure();
        println!("NI DAQ: Plugin created with {} inputs, {} outputs", 
                plugin.inputs.len(), plugin.outputs.len());
        
        plugin
    }

    pub fn set_config(
        &mut self,
        device_name: String,
        sample_rate: f64,
        samples_per_channel: u32,
        ai_channels: Vec<String>,
        ao_channels: Vec<String>,
        _di_channels: Vec<String>,
        _do_channels: Vec<String>,
    ) {
        self.device_name = device_name;
        self.sample_rate = sample_rate;
        self.samples_per_channel = samples_per_channel;
        self.analog_input_channels = ai_channels;
        self.analog_output_channels = ao_channels;

        // Update ports
        self.update_ports();
    }

    pub fn handle_scan_trigger(&mut self) -> bool {
        // Trigger device and channel discovery
        let devices = Self::discover_devices();
        if let Some(device) = devices.first() {
            let (ai_channels, ao_channels) = Self::discover_channels(device);
            
            // Update configuration with discovered channels
            self.set_config(
                device.clone(),
                self.sample_rate,
                self.samples_per_channel,
                ai_channels,
                ao_channels,
                Vec::new(),
                Vec::new(),
            );
            
            // Return true to indicate configuration changed
            return true;
        }
        false
    }

    fn update_ports(&mut self) {
        self.inputs.clear();
        self.outputs.clear();

        // Add analog output channels as inputs (values to write)
        for channel in &self.analog_output_channels {
            self.inputs.push(Port {
                id: PortId(format!("ao_{}", channel)),
            });
        }

        // Add analog input channels as outputs (values read)
        for channel in &self.analog_input_channels {
            self.outputs.push(Port {
                id: PortId(format!("ai_{}", channel)),
            });
        }
    }

    // Device discovery - mock for now, can be replaced with real NI-DAQmx calls later
    pub fn discover_devices() -> Vec<String> {
        // Try to detect if we have NI hardware by checking for nidaqmxconfig
        if std::process::Command::new("nidaqmxconfig").arg("--help").output().is_ok() {
            vec!["Dev1".to_string(), "PCI-6251".to_string()]
        } else {
            vec!["Dev1".to_string(), "SimDev1".to_string()]
        }
    }

    pub fn discover_channels(device_name: &str) -> (Vec<String>, Vec<String>) {
        let ai_channels = match device_name {
            "PCI-6251" => (0..16).map(|i| format!("ai{}", i)).collect(),
            "Dev1" => vec!["ai0".to_string(), "ai1".to_string(), "ai2".to_string(), "ai3".to_string()],
            _ => vec!["ai0".to_string(), "ai1".to_string()],
        };
        
        let ao_channels = match device_name {
            "PCI-6251" => vec!["ao0".to_string(), "ao1".to_string()],
            "Dev1" => vec!["ao0".to_string(), "ao1".to_string()],
            _ => vec!["ao0".to_string()],
        };

        (ai_channels, ao_channels)
    }

    pub fn auto_configure(&mut self) {
        println!("NI DAQ: Starting auto-configuration...");
        let devices = Self::discover_devices();
        println!("NI DAQ: Discovered devices: {:?}", devices);
        
        if let Some(device) = devices.first() {
            let (ai_channels, ao_channels) = Self::discover_channels(device);
            println!("NI DAQ: Device '{}' has {} AI channels, {} AO channels", 
                    device, ai_channels.len(), ao_channels.len());
            
            self.set_config(
                device.clone(),
                self.sample_rate,
                self.samples_per_channel,
                ai_channels,
                ao_channels,
                Vec::new(),
                Vec::new(),
            );
        } else {
            println!("NI DAQ: No devices discovered");
        }
    }

    pub fn set_input(&mut self, port_name: &str, value: f64) {
        self.input_values.insert(port_name.to_string(), value);
    }

    pub fn get_output(&self, port_name: &str) -> f64 {
        self.output_values.get(port_name).copied().unwrap_or(0.0)
    }
}

impl Plugin for NiDaqPlugin {
    fn id(&self) -> PluginId {
        self.id
    }

    fn meta(&self) -> &PluginMeta {
        &self.meta
    }

    fn inputs(&self) -> &[Port] {
        &self.inputs
    }

    fn outputs(&self) -> &[Port] {
        &self.outputs
    }

    fn process(&mut self, _ctx: &mut PluginContext) -> Result<(), PluginError> {
        if !self.is_open {
            return Ok(());
        }

        // Mock: Generate simulated data for analog inputs
        for (i, channel) in self.analog_input_channels.iter().enumerate() {
            let port_name = format!("ai_{}", channel);
            // Simulate sine wave data with different frequencies per channel
            let value = (i as f64 * 0.5 + std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64() * 2.0)
                .sin() * 5.0;
            self.output_values.insert(port_name, value);
        }

        Ok(())
    }
}

impl DeviceDriver for NiDaqPlugin {
    fn open(&mut self) -> Result<(), PluginError> {
        self.is_open = true;
        Ok(())
    }

    fn close(&mut self) -> Result<(), PluginError> {
        self.is_open = false;
        Ok(())
    }
}
