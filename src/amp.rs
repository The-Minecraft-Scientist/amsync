use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct AppleMusicCatalogSong(pub u64);
pub struct AppleMusicLibrarySong(pub String);
impl AppleMusicCatalogSong {
    pub fn from_json(obj: &Value) -> Self {
        Self(u64::from_str_radix(&obj.as_object().unwrap()["id"].as_str().unwrap(), 10).unwrap())
    }
}
#[derive(Debug, Clone)]
pub struct AppleMusicCatalogSongWithMeta {
    pub song: AppleMusicCatalogSong,
    pub meta: Metadata,
}
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct SpotifySong(pub String);
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct UnifiedSong {
    pub name: String,
    pub isrc: String,
}
#[derive(Debug, Clone)]
pub struct IsrcWithMeta {
    pub isrc: String,
    pub meta: Metadata,
}
#[derive(Debug, Clone)]
pub struct Metadata {
    pub album_name: String,
    pub release_date: Option<chrono::NaiveDate>,
}
impl Metadata {
    pub fn distance(&self, other: &Self) -> u32 {
        if self.album_name == other.album_name {
            return 0;
        }
        let album_name_distance = stringmetrics::levenshtein(&self.album_name, &other.album_name);
        album_name_distance
            + match (&self.release_date, &other.release_date) {
                (Some(sel), Some(othe)) => {
                    sel.signed_duration_since(*othe).num_days().unsigned_abs() as u32
                }
                _ => 0,
            }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct AppleMusicPlaylistId(pub String);
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct UnifiedPlaylist {
    name: String,
    songs_state: Vec<UnifiedSong>,
}
