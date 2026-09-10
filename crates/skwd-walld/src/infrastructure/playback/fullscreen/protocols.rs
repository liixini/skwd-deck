#[allow(dead_code, non_camel_case_types, unused_imports, clippy::all, clippy::wildcard_imports)]
pub(super) mod workspace {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/cosmic-workspace-unstable-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/cosmic-workspace-unstable-v1.xml");
}

#[allow(dead_code, non_camel_case_types, unused_imports, clippy::all, clippy::wildcard_imports)]
pub(super) mod toplevel {
    use super::workspace::*;
    use wayland_client;
    use wayland_client::protocol::*;
    use wayland_protocols::ext::foreign_toplevel_list::v1::client::*;
    use wayland_protocols::ext::workspace::v1::client::*;
    pub mod __interfaces {
        use super::super::workspace::__interfaces::*;
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols::ext::foreign_toplevel_list::v1::client::__interfaces::*;
        use wayland_protocols::ext::workspace::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/cosmic-toplevel-info-unstable-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/cosmic-toplevel-info-unstable-v1.xml");
}
