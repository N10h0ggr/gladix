use shared::config::*;
use prost::Message;

#[test]
fn test_scanner_config_roundtrip() {
    let original = ScannerConfig {
        enabled: true,
        interval_seconds: 300,
        recursive: false,
        file_extensions: ".exe,.dll".to_string(),
        paths: vec!["C:\\Temp".to_string(), "C:\\Downloads".to_string()],
    };

    let encoded = prost::Message::encode_to_vec(&original);
    let decoded = ScannerConfig::decode(&*encoded).expect("decode failed");

    assert_eq!(decoded.enabled, true);
    assert_eq!(decoded.interval_seconds, 300);
    assert_eq!(decoded.file_extensions, ".exe,.dll");
    assert_eq!(decoded.paths.len(), 2);
    assert_eq!(decoded.paths[0], "C:\\Temp");
}

#[test]
fn test_config_union_serialization() {
    let config = ConfigUpdate {
        scanner: Some(ScannerConfig {
            enabled: true,
            interval_seconds: 600,
            recursive: true,
            file_extensions: ".bat,.vbs".into(),
            paths: vec!["D:\\Scripts".into()],
        }),
        process: Some(ProcessConfig {
            enabled: true,
            hook_creation: true,
            hook_termination: false,
            detect_remote_threads: true,
        }),
        fs: Some(FsConfig {
            enabled: true,
            filter_mask: 0x07,
            path_whitelist: vec!["C:\\Users".into()],
            path_blacklist: vec!["C:\\Temp".into()],
        }),
        network: None,
        etw: None,
    };

    let encoded = prost::Message::encode_to_vec(&config);
    let decoded = ConfigUpdate::decode(&*encoded).expect("decode failed");

    assert!(decoded.scanner.is_some());
    assert_eq!(decoded.scanner.unwrap().paths[0], "D:\\Scripts");
    assert!(decoded.process.unwrap().detect_remote_threads);
}
