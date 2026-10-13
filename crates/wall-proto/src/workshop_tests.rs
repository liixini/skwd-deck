use super::workshop_id;

#[test]
fn parses_workshop_ids() {
    for input in [
        " 123 ",
        "https://steamcommunity.com/sharedfiles/filedetails/?id=123",
        "https://www.steamcommunity.com/workshop/filedetails/?id=123&appid=431960",
        "steam://url/CommunityFilePage/123",
        "steam://url/communityfilepage/123",
    ] {
        assert_eq!(workshop_id(input), Some(123), "{input}");
    }
}

#[test]
fn rejects_foreign_items() {
    for input in [
        "0",
        "18446744073709551616",
        "12abc",
        "https://evil.example/sharedfiles/filedetails/?id=123",
        "https://steamcommunity.com/workshop/filedetails/?id=123&appid=999",
        "https://steamcommunity.com/sharedfiles/filedetails/?id=0",
        "steam://url/CommunityFilePage/123/other",
        "steam://url/CommunityFilePage/123?appid=999",
    ] {
        assert_eq!(workshop_id(input), None, "{input}");
    }
}
