use wasm_bindgen_test::wasm_bindgen_test;
use zeroflops::{Id, Spotify};

#[wasm_bindgen_test]
fn test_parse_spotify_source() {
    assert_eq!(
        zeroflops_wasm::parse_spotify_source(String::from(
            "https://open.spotify.com/playlist/5jPjYAdQO0MgzHdwSmYPNZ?si=7d1f5dfadb654daa"
        )),
        Some(Spotify::Playlist(Id {
            id: String::from("5jPjYAdQO0MgzHdwSmYPNZ"),
            raw_id: String::from(
                "https://open.spotify.com/playlist/5jPjYAdQO0MgzHdwSmYPNZ?si=7d1f5dfadb654daa"
            )
        }))
    );
}
