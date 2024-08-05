# Store Temporary Link

Make temporary links for storing stuff.

## Running

1. Install postgresql and start service

2. Install [shuttle](https://docs.shuttle.rs/introduction/installation)

3. Run dev server:

```sh
cargo shuttle run
```

4. Add `.env` file if want live database for compile time checks:

```
DATABASE_URL=postgres://postgres@localhost/stlink
```
