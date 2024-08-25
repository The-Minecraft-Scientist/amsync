#![allow(unused)]
#![feature(decl_macro)]

use serde::Serialize;
use serde_json::{value::*, Value};
use spotify_rs::{
    auth::{NoVerifier, Token},
    client::Client as SpotifyClient,
    model::{playlist::SimplifiedPlaylist, CursorPage, DatePrecision, PlayableItem},
    AuthCodeClient, AuthCodeFlow, RedirectUrl,
};
use std::{collections::HashMap, fmt::format, ops::Deref, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio::sync::oneshot::channel;
mod amp;
use amp::{
    AppleMusicCatalogSong, AppleMusicLibrarySong, AppleMusicPlaylistId, IsrcWithMeta, Metadata,
    SpotifySong, UnifiedPlaylist, UnifiedSong,
};
use dotenv::var;
use futures::{lock::Mutex, stream::Next, StreamExt, TryFutureExt, TryStreamExt};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Method, RequestBuilder,
};

use warp::{reply::Response, Filter};

use crate::amp::AppleMusicCatalogSongWithMeta;

macro_rules! header {
    ($h:ident, $k:expr, $val:expr) => {
        $h.insert(
            reqwest::header::HeaderName::from_str($k).unwrap(),
            reqwest::header::HeaderValue::from_str($val).unwrap(),
        );
    };
}

pub struct AppleMusicDriver {
    client: Client,
}
pub struct SpotifyDriver(SpotifyClient<Token, AuthCodeFlow, NoVerifier>);
impl SpotifyDriver {
    pub async fn new() -> Self {
        let (mut txcode, mut rxcode) = std::sync::mpsc::sync_channel(0);
        let (txstop, rxstop) = channel::<()>();

        let client_id = var("CLIENT_ID").unwrap();
        let client_secret = var("CLIENT_SECRET").unwrap();
        let redirect_url = var("REDIRECT_URI").unwrap();

        let auth = AuthCodeFlow::new(client_id, client_secret, vec!["playlist-read-private"]);

        let (client, url) =
            AuthCodeClient::new(auth, RedirectUrl::new(redirect_url).unwrap(), true);

        let w = warp::get()
            .and(warp::path("callback"))
            .and(warp::query::<HashMap<String, String>>())
            .map(move |a: HashMap<String, String>| {
                dbg!(&a);
                txcode.send((
                    a.get("code").unwrap().to_string(),
                    a.get("state").unwrap().to_string(),
                ));
                Response::new("<body onload=\"window.close()\">".into())
            });
        webbrowser::open(url.as_str());

        let (_addr, server) =
            warp::serve(w).bind_with_graceful_shutdown(([127, 0, 0, 1], 8888), async move {
                rxstop.await.ok();
            });
        tokio::task::spawn(server);
        let c = rxcode.recv().unwrap();
        let spotify = client.authenticate(c.0, c.1).await.unwrap();
        txstop.send(());
        Self(spotify)
    }
    pub async fn get_playlists(&mut self) -> Vec<(String, String)> {
        let mut current = 0;
        let mut playlists = Vec::new();
        loop {
            let resp = self
                .0
                .current_user_playlists()
                .limit(30)
                .offset(current)
                .get()
                .await
                .unwrap();
            playlists.extend_from_slice(&resp.items);

            if resp.items.len() < 30 {
                break;
            }
            current += 30;
        }
        playlists
            .into_iter()
            .map(|f| (f.name, f.id))
            .collect::<Vec<_>>()
    }
    pub async fn isrcs_from_playlist(&mut self, playlist_id: &str) -> Vec<IsrcWithMeta> {
        let mut items = Vec::new();
        let mut current = 0;
        const PAGE_SIZE: u32 = 50;
        loop {
            let req = self
                .0
                .playlist_items(playlist_id)
                .limit(PAGE_SIZE)
                .offset(current)
                .get()
                .await;
            let Ok(resp) = req else {
                dbg!(req);
                break;
            };
            items.extend_from_slice(&resp.items);
            if resp.items.len() < PAGE_SIZE as usize {
                break;
            }
            current += PAGE_SIZE;
        }

        items
            .into_iter()
            .filter_map(|v| {
                let PlayableItem::Track(t) = v.track else {
                    return None;
                };
                let isrc = t.external_ids.isrc?;
                let album_name = t.album.name;
                let release_date = match t.album.release_date_precision {
                    DatePrecision::Year => chrono::NaiveDate::from_yo_opt(
                        t.album.release_date.as_str().parse::<i32>().unwrap(),
                        365 / 2,
                    )
                    .unwrap(),
                    DatePrecision::Month => {
                        let str = t.album.release_date;
                        let split = str.split("-").collect::<Vec<&str>>();
                        chrono::NaiveDate::from_ymd_opt(
                            split[0].parse::<i32>().unwrap(),
                            split[1].parse::<u32>().unwrap(),
                            31 / 2,
                        )
                        .unwrap()
                    }
                    DatePrecision::Day => {
                        let str = t.album.release_date;
                        let split = str.split("-").collect::<Vec<&str>>();
                        chrono::NaiveDate::from_ymd_opt(
                            split[0].parse::<i32>().unwrap(),
                            split[1].parse::<u32>().unwrap(),
                            split[2].parse::<u32>().unwrap(),
                        )
                        .unwrap()
                    }
                };
                Some(IsrcWithMeta {
                    isrc: isrc.to_uppercase(),
                    meta: Metadata {
                        album_name,
                        release_date: Some(release_date),
                    },
                })
            })
            .collect::<Vec<_>>()
    }
}

