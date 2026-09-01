pub fn probe_json() -> String {
    let mut adapters = Vec::new();
    let backend = wgpu::Backends::VULKAN;
    let instance =
        wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: backend, ..Default::default() });
    for adapter in instance.enumerate_adapters(backend) {
        let info = adapter.get_info();
        let features = adapter.features();
        let limits = adapter.limits();
        adapters.push(serde_json::json!({
            "backend": format!("{:?}", info.backend),
            "name": info.name,
            "device_type": format!("{:?}", info.device_type),
            "driver": info.driver,
            "driver_info": info.driver_info,
            "bc7_thumbnails": features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            "max_texture_dim": limits.max_texture_dimension_2d,
        }));
    }
    let hardware = adapters.iter().any(|adapter| adapter["device_type"] != "Cpu");
    let compressed = adapters.iter().any(|adapter| adapter["bc7_thumbnails"] == true);
    serde_json::json!({
        "ok": !adapters.is_empty(),
        "hardware_gpu": hardware,
        "thumbnails_supported": compressed,
        "adapters": adapters,
    })
    .to_string()
}
