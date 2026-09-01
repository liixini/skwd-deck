use super::*;

#[test]
fn kind_wire_round_trip() {
    for kind in [RendererKind::Static, RendererKind::Video, RendererKind::We] {
        assert_eq!(RendererKind::from_wire(kind.wire()), kind);
    }
    assert_eq!(RendererKind::from_wire("shader"), RendererKind::Static);
    assert_eq!(RendererKind::from_wire("garbage"), RendererKind::Static);
}
