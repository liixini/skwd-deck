use super::*;

const DISCRETE: vk::PhysicalDeviceType = vk::PhysicalDeviceType::DISCRETE_GPU;
const INTEGRATED: vk::PhysicalDeviceType = vk::PhysicalDeviceType::INTEGRATED_GPU;
const SOFTWARE: vk::PhysicalDeviceType = vk::PhysicalDeviceType::CPU;
const VIRTUAL: vk::PhysicalDeviceType = vk::PhysicalDeviceType::VIRTUAL_GPU;

fn id(byte: u8) -> Uuid {
    [byte; GPU_UUID_BYTES]
}

#[test]
fn dawn_adapter_order() {
    let listed = [(id(1), SOFTWARE), (id(2), INTEGRATED), (id(3), DISCRETE)];
    for (wanted, expected) in [(3, Some(0)), (2, Some(1)), (1, Some(2)), (9, None)] {
        assert_eq!(dawn_index(&listed, &id(wanted)).unwrap(), expected, "{wanted}");
    }
    assert_eq!(dawn_index(&[(id(5), VIRTUAL), (id(6), SOFTWARE)], &id(5)).unwrap(), Some(1));
}

#[test]
fn dawn_ties_not_guessed() {
    let listed = [(id(1), INTEGRATED), (id(2), DISCRETE), (id(3), DISCRETE)];
    assert!(dawn_index(&listed, &id(2)).is_err());
    assert!(dawn_index(&listed, &id(3)).is_err());
    assert_eq!(dawn_index(&listed, &id(1)).unwrap(), Some(2));
    assert_eq!(dawn_index(&listed, &id(9)).unwrap(), None);
}
