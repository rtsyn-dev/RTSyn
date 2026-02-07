#[test]
fn gui_config_defaults() {
    let config = rtsyn_gui::GuiConfig::default();
    assert_eq!(config.title, "RTSyn");
    assert_eq!(config.width, 1280.0);
    assert_eq!(config.height, 720.0);
}