impl AppleMusicDriver {
    pub fn new() -> Self {
        let authorization = var("BEARER").unwrap();
        let media_user_token = var("MEDIA_USER_TOKEN").unwrap();
        let cookies = var("AMP_COOKIES").unwrap();

        let mut headers = HeaderMap::new();
        header!(headers, "Authorization", &authorization);
        header!(headers, "media-user-token", &media_user_token);
        header!(headers, "Cookie", &cookies);
        header!(headers, "Host", "amp-api.music.apple.com");
        header!(headers, "Accept-Encoding", "gzip, deflate, br");
        header!(headers, "Referer", "https://music.apple.com");
        header!(headers, "Origin", "https://music.apple.com");
        header!(headers, "Connection", "keep-alive");
        header!(headers, "Sec-Fetch-Dest", "empty");
        header!(headers, "Sec-Fetch-Mode", "cors");
        header!(headers, "Sec-Fetch-Site", "same-site");
        header!(headers, "TE", "trailers");

        Self {
            client: reqwest::ClientBuilder::new()
                .default_headers(headers)
                .build()
                .unwrap(),
        }
    }
    pub async fn songs_from_isrcs(&self, isrcss: &Vec<IsrcWithMeta>) -> Vec<AppleMusicCatalogSong> {
        let mut out = Vec::with_capacity(isrcss.len());
        let mut map =
            HashMap::<String, Vec<AppleMusicCatalogSongWithMeta>>::with_capacity(isrcss.len());
        for isrc in isrcss.chunks(5) {
            let mut filt_str = "".to_owned();
            for id in isrc {
                filt_str = format!("{},{}", filt_str, id.isrc);
            }
            let req = self
                .client
                .request(
                    Method::GET,
                    format!(
                        "https://amp-api.music.apple.com/v1/catalog/us/songs/?filter[isrc]={}",
                        filt_str
                    ),
                )
                .query(&[
                    ("fields[music-videos]", "id"),
                    ("fields[library-songs]", "id"),
                    ("fields[playlists]", "supportsSing"),
                    ("fields[songs]", "id,isrc,name,releaseDate,albumName"),
                    ("format[resources]", "map"),
                    ("include", "fields"),
                    ("omit", "autos"),
                ])
                .build()
                .unwrap();

            let Ok(response) = self.client.execute(req).await else {
                return vec![];
            };

            let json: Value = response.json().await.unwrap();
            let Some(songs) = json.as_object().unwrap()["resources"]
                .as_object()
                .unwrap()
                .get("songs")
            else {
                continue;
            };
            for song in songs.as_object().unwrap() {
                let catalog_song =
                    AppleMusicCatalogSong(song.1["id"].as_str().unwrap().parse::<u64>().unwrap());
                let attrs = song.1["attributes"].as_object().unwrap();
                let e = map
                    .entry(attrs["isrc"].as_str().unwrap().to_owned())
                    .or_default();
                let album_name = attrs["albumName"].as_str().unwrap().to_owned();
                let Some(ent) = attrs.get("releaseDate") else {
                    e.push(AppleMusicCatalogSongWithMeta {
                        song: catalog_song,
                        meta: Metadata {
                            album_name,
                            release_date: None,
                        },
                    });
                    continue;
                };
                let str = ent.as_str().unwrap().to_owned();

                let split = str.split("-").collect::<Vec<&str>>();
                let date = match split.len() {
                    1 => chrono::NaiveDate::from_yo_opt(split[0].parse::<i32>().unwrap(), 365 / 2)
                        .unwrap(),
                    3 => chrono::NaiveDate::from_ymd_opt(
                        split[0].parse::<i32>().unwrap(),
                        split[1].parse::<u32>().unwrap(),
                        split[2].parse::<u32>().unwrap(),
                    )
                    .unwrap(),
                    _ => {
                        panic!("malformed release date!")
                    }
                };
                e.push(AppleMusicCatalogSongWithMeta {
                    song: catalog_song,
                    meta: Metadata {
                        album_name,
                        release_date: Some(date),
                    },
                });
            }
        }
        for id in isrcss {
            let Some(v) = map.get(id.isrc.as_str()) else {
                println!("failed to match isrc: {}", id.isrc);
                continue;
            };
            let mut l = v
                .iter()
                .map(|s| (s, s.meta.distance(&id.meta)))
                .collect::<Vec<(&AppleMusicCatalogSongWithMeta, u32)>>();
            l.sort_by_key(|f| f.1);
            let Some(first) = l.first() else {
                continue;
            };
            out.push(first.0.song);
        }
        out
    }

