# amsync
Playlist synchronization for spotify and apple music. Currently only transers spotify playlists to apple music. 
## setup
* you'll need to harvest bearer token, media user token, and cookies from devtools in a logged-in browser by inspecting a request made to amp-api.music.apple.com by Apple Music Web.

* you will also need a spotify api token/integration (client id and client secret) with its redirect link set to https://localhost:8888/callback/

* all of the above should be placed in credentials.toml as described in the credentials.toml.sample given in this repo

## general usage

* in apple music, make an empty playlist, and name it `\[amsync] name of spotify playlist in your spotify library to sync`.
* `cargo run` amsync after following setup instructions
* log in with spotify

note that amsync currently does not track the songs it adds to playlists, so running it twice will result in the playlist repeating twice. 

it may also fail to match isrcs where they exist on spotify but not on apple music.

## how it works
commercially released songs typically have an ISRC or Internation Standard Recording Code associated with them. 
both spotify and apple music support searching for songs with this id as well getting the isrc of a song. with this alone, you can get something that works, but in practice there are usually many different versions or releases associated with 1 ISRC on any given platform. to find the best match, amsync gathers metadata about all possible candidates (songs with a matching isrc). it then uses levenshtein distance to compute the lexical difference between their song titles and artist names, and adds that to the number of days between the release dates of the target and the candidate to form a heuristic for evaluating how likely it is that a given Apple Music song exactly matches the desired spotify song. this heuristic is evaluated for every candidate and the best (closest) candidate is selected and queued to be added to the Apple Music playlist.