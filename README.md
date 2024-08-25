# amsync

## setup
you'll need to harvest bearer token, media user token, and cookies from devtools in a logged-in browser by inspecting a request made to amp-api.music.apple.com by Apple Music Web.

You will also need a spotify api token/integration (client id and client secret) with its redirect link set to https://localhost:8888/callback/

All of the above should be placed in credentials.toml as described in the credentials.toml.sample given in this repo

## general usage

* in apple music, make an empty playlist, and name it "\[amsync] name of spotify playlist to sync".
* `cargo run` amsync after following setup instructions

Note that amsync currently does not track the songs it adds to playlists, so running it twice will result in the playlist repeating twice
