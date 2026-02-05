use csv_recorder_plugin::CsvRecorderedPlugin;
use live_plotter_plugin::LivePlotterPlugin;
use rtsyn_plugin::prelude::*;

#[test]
fn csv_recorder_has_ui_schema() {
    let plugin = CsvRecorderedPlugin::new(1);
    let schema = plugin.ui_schema().expect("CSV recorder should have UI schema");

    assert_eq!(schema.fields.len(), 4);

    // Check separator field
    assert_eq!(schema.fields[0].key, "separator");
    assert_eq!(schema.fields[0].label, "Separator");

    // Check include_time field
    assert_eq!(schema.fields[1].key, "include_time");

    // Check path field
    assert_eq!(schema.fields[2].key, "path");
    if let FieldType::FilePath { mode, .. } = schema.fields[2].field_type {
        assert_eq!(mode, FileMode::SaveFile);
    } else {
        panic!("Expected FilePath field type");
    }

    // Check columns field
    assert_eq!(schema.fields[3].key, "columns");
    if let FieldType::DynamicList { .. } = schema.fields[3].field_type {
        // OK
    } else {
        panic!("Expected DynamicList field type");
    }
}

#[test]
fn csv_recorder_behavior() {
    let plugin = CsvRecorderedPlugin::new(1);
    let behavior = plugin.behavior();

    assert!(behavior.supports_start_stop);
    assert!(!behavior.supports_restart);
    assert_eq!(
        behavior.extendable_inputs,
        ExtendableInputs::Auto {
            pattern: "in_{}".to_string()
        }
    );
    assert!(!behavior.loads_started);

    let conn_behavior = plugin.connection_behavior();
    assert!(conn_behavior.dependent);
}

#[test]
fn csv_recorder_dynamic_inputs() {
    let mut plugin = CsvRecorderedPlugin::new(1);

    assert_eq!(plugin.inputs().len(), 0);

    // Add inputs
    plugin.on_input_added("in_0").unwrap();
    assert_eq!(plugin.inputs().len(), 1);

    plugin.on_input_added("in_1").unwrap();
    assert_eq!(plugin.inputs().len(), 2);

    plugin.on_input_added("in_2").unwrap();
    assert_eq!(plugin.inputs().len(), 3);

    // Remove middle input
    plugin.on_input_removed("in_1").unwrap();
    assert_eq!(plugin.inputs().len(), 2);
    assert_eq!(plugin.inputs()[0].id.0, "in_0");
    assert_eq!(plugin.inputs()[1].id.0, "in_1"); // Reindexed from in_2
}

#[test]
fn live_plotter_has_ui_schema() {
    let plugin = LivePlotterPlugin::new(1);
    let schema = plugin.ui_schema().expect("Live plotter should have UI schema");

    assert_eq!(schema.fields.len(), 4);

    // Check refresh_hz field
    assert_eq!(schema.fields[0].key, "refresh_hz");
    if let FieldType::Float { min, max, .. } = schema.fields[0].field_type {
        assert_eq!(min, Some(1.0));
        assert_eq!(max, Some(120.0));
    } else {
        panic!("Expected Float field type");
    }

    // Check window_multiplier field
    assert_eq!(schema.fields[1].key, "window_multiplier");

    // Check window_value field
    assert_eq!(schema.fields[2].key, "window_value");

    // Check amplitude field
    assert_eq!(schema.fields[3].key, "amplitude");
}

#[test]
fn live_plotter_behavior() {
    let plugin = LivePlotterPlugin::new(1);
    let behavior = plugin.behavior();

    assert!(behavior.supports_start_stop);
    assert!(!behavior.supports_restart);
    assert_eq!(
        behavior.extendable_inputs,
        ExtendableInputs::Auto {
            pattern: "in_{}".to_string()
        }
    );
    assert!(!behavior.loads_started);

    let conn_behavior = plugin.connection_behavior();
    assert!(conn_behavior.dependent);
}

#[test]
fn live_plotter_dynamic_inputs() {
    let mut plugin = LivePlotterPlugin::new(1);

    assert_eq!(plugin.inputs().len(), 0);

    // Add inputs
    plugin.on_input_added("in_0").unwrap();
    assert_eq!(plugin.inputs().len(), 1);

    plugin.on_input_added("in_1").unwrap();
    assert_eq!(plugin.inputs().len(), 2);

    // Remove input
    plugin.on_input_removed("in_0").unwrap();
    assert_eq!(plugin.inputs().len(), 1);
    assert_eq!(plugin.inputs()[0].id.0, "in_0"); // Reindexed from in_1
}

#[test]
fn ui_schema_json_serialization() {
    let plugin = CsvRecorderedPlugin::new(1);
    let schema = plugin.ui_schema().unwrap();

    // Serialize to JSON
    let json = serde_json::to_string(&schema).expect("Should serialize");
    assert!(json.contains("separator"));
    assert!(json.contains("include_time"));
    assert!(json.contains("path"));
    assert!(json.contains("columns"));

    // Deserialize back
    let deserialized: UISchema = serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(deserialized.fields.len(), 4);
}

#[test]
fn behavior_json_serialization() {
    let plugin = CsvRecorderedPlugin::new(1);
    let behavior = plugin.behavior();

    // Serialize to JSON
    let json = serde_json::to_string(&behavior).expect("Should serialize");
    assert!(json.contains("extendable_inputs"));
    assert!(json.contains("auto"));
    assert!(json.contains("in_{}"));

    // Deserialize back
    let deserialized: PluginBehavior = serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(deserialized, behavior);
}
