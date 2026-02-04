use rtsyn_plugin::{
    DeviceDriver, Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port, PortId,
};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(feature = "comedi")]
use comedi::{from_phys, get_maxdata, get_range, read, to_phys, write, Comedi, SubdeviceType};

pub struct ComediDaqPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    input_port_names: Vec<String>,
    output_port_names: Vec<String>,

    device_path: String,
    ai_channels: Vec<(u32, u32)>,
    ao_channels: Vec<(u32, u32)>,

    input_values: HashMap<String, f64>,
    output_values: HashMap<String, f64>,

    is_open: bool,
    last_scan_devices: bool,
    last_scan_nonce: u64,
    active_inputs: Vec<bool>,
    active_outputs: Vec<bool>,
    #[cfg(feature = "comedi")]
    dev: Option<Comedi>,
    #[cfg(not(feature = "comedi"))]
    mock_phase: f64,
}

unsafe impl Send for ComediDaqPlugin {}

impl ComediDaqPlugin {
    pub fn new(id: u64) -> Self {
        let mut plugin = Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "Comedi DAQ Device Driver".to_string(),
                fixed_vars: vec![],
                default_vars: vec![
                    ("device_path".to_string(), Value::from("/dev/comedi0")),
                    ("scan_devices".to_string(), Value::from(false)),
                    ("scan_nonce".to_string(), Value::from(0_u64)),
                ],
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            input_port_names: Vec::new(),
            output_port_names: Vec::new(),
            device_path: "/dev/comedi0".to_string(),
            ai_channels: Vec::new(),
            ao_channels: Vec::new(),
            input_values: HashMap::new(),
            output_values: HashMap::new(),
            is_open: false,
            last_scan_devices: false,
            last_scan_nonce: 0,
            active_inputs: Vec::new(),
            active_outputs: Vec::new(),
            #[cfg(feature = "comedi")]
            dev: None,
            #[cfg(not(feature = "comedi"))]
            mock_phase: 0.0,
        };

        plugin.auto_configure();
        plugin
    }

    pub fn set_config(&mut self, device_path: String, scan_devices: bool, scan_nonce: u64) {
        let changed = self.device_path != device_path;
        if changed {
            self.device_path = device_path;
        }
        if changed || (scan_devices && !self.last_scan_devices) || scan_nonce != self.last_scan_nonce
        {
            self.auto_configure();
        }
        self.last_scan_devices = scan_devices;
        self.last_scan_nonce = scan_nonce;
    }

    pub fn set_input(&mut self, port_name: &str, value: f64) {
        self.input_values.insert(port_name.to_string(), value);
    }

    pub fn get_output(&self, port_name: &str) -> f64 {
        self.output_values.get(port_name).copied().unwrap_or(0.0)
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn set_active_ports(
        &mut self,
        input_ports: &std::collections::HashSet<String>,
        output_ports: &std::collections::HashSet<String>,
    ) {
        if self.active_inputs.len() != self.input_port_names.len() {
            self.active_inputs.resize(self.input_port_names.len(), false);
        }
        if self.active_outputs.len() != self.output_port_names.len() {
            self.active_outputs
                .resize(self.output_port_names.len(), false);
        }
        for (idx, name) in self.input_port_names.iter().enumerate() {
            self.active_inputs[idx] = input_ports.contains(name);
        }
        for (idx, name) in self.output_port_names.iter().enumerate() {
            self.active_outputs[idx] = output_ports.contains(name);
        }
    }

    pub fn input_port_names(&self) -> &[String] {
        &self.input_port_names
    }

    pub fn output_port_names(&self) -> &[String] {
        &self.output_port_names
    }

    fn update_ports(&mut self) {
        self.inputs.clear();
        self.outputs.clear();
        self.input_port_names.clear();
        self.output_port_names.clear();
        self.active_inputs.clear();
        self.active_outputs.clear();

        for (sd, ch) in &self.ao_channels {
            let name = format!("ao{sd}_{ch}");
            self.inputs.push(Port {
                id: PortId(name.clone()),
            });
            self.input_port_names.push(name);
            self.active_inputs.push(false);
        }

        for (sd, ch) in &self.ai_channels {
            let name = format!("ai{sd}_{ch}");
            self.outputs.push(Port {
                id: PortId(name.clone()),
            });
            self.output_port_names.push(name);
            self.active_outputs.push(false);
        }
    }

    fn mock_default_channels(&mut self) {
        if self.ai_channels.is_empty() {
            self.ai_channels = vec![(0, 0), (0, 1)];
        }
        if self.ao_channels.is_empty() {
            self.ao_channels = vec![(1, 0)];
        }
    }

    #[cfg(feature = "comedi")]
    fn auto_configure(&mut self) {
        let Ok(dev) = Comedi::open(&self.device_path) else {
            self.mock_default_channels();
            self.update_ports();
            return;
        };

        let mut ai = Vec::new();
        let mut ao = Vec::new();
        let n = dev.get_n_subdevices().unwrap_or(0);
        for sd in 0..n {
            match dev.get_subdevice_type(sd) {
                Ok(SubdeviceType::AnalogInput) => {
                    let ch = dev.get_n_channels(sd).unwrap_or(0);
                    for c in 0..ch {
                        ai.push((sd, c));
                    }
                }
                Ok(SubdeviceType::AnalogOutput) => {
                    let ch = dev.get_n_channels(sd).unwrap_or(0);
                    for c in 0..ch {
                        ao.push((sd, c));
                    }
                }
                _ => {}
            }
        }

        self.ai_channels = ai;
        self.ao_channels = ao;
        if self.ai_channels.is_empty() && self.ao_channels.is_empty() {
            self.mock_default_channels();
        }
        self.update_ports();
    }

    #[cfg(not(feature = "comedi"))]
    fn auto_configure(&mut self) {
        self.mock_default_channels();
        self.update_ports();
    }

    fn comedi_error<E: std::fmt::Display>(err: E) -> PluginError {
        PluginError::Runtime(err.to_string())
    }
}

