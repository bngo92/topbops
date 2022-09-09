# topbops
## topbops-web
```
COSMOS_MASTER_KEY= COSMOS_ACCOUNT= SPOTIFY_TOKEN= cargo +nightly run --features dev
```
## topbops-wasm
```
rustup run nightly wasm-pack build --target web
```
## TODO
### P0
- [x] Move tournament component to use grids
- [x] Fix tournament list view
- [x] Add search page
- [ ] Support custom queries for lists
- [x] Support hiding items
- [ ] Support deleting lists
### P1
- [ ] Support user lists
- [ ] Add dedicated import page
- [ ] Add documentation
- [ ] Support resetting items
### P2
- [ ] Add sort mode
- [ ] Revisit data model
- [x] Fix sort mode responsiveness
- [ ] Add spinners
- [ ] Add Google auth
- [ ] Improve error handling
- [ ] Add sharing
