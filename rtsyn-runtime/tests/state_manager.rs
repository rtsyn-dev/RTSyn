use rtsyn_runtime::state_manager::RuntimeState;
use std::collections::HashMap;

#[test]
fn test_new() {
    let state = RuntimeState::new();
    assert!(state.outputs.is_empty());
    assert!(state.input_values.is_empty());
    assert!(state.internal_variable_values.is_empty());
    assert!(state.viewer_values.is_empty());
    assert_eq!(state.tick_count, 0);
}

#[test]
fn test_clear() {
    let mut state = RuntimeState::new();
    
    // Add data
    state.outputs.insert((1, "out".to_string()), 1.0);
    state.input_values.insert((1, "in".to_string()), 2.0);
    state.internal_variable_values.insert((1, "var".to_string()), serde_json::Value::from(3.0));
    state.viewer_values.insert((1, "view".to_string()), serde_json::Value::from(4.0));
    state.tick_count = 10;
    
    state.clear();
    
    assert!(state.outputs.is_empty());
    assert!(state.input_values.is_empty());
    assert!(state.internal_variable_values.is_empty());
    assert!(state.viewer_values.is_empty());
    assert_eq!(state.tick_count, 0);
}

#[test]
fn test_update_tick() {
    let mut state = RuntimeState::new();
    
    state.update_tick();
    assert_eq!(state.tick_count, 1);
    
    state.update_tick();
    assert_eq!(state.tick_count, 2);
}

#[test]
fn test_update_tick_wrapping() {
    let mut state = RuntimeState::new();
    state.tick_count = u64::MAX;
    
    state.update_tick();
    assert_eq!(state.tick_count, 0);
}

#[test]
fn test_hashmap_operations() {
    let mut state = RuntimeState::new();
    
    // Test outputs
    state.outputs.insert((1, "output1".to_string()), 10.5);
    state.outputs.insert((2, "output2".to_string()), 20.5);
    assert_eq!(state.outputs.get(&(1, "output1".to_string())), Some(&10.5));
    assert_eq!(state.outputs.len(), 2);
    
    // Test input_values
    state.input_values.insert((1, "input1".to_string()), 30.5);
    assert_eq!(state.input_values.get(&(1, "input1".to_string())), Some(&30.5));
    
    // Test internal_variable_values
    state.internal_variable_values.insert((1, "var1".to_string()), serde_json::Value::from(40.5));
    assert_eq!(state.internal_variable_values.get(&(1, "var1".to_string())), Some(&serde_json::Value::from(40.5)));
    
    // Test viewer_values
    state.viewer_values.insert((1, "view1".to_string()), serde_json::Value::from("test"));
    assert_eq!(state.viewer_values.get(&(1, "view1".to_string())), Some(&serde_json::Value::from("test")));
}