impl Plugin for ComediDaqPlugin {
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

    #[cfg(feature = "comedi")]
    fn process(&mut self, _ctx: &mut PluginContext) -> Result<(), PluginError> {
        if !self.is_open {
            return Ok(());
        }
        let Some(dev) = self.dev.as_ref() else {
            return Ok(());
        };

        for (idx, (sd, ch)) in self.ao_channels.iter().enumerate() {
            if !self.active_inputs.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let port = format!("ao{sd}_{ch}");
            if let Some(v) = self.input_values.get(&port) {
                let range = get_range(dev, *sd, *ch, 0).map_err(Self::comedi_error)?;
                let max = get_maxdata(dev, *sd, *ch).map_err(Self::comedi_error)?;
                let raw = from_phys(*v, range, max);
                write(dev, *sd, *ch, 0, raw).map_err(Self::comedi_error)?;
            }
        }

        for (idx, (sd, ch)) in self.ai_channels.iter().enumerate() {
            if !self.active_outputs.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let raw = read(dev, *sd, *ch, 0).map_err(Self::comedi_error)?;
            let range = get_range(dev, *sd, *ch, 0).map_err(Self::comedi_error)?;
            let max = get_maxdata(dev, *sd, *ch).map_err(Self::comedi_error)?;
            let phys = to_phys(raw, range, max);

            let port = format!("ai{sd}_{ch}");
            self.output_values.insert(port, phys);
        }

        Ok(())
    }

    #[cfg(not(feature = "comedi"))]
    fn process(&mut self, ctx: &mut PluginContext) -> Result<(), PluginError> {
        if !self.is_open {
            return Ok(());
        }
        let t = ctx.tick as f64 * ctx.period_seconds;
        self.mock_phase = (self.mock_phase + t * 2.0).fract();

        for (idx, (sd, ch)) in self.ai_channels.iter().enumerate() {
            if !self.active_outputs.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let port = format!("ai{sd}_{ch}");
            let value = (self.mock_phase * 6.28318 + idx as f64).sin() * 5.0;
            self.output_values.insert(port, value);
        }

        Ok(())
    }
}

impl DeviceDriver for ComediDaqPlugin {
    #[cfg(feature = "comedi")]
    fn open(&mut self) -> Result<(), PluginError> {
        let dev = Comedi::open(&self.device_path).map_err(Self::comedi_error)?;
        self.dev = Some(dev);
        self.is_open = true;
        Ok(())
    }

    #[cfg(not(feature = "comedi"))]
    fn open(&mut self) -> Result<(), PluginError> {
        self.is_open = true;
        Ok(())
    }

    #[cfg(feature = "comedi")]
    fn close(&mut self) -> Result<(), PluginError> {
        self.dev = None;
        self.is_open = false;
        Ok(())
    }

    #[cfg(not(feature = "comedi"))]
    fn close(&mut self) -> Result<(), PluginError> {
        self.is_open = false;
        Ok(())
    }
}
