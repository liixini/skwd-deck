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
