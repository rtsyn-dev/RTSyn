use rtsyn_plugin::{
    DeviceDriver, Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port, PortId,
};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(feature = "hardware")]
use std::ffi::CString;
#[cfg(feature = "hardware")]
use std::ptr;
#[cfg(feature = "hardware")]
use ni_daqmx_sys;

// Real-time optimized NI DAQ plugin
pub struct NiDaqPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    
    // DAQ configuration
    device_name: String,
    sample_rate: f64,
    samples_per_channel: u32,
    
    // Channel management - pre-allocated for real-time
    analog_input_channels: Vec<String>,
    analog_output_channels: Vec<String>,
    digital_input_channels: Vec<String>,
    digital_output_channels: Vec<String>,
    
    // DAQ handles - kept open for real-time performance
    #[cfg(feature = "hardware")]
    ai_task_handle: Option<*mut std::ffi::c_void>,
    #[cfg(feature = "hardware")]
    ao_task_handle: Option<*mut std::ffi::c_void>,
    #[cfg(feature = "hardware")]
    di_task_handle: Option<*mut std::ffi::c_void>,
    #[cfg(feature = "hardware")]
    do_task_handle: Option<*mut std::ffi::c_void>,
    
    // Pre-allocated buffers for real-time operation
    ai_buffer: Vec<f64>,
    ao_buffer: Vec<f64>,
    di_buffer: Vec<u32>,
    do_buffer: Vec<u32>,
    
    // Input/output value caches
    input_values: HashMap<String, f64>,
    output_values: HashMap<String, f64>,
    
    // State
    is_open: bool,
    channels_configured: bool,
}

// SAFETY: DAQ handles are thread-safe according to NI-DAQmx documentation
unsafe impl Send for NiDaqPlugin {}

