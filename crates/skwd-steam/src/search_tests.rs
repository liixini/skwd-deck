use super::*;

#[test]
fn query_type_codes() {
    assert!(matches!(query_type(0), UGCQueryType::RankedByVote));
    assert!(matches!(query_type(1), UGCQueryType::RankedByPublicationDate));
    assert!(matches!(query_type(9), UGCQueryType::RankedByTotalUniqueSubscriptions));
    assert!(matches!(query_type(21), UGCQueryType::RankedByLastUpdatedDate));
    assert!(matches!(query_type(3), UGCQueryType::RankedByTrend));
    assert!(matches!(query_type(777), UGCQueryType::RankedByTrend));
}

#[test]
fn native_preview_media() {
    assert_eq!(
        workshop_preview("https://images.steamusercontent.com/ugc/example".into(), 0),
        Some(WorkshopPreview::Image {
            url: "https://images.steamusercontent.com/ugc/example".into()
        })
    );
    assert_eq!(
        workshop_preview("PEZIDtt1ceA".into(), 1),
        Some(WorkshopPreview::Youtube { id: "PEZIDtt1ceA".into() })
    );
    assert_eq!(workshop_preview("javascript:alert(1)".into(), 0), None);
    assert_eq!(workshop_preview("bad/video".into(), 1), None);
    assert_eq!(workshop_preview("model".into(), 2), None);
}
