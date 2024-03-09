use std::io::Cursor;

use arrow::{array::AsArray, compute, datatypes::UInt64Type, ipc::reader::FileReader};
use zeroflops::{Id, List, ListMode, Lists, Source, SourceType, Spotify};

#[test]
fn test_get_lists() {
    let Some(url) = get_url("/api/lists") else {
        return;
    };
    let lists: Lists = reqwest::blocking::get(url).unwrap().json().unwrap();
    let [ref bop_to_the_top, ref artists, ref winners] = lists.lists[..] else {
        unreachable!()
    };

    assert_bop_to_the_top(bop_to_the_top);
    assert_eq!(
        artists.clone(),
        List {
            user_id: "demo".to_owned(),
            mode: ListMode::View(None),
            name: "Artists".to_owned(),
            sources: Vec::new(),
            iframe: None,
            items: Vec::new(),
            favorite: true,
            query: "SELECT artists, AVG(user_score) FROM item GROUP BY artists".to_owned(),
            ..artists.clone()
        }
    );
    assert_eq!(
        winners.clone(),
        List {
            user_id: "demo".to_owned(),
            mode: ListMode::View(None),
            name: "Winners".to_owned(),
            sources: Vec::new(),
            iframe: None,
            items: Vec::new(),
            favorite: true,
            query: "SELECT name, user_score FROM item WHERE user_score >= 1500".to_owned(),
            ..winners.clone()
        }
    );
}

#[test]
fn test_get_list() {
    let Some(url) = get_url("/api/lists/5MztFbRbMpyxbVYuOSfQV9") else {
        return;
    };
    let list: List = reqwest::blocking::get(url).unwrap().json().unwrap();
    assert_bop_to_the_top(&list);
}

pub fn assert_bop_to_the_top(list: &List) {
    assert_eq!(list.user_id, "demo");
    assert_eq!(list.mode, ListMode::External);
    assert_eq!(list.name, "Bop to the Top");
    assert_eq!(
        list.sources,
        vec![Source {
            source_type: SourceType::Spotify(Spotify::Playlist(Id {
                id: "5MztFbRbMpyxbVYuOSfQV9".to_owned(),
                raw_id: "https://open.spotify.com/embed/playlist/5MztFbRbMpyxbVYuOSfQV9?utm_source=generator".to_owned()
            })),
            name: "Bop to the Top".to_owned()
        }]
    );
    assert_eq!(
        list.iframe,
        Some(
            "https://open.spotify.com/embed/playlist/5MztFbRbMpyxbVYuOSfQV9?utm_source=generator"
                .to_owned()
        )
    );
    assert_eq!(list.items.len(), 10);
    for item in &list.items {
        assert!(item.id.starts_with("spotify:track:"));
        assert!(!item.name.is_empty());
        assert!(item.iframe.is_some());
    }
    assert!(list.favorite);
    assert_eq!(list.query, "SELECT name, user_score FROM item");
}

#[test]
fn test_search() {
    let Some(url) = get_url(
        "/api/items?q=search&query=SELECT%20name,%20user_wins%20,user_losses%20FROM%20item",
    ) else {
        return;
    };
    let items = Cursor::new(reqwest::blocking::get(url).unwrap().bytes().unwrap());
    let mut reader = FileReader::try_new(items, None).unwrap();
    let arrays = reader.next().unwrap().unwrap().columns().to_vec();
    assert_eq!(arrays[0].len(), 10);
    assert!(arrays[0].as_string_opt::<i64>().is_some());
    assert_eq!(arrays[1].len(), 10);
    assert_eq!(arrays[2].len(), 10);
    assert_eq!(
        compute::sum(arrays[1].as_primitive::<UInt64Type>()),
        compute::sum(arrays[2].as_primitive::<UInt64Type>()),
    );
}

fn get_url(path: &str) -> Option<String> {
    std::env::var("TEST_URL").ok().map(|url| url + path)
}
