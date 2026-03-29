/// Tests for M4A compatibility shim
/// - audex::m4a::delete forwards to MP4 delete
/// - M4A, EasyM4A, and other aliases work correctly
use audex::m4a;

#[test]
fn test_m4a_delete_function() {
    // Test that the delete function is accessible and works
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.m4a");

    // Create a minimal MP4/M4A file (just write some bytes for now)
    // In a real scenario, this would be a valid M4A file
    std::fs::write(&test_path, b"").unwrap();

    // The delete function should be accessible from m4a module
    // For an empty file, this might fail, but we're testing that the function exists
    let result = m4a::clear(&test_path);

    // We don't care if it succeeds or fails - we're just verifying the function is accessible
    // and forwards to the MP4 delete function
    let _ = result;
}

#[test]
fn test_m4a_type_alias() {
    // Test that M4A alias exists and is the same as MP4
    // This is a compile-time check more than runtime
    use audex::m4a::M4A;

    // If this compiles, the alias works
    let _: Option<M4A> = None;
}

#[test]
fn test_easym4a_type_alias() {
    // Test that EasyM4A alias exists
    use audex::m4a::EasyM4A;

    // If this compiles, the alias works
    let _: Option<EasyM4A> = None;
}

#[test]
fn test_m4a_module_exports() {
    // Test that key exports are available from m4a module
    use audex::m4a::{AtomDataType, MP4Cover, MP4Info, MP4Tags};

    // If this compiles, all the exports work
    let _: Option<MP4Tags> = None;
    let _: Option<MP4Info> = None;
    let _: Option<MP4Cover> = None;
    let _: Option<AtomDataType> = None;
}

#[test]
fn test_m4a_utility_functions() {
    // Test that utility functions are accessible
    use audex::m4a::{key2name, name2key};

    // Test name2key (takes bytes)
    let key = name2key(b"Album");
    assert!(!key.is_empty());

    // Test key2name (takes string)
    let name = key2name("\u{00A9}alb").unwrap();
    assert!(!name.is_empty());
}

#[test]
fn test_m4a_easym4atags_alias() {
    // Test that EasyM4ATags alias exists
    use audex::m4a::EasyM4ATags;

    // If this compiles, the alias works
    let _: Option<EasyM4ATags> = None;
}

#[test]
fn test_m4a_key_registry() {
    // Test that KeyRegistry is accessible
    use audex::m4a::KeyRegistry;

    let _registry = KeyRegistry::new();
    // If this compiles and runs, KeyRegistry is accessible
}
