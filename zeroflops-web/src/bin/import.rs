use std::fs::File;

use rusqlite::Connection;
use zeroflops::{Error, List, RawList};
use zeroflops_web::{Item, RawItem};

fn main() {
    import().unwrap();
}

fn import() -> Result<(), Error> {
    let items: Vec<Item> = serde_json::from_reader(File::open("items.json").unwrap())?;
    let lists: Vec<List> = serde_json::from_reader(File::open("lists.json").unwrap())?;
    let mut conn = Connection::open("zeroflops").unwrap();
    let tx = conn.transaction().unwrap();
    for item in items {
        tx.execute(
            "INSERT INTO item (id, user_id, type, name, iframe, rating, user_score, user_wins, user_losses, metadata, hidden) VALUES (:id, :user_id, :type, :name, :iframe, :rating, :user_score, :user_wins, :user_losses, :metadata, :hidden)",
            serde_rusqlite::to_params_named(RawItem::from(item)).unwrap().to_slice().as_slice()
        )
        .unwrap();
    }
    for list in lists {
        tx.execute(
            "INSERT INTO list (id, user_id, mode, name, sources, iframe, items, favorite, query) VALUES (:id, :user_id, :mode, :name, :sources, :iframe, :items, :favorite, :query)",
            serde_rusqlite::to_params_named(RawList::from(list)).unwrap().to_slice().as_slice()
        )
        .unwrap();
    }
    tx.commit().unwrap();
    Ok(())
}
