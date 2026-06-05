use deva_light::remote::detect_local_addresses;

#[test]
fn detect_local_addresses_returns_at_least_one_entry() {
    let addresses = detect_local_addresses();
    assert!(!addresses.is_empty());
}