impl NiDaqPlugin {
    pub fn new(id: u64) -> Self {
        Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "NI DAQ Device Driver".to_string(),
                fixed_vars: vec![
                    ("device_name".to_string(), Value::from("Dev1")),
                ],
                default_vars: vec![
                    ("sample_rate".to_string(), Value::from(10000.0)),
                    ("samples_per_channel".to_string(), Value::from(1000)),
                    ("ai_channels".to_string(), Value::from("")),
                    ("ao_channels".to_string(), Value::from("")),
                    ("di_channels".to_string(), Value::from("")),
                    ("do_channels".to_string(), Value::from("")),
                ],
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            device_name: "Dev1".to_string(),
            sample_rate: 10000.0,
            samples_per_channel: 1000,
            analog_input_channels: Vec::new(),
            analog_output_channels: Vec::new(),
            digital_input_channels: Vec::new(),
            digital_output_channels: Vec::new(),
            #[cfg(feature = "hardware")]
            ai_task_handle: None,
            #[cfg(feature = "hardware")]
            ao_task_handle: None,
            #[cfg(feature = "hardware")]
            di_task_handle: None,
            #[cfg(feature = "hardware")]
            do_task_handle: None,
            ai_buffer: Vec::new(),
            ao_buffer: Vec::new(),
            di_buffer: Vec::new(),
            do_buffer: Vec::new(),
            input_values: HashMap::new(),
            output_values: HashMap::new(),
            is_open: false,
            channels_configured: false,
        }
    }

    pub fn set_config(
        &mut self,
        device_name: String,
        sample_rate: f64,
        samples_per_channel: u32,
        ai_channels: Vec<String>,
        ao_channels: Vec<String>,
        di_channels: Vec<String>,
        do_channels: Vec<String>,
    ) {
        // Close existing tasks if configuration changes
        if self.device_name != device_name 
            || self.analog_input_channels != ai_channels
            || self.analog_output_channels != ao_channels
            || self.digital_input_channels != di_channels
            || self.digital_output_channels != do_channels {
            self.close_tasks();
            self.channels_configured = false;
        }

        self.device_name = device_name;
        self.sample_rate = sample_rate;
        self.samples_per_channel = samples_per_channel;
        self.analog_input_channels = ai_channels;
        self.analog_output_channels = ao_channels;
        self.digital_input_channels = di_channels;
        self.digital_output_channels = do_channels;

        // Update ports
        self.update_ports();
        
        // Pre-allocate buffers for real-time performance
        self.ai_buffer.resize(self.analog_input_channels.len() * self.samples_per_channel as usize, 0.0);
        self.ao_buffer.resize(self.analog_output_channels.len() * self.samples_per_channel as usize, 0.0);
        self.di_buffer.resize(self.digital_input_channels.len(), 0);
        self.do_buffer.resize(self.digital_output_channels.len(), 0);
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

        // Add digital output channels as inputs
        for channel in &self.digital_output_channels {
            self.inputs.push(Port {
                id: PortId(format!("do_{}", channel)),
            });
        }

        // Add analog input channels as outputs (values read)
        for channel in &self.analog_input_channels {
            self.outputs.push(Port {
                id: PortId(format!("ai_{}", channel)),
            });
        }

        // Add digital input channels as outputs
        for channel in &self.digital_input_channels {
            self.outputs.push(Port {
                id: PortId(format!("di_{}", channel)),
            });
        }
    }

    fn close_tasks(&mut self) {
        #[cfg(feature = "hardware")]
        unsafe {
            if let Some(handle) = self.ai_task_handle.take() {
                ni_daqmx_sys::DAQmxStopTask(handle);
                ni_daqmx_sys::DAQmxClearTask(handle);
            }
            if let Some(handle) = self.ao_task_handle.take() {
                ni_daqmx_sys::DAQmxStopTask(handle);
                ni_daqmx_sys::DAQmxClearTask(handle);
            }
            if let Some(handle) = self.di_task_handle.take() {
                ni_daqmx_sys::DAQmxStopTask(handle);
                ni_daqmx_sys::DAQmxClearTask(handle);
            }
            if let Some(handle) = self.do_task_handle.take() {
                ni_daqmx_sys::DAQmxStopTask(handle);
                ni_daqmx_sys::DAQmxClearTask(handle);
            }
        }
        #[cfg(feature = "mock")]
        {
            // Mock: No actual hardware to close
        }
    }

    fn create_ai_task(&mut self) -> Result<(), PluginError> {
        if self.analog_input_channels.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "hardware")]
        unsafe {
            let mut task_handle: ni_daqmx_sys::TaskHandle = ptr::null_mut();
            
            // Create task
            let task_name = CString::new("AI_Task").map_err(|_| PluginError::ProcessingFailed)?;
            let result = ni_daqmx_sys::DAQmxCreateTask(task_name.as_ptr(), &mut task_handle);
            if result != 0 {
                return Err(PluginError::ProcessingFailed);
            }

            // Add channels
            for channel in &self.analog_input_channels {
                let channel_name = CString::new(format!("{}/{}", self.device_name, channel))
                    .map_err(|_| PluginError::ProcessingFailed)?;
                let result = ni_daqmx_sys::DAQmxCreateAIVoltageChan(
                    task_handle,
                    channel_name.as_ptr(),
                    ptr::null(),
                    ni_daqmx_sys::DAQmx_Val_Cfg_Default.into(),
                    -10.0, // Min value
                    10.0,  // Max value
                    ni_daqmx_sys::DAQmx_Val_Volts.into(),
                    ptr::null(),
                );
                if result != 0 {
                    ni_daqmx_sys::DAQmxClearTask(task_handle);
                    return Err(PluginError::ProcessingFailed);
                }
            }

            // Configure timing for real-time
            let result = ni_daqmx_sys::DAQmxCfgSampClkTiming(
                task_handle,
                ptr::null(),
                self.sample_rate,
                ni_daqmx_sys::DAQmx_Val_Rising.into(),
                ni_daqmx_sys::DAQmx_Val_FiniteSamps.into(),
                self.samples_per_channel as u64,
            );
            if result != 0 {
                ni_daqmx_sys::DAQmxClearTask(task_handle);
                return Err(PluginError::ProcessingFailed);
            }

            self.ai_task_handle = Some(task_handle);
        }
        
        #[cfg(feature = "mock")]
        {
            // Mock: Simulate successful task creation
        }
        
        Ok(())
    }

    fn create_ao_task(&mut self) -> Result<(), PluginError> {
        if self.analog_output_channels.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "hardware")]
        unsafe {
            let mut task_handle: ni_daqmx_sys::TaskHandle = ptr::null_mut();
            
            let task_name = CString::new("AO_Task").map_err(|_| PluginError::ProcessingFailed)?;
            let result = ni_daqmx_sys::DAQmxCreateTask(task_name.as_ptr(), &mut task_handle);
            if result != 0 {
                return Err(PluginError::ProcessingFailed);
            }

            for channel in &self.analog_output_channels {
                let channel_name = CString::new(format!("{}/{}", self.device_name, channel))
                    .map_err(|_| PluginError::ProcessingFailed)?;
                let result = ni_daqmx_sys::DAQmxCreateAOVoltageChan(
                    task_handle,
                    channel_name.as_ptr(),
                    ptr::null(),
                    -10.0,
                    10.0,
                    ni_daqmx_sys::DAQmx_Val_Volts.into(),
                    ptr::null(),
                );
                if result != 0 {
                    ni_daqmx_sys::DAQmxClearTask(task_handle);
                    return Err(PluginError::ProcessingFailed);
                }
            }

            self.ao_task_handle = Some(task_handle);
        }
        
        #[cfg(feature = "mock")]
        {
            // Mock: Simulate successful task creation
        }
        
        Ok(())
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

        // Real-time critical section - minimize allocations and syscalls
        
        // Read analog inputs
        #[cfg(feature = "hardware")]
        if let Some(handle) = self.ai_task_handle {
            unsafe {
                let mut samples_read = 0i64;
                let result = ni_daqmx_sys::DAQmxReadAnalogF64(
                    handle,
                    (self.samples_per_channel as i32).into(),
                    10.0, // Timeout
                    ni_daqmx_sys::DAQmx_Val_GroupByChannel.try_into().unwrap(),
                    self.ai_buffer.as_mut_ptr(),
                    (self.ai_buffer.len() as u32).into(),
                    &mut samples_read as *mut i64,
                    ptr::null_mut(),
                );
                
                if result == 0 && samples_read > 0 {
                    // Update output values with latest sample from each channel
                    for (i, channel) in self.analog_input_channels.iter().enumerate() {
                        let port_name = format!("ai_{}", channel);
                        let sample_idx = i * self.samples_per_channel as usize + (samples_read as usize - 1);
                        if sample_idx < self.ai_buffer.len() {
                            self.output_values.insert(port_name, self.ai_buffer[sample_idx]);
                        }
                    }
                }
            }
        }

        #[cfg(feature = "mock")]
        {
            // Mock: Generate simulated data for analog inputs
            for (i, channel) in self.analog_input_channels.iter().enumerate() {
                let port_name = format!("ai_{}", channel);
                // Simulate sine wave data
                let value = (i as f64 * 0.1).sin() * 5.0;
                self.output_values.insert(port_name, value);
            }
        }

        // Write analog outputs
        #[cfg(feature = "hardware")]
        if let Some(handle) = self.ao_task_handle {
            // Update buffer with input values
            for (i, channel) in self.analog_output_channels.iter().enumerate() {
                let port_name = format!("ao_{}", channel);
                if let Some(&value) = self.input_values.get(&port_name) {
                    // Fill entire buffer with the same value for continuous output
                    for j in 0..self.samples_per_channel as usize {
                        let idx = i * self.samples_per_channel as usize + j;
                        if idx < self.ao_buffer.len() {
                            self.ao_buffer[idx] = value;
                        }
                    }
                }
            }

            unsafe {
                let mut samples_written = 0i64;
                ni_daqmx_sys::DAQmxWriteAnalogF64(
                    handle,
                    (self.samples_per_channel as i32).into(),
                    (false as u32).into(), // Auto start
                    10.0, // Timeout
                    ni_daqmx_sys::DAQmx_Val_GroupByChannel.try_into().unwrap(),
                    self.ao_buffer.as_ptr(),
                    &mut samples_written as *mut i64,
                    ptr::null_mut(),
                );
            }
        }

        #[cfg(feature = "mock")]
        {
            // Mock: Just acknowledge the output values were received
            for channel in &self.analog_output_channels {
                let port_name = format!("ao_{}", channel);
                if let Some(_value) = self.input_values.get(&port_name) {
                    // Mock: Output acknowledged
                }
            }
        }

        Ok(())
    }
}

impl DeviceDriver for NiDaqPlugin {
    fn open(&mut self) -> Result<(), PluginError> {
        if self.is_open {
            return Ok(());
        }

        // Create tasks for enabled channels
        self.create_ai_task()?;
        self.create_ao_task()?;

        // Start tasks
        #[cfg(feature = "hardware")]
        unsafe {
            if let Some(handle) = self.ai_task_handle {
                let result = ni_daqmx_sys::DAQmxStartTask(handle);
                if result != 0 {
                    return Err(PluginError::ProcessingFailed);
                }
            }
            if let Some(handle) = self.ao_task_handle {
                let result = ni_daqmx_sys::DAQmxStartTask(handle);
                if result != 0 {
                    return Err(PluginError::ProcessingFailed);
                }
            }
        }

        #[cfg(feature = "mock")]
        {
            // Mock: Simulate successful task start
        }

        self.is_open = true;
        self.channels_configured = true;
        Ok(())
    }

    fn close(&mut self) -> Result<(), PluginError> {
        if !self.is_open {
            return Ok(());
        }

        self.close_tasks();
        self.is_open = false;
        Ok(())
    }
}