    pub async fn add_isrcs_to_playlist(
        &self,
        playlist: AppleMusicPlaylistId,
        isrcs: &Vec<IsrcWithMeta>,
    ) {
        let idss = self.songs_from_isrcs(isrcs).await;
        for ids in idss.chunks(20) {
            let mut map = serde_json::value::Map::<String, Value>::new();
            let mut v = Vec::with_capacity(ids.len());
            map.insert("type".to_owned(), Value::String("songs".to_owned()));
            for id in ids {
                let mut entry = map.clone();
                entry.insert("id".to_owned(), Value::String(id.0.to_string()));
                v.push(Value::Object(entry));
            }
            let mut v2 = Map::<String, Value>::new();
            v2.insert("data".to_owned(), Value::Array(v));
            let json = Value::Object(v2);
            let req = self
                .client
                .post(format!(
                    "https://amp-api.music.apple.com/v1/me/library/playlists/{}/tracks",
                    playlist.0
                ))
                .body(serde_json::ser::to_string(&json).unwrap())
                .build()
                .unwrap();
            let _ = self.client.execute(req).await;
        }
    }
    pub async fn isrcs_from_playlist(&self, playlist: AppleMusicPlaylistId) -> Vec<String> {
        let mut songv = vec![];
        let req = self
            .client
            .get(format!(
                "https://amp-api.music.apple.com/v1/me/library/playlists/{}/tracks",
                playlist.0
            ))
            .query(&[
                ("fields[music-videos]", "has4K"),
                ("fields[library-songs]", "hasCredits"),
                ("fields[playlists]", "supportsSing"),
                ("fields[songs]", "isrc,name,releaseDate,albumName"),
                ("format[resources]", "map"),
                ("include", "catalog,fields"),
                ("omit", "autos"),
            ])
            .build()
            .unwrap();
        let Ok(resp) = self.client.execute(req).await else {
            return vec![];
        };
        let json: Value = resp.json().await.unwrap();
        let songs = json.as_object().unwrap()["resources"].as_object().unwrap()["songs"]
            .as_object()
            .unwrap();
        for song in songs.iter() {
            songv.push(
                song.1.as_object().unwrap()["attributes"]
                    .as_object()
                    .unwrap()["isrc"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
        }
        songv
    }
    pub async fn get_playlists_to_sync(&self) -> Vec<(String, AppleMusicPlaylistId)> {
        let mut to_change = Arc::new(Mutex::new(vec![]));
        let req = self
            .client
            .request(
                Method::GET,
                "https://amp-api.music.apple.com/v1/me/library/playlists",
            )
            .build()
            .unwrap();
        let Ok(resp) = self.client.execute(req).await else {
            return vec![];
        };
        let text = resp.text().await.unwrap();
        let json = serde_json::value::Value::from_str(&text).unwrap();
        for item in json["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_object().unwrap())
        {
            let name = item["attributes"].as_object().unwrap()["name"]
                .as_str()
                .unwrap();
            if name.contains("[amsync]") || name.contains("halloween") {
                to_change.lock().await.push((
                    item["attributes"].as_object().unwrap()["name"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    AppleMusicPlaylistId(item["id"].as_str().unwrap().to_string()),
                ))
            }
        }
        to_change.lock_owned().await.clone()
    }
    pub async fn get_latest_recently_played_song(&self) -> Option<AppleMusicCatalogSong> {
        let Ok(resp) = self
            .client
            .execute(
                self.client
                    .get("https://amp-api.music.apple.com/v1/me/recent/played/tracks")
                    .query(&[("limit", "1"), ("types", "songs")])
                    .build()
                    .unwrap(),
            )
            .await
        else {
            return None;
        };
        let body = resp.text().await.unwrap();
        let jval = serde_json::Value::from_str(&body).unwrap();
        let song = jval.as_object().unwrap()["data"].as_array().unwrap();
        Some(AppleMusicCatalogSong::from_json(&song[0]))
    }
}

impl Default for AppleMusicDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[tokio::main]
async fn main() {
    dotenv::from_filename("credentials.env");

    let amd = AppleMusicDriver::new();
    let mut spd = SpotifyDriver::new().await;
    let mut map = HashMap::new();
    spd.get_playlists().await.into_iter().for_each(|v| {
        map.insert(v.0.trim().to_string(), v);
    });
    let amplaylistids = amd.get_playlists_to_sync().await;

    for appleplaylist in amplaylistids {
        let start = std::time::Instant::now();
        if let Some(playlist) = map.get(appleplaylist.0.replace("[amsync]", "").trim()) {
            let isrcs = spd.isrcs_from_playlist(&playlist.1).await;
            println!(
                "adding songs from spotify playlist {} ({}) to apple music playlist {}",
                playlist.1, playlist.0, &appleplaylist.0,
            );

            amd.add_isrcs_to_playlist(appleplaylist.1.clone(), &isrcs)
                .await;
        }
    }
}
