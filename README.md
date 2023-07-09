# zeroflops
## zeroflops-web
```
COSMOS_MASTER_KEY= COSMOS_ACCOUNT= SPOTIFY_TOKEN= cargo +nightly run --features dev
```
## zeroflops-wasm
```
rustup run nightly wasm-pack build --target web
```
## TODO
### P0
- [x] Move tournament component to use grids
- [x] Fix tournament list view
- [x] Add search page
- [x] Support custom queries for lists
- [x] Support hiding items
- [x] Support deleting lists
- [ ] Debug session issues
- [ ] Audit authz
### P1
- [x] Add sort/rank page to lists
- [x] Add Google auth
- [ ] Add IMDb data source
- [x] Support user lists
- [ ] Add dedicated import page
- [ ] Add documentation
- [ ] Support resetting items
- [ ] Add Spotify search support
- [x] Add chart visualization
- [ ] Add custom tournaments
- [ ] Add item notes
- [ ] Add description
- [ ] Add lists as a data source
- [ ] Add time weighted averages
- [ ] Support data source refresh
### P2
- [ ] Add list sort mode (via rank or rating)
- [ ] Revisit data model
- [x] Fix sort mode responsiveness
- [ ] Add spinners
- [ ] Improve error handling
- [ ] Add sharing
- [ ] Add multiplayer
- [ ] Add Spotify snapshot caching 
- [ ] Add public home page
- [ ] Add CFB support 
